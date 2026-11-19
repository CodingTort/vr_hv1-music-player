// ── Synced lyrics ─────────────────────────────────────────────────────────────
//
// Lookup order for a track:
//   1. `<song>.lrc` next to the audio file — drop one in to override anything
//   2. lyrics embedded in the file's tags (ID3 USLT, Vorbis LYRICS, MP4 ©lyr)
//   3. ~/.cache/lavanda/lyrics/ — results of earlier online lookups
//   4. LRCLIB (https://lrclib.net), a free community database of synced
//      lyrics. Like youtube.rs this shells out (to curl), so lavanda itself
//      still has no networking code. Turn it off with `lyrics_online = false`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::library::models::Track;

const LRCLIB: &str = "https://lrclib.net/api";

/// LRCLIB asks clients to identify themselves with name, version and homepage.
const USER_AGENT: &str = concat!(
    "lavanda v",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/sheep-farm/lavanda)"
);

/// Synced lyrics are only trusted when their duration matches the file this
/// closely. Otherwise the timestamps belong to a different cut of the song
/// (a music video with an intro, a radio edit…) and lines would light up at
/// the wrong moments, so the words are shown unsynced instead.
const MAX_DURATION_DIFF: f64 = 3.0;

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct LyricLine {
    /// When the line starts. Always zero for unsynced lyrics.
    pub time: Duration,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lyrics {
    /// Timestamped lines, sorted by time.
    Synced(Vec<LyricLine>),
    /// Words without timing.
    Plain(Vec<LyricLine>),
    Instrumental,
    NotFound,
    /// The online lookup failed (offline, curl missing…). Retried next time.
    Error(String),
}

/// What a lookup needs to know about a track.
#[derive(Debug, Clone)]
pub struct Query {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub duration: Duration,
}

impl From<&Track> for Query {
    fn from(t: &Track) -> Self {
        Query {
            path: t.path.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            duration: t.duration,
        }
    }
}

/// Finds lyrics for a track, trying local sources before going online.
pub async fn load(q: Query) -> Lyrics {
    let local = q.clone();
    // File reads and tag parsing block, so keep them off the async executor.
    let online = crate::config::get().lyrics_online;
    let local = tokio::task::spawn_blocking(move || load_local(&local)).await.ok().flatten();
    // A cached upload with a translation woven in is looked up again (once:
    // the result replaces it), since a clean one may exist.
    let stale = match &local {
        Some(Local::Cached(r)) => online && r.interleaved() && !r.rechecked,
        _ => false,
    };
    match local {
        Some(Local::File(found)) => return found,
        Some(Local::Cached(r)) if !stale => return r.into_lyrics(),
        _ => {}
    }
    if !online {
        return Lyrics::NotFound;
    }
    match fetch_lrclib(&q).await {
        Ok(Some(mut record)) => {
            record.rechecked = record.interleaved();
            store_cache(&q.path, &record);
            record.into_lyrics()
        }
        Ok(None) => Lyrics::NotFound,
        Err(e) => Lyrics::Error(e),
    }
}

enum Local {
    /// A sidecar .lrc or embedded lyrics.
    File(Lyrics),
    /// A previous LRCLIB result.
    Cached(Record),
}

fn load_local(q: &Query) -> Option<Local> {
    if let Some(found) = std::fs::read_to_string(q.path.with_extension("lrc"))
        .ok()
        .and_then(|text| from_text(&text))
    {
        return Some(Local::File(found));
    }
    if let Some(found) = embedded_lyrics(&q.path).and_then(|text| from_text(&text)) {
        return Some(Local::File(found));
    }
    let cached = std::fs::read_to_string(cache_path(&q.path)).ok()?;
    serde_json::from_str::<Record>(&cached).ok().map(Local::Cached)
}

fn embedded_lyrics(path: &Path) -> Option<String> {
    use lofty::prelude::*;
    let tagged = lofty::probe::Probe::open(path).ok()?.read().ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    tag.get_string(&ItemKey::Lyrics).map(str::to_owned)
}

/// Reads lyrics that may or may not be in LRC format.
pub fn from_text(text: &str) -> Option<Lyrics> {
    if let Some(lines) = parse_lrc(text) {
        return Some(Lyrics::Synced(lines));
    }
    plain_lines(text).map(Lyrics::Plain)
}

fn plain_lines(text: &str) -> Option<Vec<LyricLine>> {
    let mut lines: Vec<LyricLine> = Vec::new();
    for raw in text.lines() {
        let text = raw.trim().to_string();
        // Keep single blank lines between stanzas, drop runs of them.
        if text.is_empty() && lines.last().map_or(true, |l| l.text.is_empty()) {
            continue;
        }
        lines.push(LyricLine { time: Duration::ZERO, text });
    }
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    (!lines.is_empty()).then_some(lines)
}

// ── LRC ───────────────────────────────────────────────────────────────────────

/// Parses LRC text into lines sorted by time. `None` if nothing is timestamped.
///
/// Handles several timestamps on one line (`[00:12.00][01:30.00]chorus`), the
/// `[offset:±ms]` tag, and enhanced-LRC word timings (`<00:18.89>word`), which
/// LRCLIB now includes and which are stripped for line-by-line display.
pub fn parse_lrc(text: &str) -> Option<Vec<LyricLine>> {
    let mut offset_ms: i64 = 0;
    let mut lines = Vec::new();

    for raw in text.lines() {
        let mut rest = raw.trim_start_matches('\u{feff}').trim();
        let mut stamps = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(end) = inner.find(']') else { break };
            let Some(time) = parse_timestamp(&inner[..end]) else {
                // A metadata line ([ar:…], [offset:…]) or bracketed lyric text.
                if stamps.is_empty() {
                    if let Some(v) = inner[..end].strip_prefix("offset:") {
                        offset_ms = v.trim().parse().unwrap_or(0);
                    }
                }
                break;
            };
            stamps.push(time);
            rest = inner[end + 1..].trim_start();
        }
        if stamps.is_empty() {
            continue;
        }
        let text = strip_word_stamps(rest);
        lines.extend(stamps.into_iter().map(|time| LyricLine { time, text: text.clone() }));
    }

    if lines.is_empty() {
        return None;
    }
    if offset_ms != 0 {
        // A positive offset makes lyrics appear sooner.
        for l in &mut lines {
            let ms = l.time.as_millis() as i64 - offset_ms;
            l.time = Duration::from_millis(ms.max(0) as u64);
        }
    }
    // Stable, so lines sharing a timestamp keep their order in the file.
    lines.sort_by_key(|l| l.time);
    Some(lines)
}

/// `mm:ss`, `mm:ss.xx`, `mm:ss.xxx` or `mm:ss:xx`.
fn parse_timestamp(tag: &str) -> Option<Duration> {
    let (min, sec) = tag.split_once(':')?;
    let min: u64 = min.trim().parse().ok()?;
    let sec: f64 = sec.trim().replacen(':', ".", 1).parse().ok()?;
    (sec.is_finite() && sec >= 0.0).then(|| Duration::from_secs_f64(min as f64 * 60.0 + sec))
}

/// `<00:18.89>Paper <00:19.25>boats` → `Paper boats`.
fn strip_word_stamps(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('>') {
            Some(end) if parse_timestamp(&after[..end]).is_some() => rest = &after[end + 1..],
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

// ── LRCLIB ────────────────────────────────────────────────────────────────────

/// An LRCLIB record. Also the on-disk cache format, so cached lookups go
/// through exactly the same conversion as fresh ones.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Record {
    track_name: Option<String>,
    artist_name: Option<String>,
    duration: Option<f64>,
    instrumental: Option<bool>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
    /// Ours, not LRCLIB's: an interleaved upload kept because a fresh
    /// lookup found nothing cleaner, so it isn't looked up on every play.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    rechecked: bool,
}

impl Record {
    /// Whether the synced lyrics weave a second line (usually a translation)
    /// in at the same timestamps as the sung ones, for a fifth or more of
    /// their timestamps.
    fn interleaved(&self) -> bool {
        let Some(lines) = self.synced_lyrics.as_deref().and_then(parse_lrc) else { return false };
        let mut times: Vec<Duration> = lines.iter().filter(|l| !l.text.trim().is_empty()).map(|l| l.time).collect();
        let total = times.len();
        times.dedup();
        total > 0 && (total - times.len()) * 5 >= times.len()
    }

    fn into_lyrics(self) -> Lyrics {
        if let Some(lines) = self.synced_lyrics.as_deref().and_then(parse_lrc) {
            return Lyrics::Synced(lines);
        }
        if let Some(lines) = self.plain_lyrics.as_deref().and_then(plain_lines) {
            return Lyrics::Plain(lines);
        }
        if self.instrumental == Some(true) {
            Lyrics::Instrumental
        } else {
            Lyrics::NotFound
        }
    }
}

async fn fetch_lrclib(q: &Query) -> Result<Option<Record>, String> {
    let secs = q.duration.as_secs_f64();

    // 1. The exact signature — what properly tagged files hit.
    let duration = format!("{}", secs.round() as u64);
    let mut params = vec![("track_name", q.title.as_str()), ("artist_name", q.artist.as_str())];
    if secs > 0.0 {
        params.push(("duration", duration.as_str()));
    }
    let mut interleaved_match = None;
    if let Some(body) = lrclib_get("get", &params).await? {
        let record: Record =
            serde_json::from_str(&body).map_err(|e| format!("unexpected LRCLIB response: {e}"))?;
        let record = check_timing(record, secs);
        if !record.interleaved() {
            return Ok(Some(record));
        }
        // A translation woven in on the same timestamps: look for a clean
        // upload first, and use this one only if there is none.
        interleaved_match = Some(record);
    }

    // 2. A search with the title cleaned up, for tags copied from YouTube
    //    such as "【Ado】患者A (Patient A)".
    let (artist, title) = search_terms(&q.title, &q.artist);
    let query = format!("{artist} {title}");
    let Some(body) = lrclib_get("search", &[("q", query.trim())]).await? else {
        return Ok(interleaved_match);
    };
    let candidates: Vec<Record> =
        serde_json::from_str(&body).map_err(|e| format!("unexpected LRCLIB response: {e}"))?;
    let found = pick(candidates, secs, &title).map(|r| check_timing(r, secs));
    if found.as_ref().is_some_and(|r| r.synced_lyrics.is_some() && !r.interleaved()) {
        return Ok(found);
    }
    if interleaved_match.is_some() {
        return Ok(interleaved_match);
    }
    if found.as_ref().is_some_and(|r| r.synced_lyrics.is_some()) {
        return Ok(found);
    }

    // 3. The song's name alone. Covers are often filed only under the
    //    original artist (Ado's "Overdose" is natori's song). This search
    //    returns other songs sharing the name, so `pick_by_name` is strict.
    let japanese = has_japanese(&format!("{} {}", q.title, q.artist));
    if let Some(body) = lrclib_get("search", &[("track_name", title.as_str())]).await? {
        let candidates: Vec<Record> =
            serde_json::from_str(&body).map_err(|e| format!("unexpected LRCLIB response: {e}"))?;
        if let Some(r) = pick_by_name(candidates, secs, &title, japanese) {
            return Ok(Some(check_timing(r, secs)));
        }
    }
    Ok(found)
}

/// For a search by song name alone, which also finds unrelated songs with
/// the same name: only the exact name, with synced lyrics timed within a few
/// seconds of the file, and for a Japanese track lyrics in Japanese or in
/// romaji (an English "Overdose" of the same length is not Ado's).
fn pick_by_name(candidates: Vec<Record>, secs: f64, title: &str, japanese: bool) -> Option<Record> {
    let want = normalize(title);
    let diff = |r: &Record| (r.duration.unwrap_or(f64::MAX) - secs).abs();
    candidates
        .into_iter()
        .filter(|r| {
            let synced = r.synced_lyrics.as_deref().filter(|s| !s.trim().is_empty());
            !want.is_empty()
                && normalize(r.track_name.as_deref().unwrap_or("")) == want
                && diff(r) <= MAX_DURATION_DIFF
                && synced.is_some_and(|s| !japanese || has_japanese(s) || parse_lrc(s).is_some_and(|l| lines_look_like_romaji(&l)))
        })
        .min_by(|a, b| diff(a).total_cmp(&diff(b)))
}

/// Drops synced lyrics that run past the end of the track. LRCLIB's stated
/// durations are not always right, but lines timed beyond the end of the file
/// were certainly timed for a longer cut and would light up at the wrong
/// moments, so only the words are kept.
fn check_timing(mut record: Record, secs: f64) -> Record {
    let last = record.synced_lyrics.as_deref().and_then(parse_lrc).and_then(|l| l.last().map(|l| l.time));
    if secs > 0.0 && last.is_some_and(|t| t.as_secs_f64() > secs + MAX_DURATION_DIFF) {
        record.synced_lyrics = None;
    }
    record
}

/// Chooses among search results: synced lyrics whose duration fits the file,
/// else the words alone from a result that is plausibly the same song.
fn pick(candidates: Vec<Record>, secs: f64, title: &str) -> Option<Record> {
    let diff = |r: &Record| (r.duration.unwrap_or(f64::MAX) - secs).abs();
    let has = |s: &Option<String>| s.as_deref().is_some_and(|s| !s.trim().is_empty());

    // Closest length, but an upload with a translation woven in only when
    // there is no clean one.
    let synced = candidates
        .iter()
        .filter(|r| has(&r.synced_lyrics) && diff(r) <= MAX_DURATION_DIFF)
        .min_by(|a, b| a.interleaved().cmp(&b.interleaved()).then(diff(a).total_cmp(&diff(b))));
    if let Some(r) = synced {
        return Some(r.clone());
    }

    let want = normalize(title);
    let same_song = |r: &Record| {
        let name = normalize(r.track_name.as_deref().unwrap_or(""));
        !want.is_empty() && !name.is_empty() && (name.contains(&want) || want.contains(&name))
    };
    candidates
        .into_iter()
        .filter(|r| has(&r.plain_lyrics) && same_song(r))
        .min_by(|a, b| diff(a).total_cmp(&diff(b)))
        .map(|r| Record { synced_lyrics: None, ..r })
}

/// GETs `{LRCLIB}/{endpoint}` with URL-encoded query parameters.
/// `Ok(None)` means 404 (no such track).
async fn lrclib_get(endpoint: &str, params: &[(&str, &str)]) -> Result<Option<String>, String> {
    let mut cmd = tokio::process::Command::new("curl");
    cmd.args(["--silent", "--show-error", "--get", "--max-time", "15"])
        .args(["--user-agent", USER_AGENT, "--write-out", "\n%{http_code}"]);
    for (key, value) in params {
        cmd.arg("--data-urlencode").arg(format!("{key}={value}"));
    }
    let out = cmd
        .arg(format!("{LRCLIB}/{endpoint}"))
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("could not run curl: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.trim().trim_start_matches("curl: ").to_string());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let (body, status) = stdout.rsplit_once('\n').unwrap_or(("", &stdout[..]));
    match status.trim() {
        "200" => Ok(Some(body.to_string())),
        "404" => Ok(None),
        code => Err(format!("LRCLIB answered HTTP {code}")),
    }
}

// ── Search terms (for the fuzzy fallback) ────────────────────────────────────

/// Works out the (artist, song) to search for from tags that may come from
/// YouTube, where the artist tag is the channel and the title is decorated:
/// `米津玄師 MV「Lemon」` → `Lemon`, `【Ado】患者A (Patient A)` → `患者A`, and
/// `Rick Astley - Never Gonna Give You Up (Official Video)` by `RickAstleyVEVO`
/// → `Rick Astley`, `Never Gonna Give You Up`.
fn search_terms(title: &str, artist: &str) -> (String, String) {
    let artist = artist.trim();
    let artist = artist.strip_suffix(" - Topic").unwrap_or(artist);
    let mut artist = artist.strip_suffix("VEVO").unwrap_or(artist).trim().to_string();

    // Japanese corner brackets usually wrap the song name itself.
    for (open, close) in [('「', '」'), ('『', '』')] {
        let inner = title
            .split_once(open)
            .and_then(|(_, rest)| rest.split_once(close))
            .map(|(inner, _)| inner.trim());
        if let Some(song) = inner.filter(|s| !s.is_empty()) {
            return (artist, song.to_string());
        }
    }

    // Drop bracketed tags: 【MV】, [Official Video], (Patient A), （…）.
    let mut kept = String::new();
    let mut depth = 0u32;
    for c in title.chars() {
        match c {
            '(' | '[' | '【' | '（' | '［' => depth += 1,
            ')' | ']' | '】' | '）' | '］' => depth = depth.saturating_sub(1),
            _ if depth == 0 => kept.push(c),
            _ => {}
        }
    }

    // "Artist - Title": keep the title's spelling of the artist, which beats
    // a channel name like "RickAstleyVEVO".
    let mut song = kept.trim();
    for sep in [" - ", " – ", " — "] {
        if let Some((left, right)) = song.split_once(sep) {
            if normalize(left) == normalize(&artist) {
                artist = left.trim().to_string();
                song = right.trim();
                break;
            }
        }
    }
    // "Title feat. Someone" → "Title". The markers are ASCII, so the byte
    // index found is always a char boundary.
    for marker in [" feat. ", " ft. ", " featuring ", " cover", " covered by "] {
        // Also drops "Song - Cover" / "Song covered by Someone".
        let hit = song.as_bytes().windows(marker.len()).position(|w| w.eq_ignore_ascii_case(marker.as_bytes()));
        if let Some(i) = hit {
            song = &song[..i];
        }
    }
    // Cover labels: Ado's 歌いました ("I sang it") and the usual 歌ってみた.
    let song = song.replace("歌いました", " ").replace("歌ってみた", " ");
    let song = song.trim().trim_end_matches(['-', '–', '—', '/']).trim();

    let song = song.split_whitespace().collect::<Vec<_>>().join(" ");
    if song.is_empty() {
        (artist, title.trim().to_string())
    } else {
        (artist, song)
    }
}

/// Lowercase letters and digits only, for loose comparisons.
fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

// ── Japanese ⇄ romaji ─────────────────────────────────────────────────────────

fn is_japanese_char(c: char) -> bool {
    // Hiragana, katakana, CJK ideographs (incl. extension A), half-width kana.
    matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{ff66}'..='\u{ff9f}')
}

/// Whether text is written in Japanese script: at least 15% of its letters
/// are kana or kanji (lyrics often mix in English words).
pub fn has_japanese(text: &str) -> bool {
    let (mut letters, mut japanese) = (0usize, 0usize);
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        letters += 1;
        japanese += is_japanese_char(c) as usize;
    }
    letters > 0 && japanese * 100 >= letters * 15
}

pub fn lines_have_japanese(lines: &[LyricLine]) -> bool {
    has_japanese(&lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" "))
}

/// Whether lyrics are Japanese written in romaji: most words split cleanly
/// into Japanese syllables ("ai-shi-te", "ha-ru-ka"), which English words
/// almost never do ("fo-re-ve-r" is left with an "r").
pub fn lines_look_like_romaji(lines: &[LyricLine]) -> bool {
    let words: Vec<String> = lines
        .iter()
        .flat_map(|l| l.text.split(|c: char| !c.is_alphabetic()))
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect();
    if words.len() < 10 {
        return false;
    }
    let syllabic = words.iter().filter(|w| is_romaji_word(w)).count();
    syllabic * 100 >= words.len() * 75
}

/// Hepburn syllables: an optional consonant (or "ch"/"sh"/"ts"), an optional
/// "y", then a vowel; a syllable-final "n"; or the first half of a doubled
/// consonant ("kk", "tt", "tch").
fn is_romaji_word(word: &str) -> bool {
    const VOWELS: &str = "aiueoāīūēō";
    const CONSONANTS: &str = "kgsztdnhbpmrwfjy";
    let c: Vec<char> = word.chars().collect();
    let vowel = |i: usize| c.get(i).is_some_and(|ch| VOWELS.contains(*ch));
    let mut i = 0;
    while i < c.len() {
        if vowel(i) {
            i += 1;
            continue;
        }
        // Syllable-final ん: "n" not followed by a vowel or "y".
        if c[i] == 'n' && !vowel(i + 1) && c.get(i + 1) != Some(&'y') {
            i += 1;
            continue;
        }
        // Doubled consonant (っ): "kk", "ss", "tt", "pp", and "tch".
        if c.get(i + 1) == Some(&c[i]) && "kstpgdbzjfhm".contains(c[i])
            || (c[i] == 't' && c.get(i + 1) == Some(&'c'))
        {
            i += 1;
            continue;
        }
        let mut j = i;
        let pair: String = c[i..(i + 2).min(c.len())].iter().collect();
        if ["ch", "sh", "ts"].contains(&pair.as_str()) {
            j += 2;
        } else if CONSONANTS.contains(c[i]) {
            j += 1;
        } else {
            return false;
        }
        if c.get(j) == Some(&'y') && c[i] != 'y' {
            j += 1;
        }
        if !vowel(j) {
            return false;
        }
        i = j + 1;
    }
    true
}

/// Romaji for Japanese lyrics, keeping each line's timing. Uses the `kakasi`
/// command (Arch: `sudo pacman -S kakasi`), the same way yt-dlp and curl are
/// used, so lavanda carries no dictionary of its own.
pub async fn romanize(lines: Vec<LyricLine>) -> Result<Vec<LyricLine>, String> {
    use tokio::io::AsyncWriteExt;

    let mut child = tokio::process::Command::new("kakasi")
        // Kanji, hiragana, katakana and full-width symbols to ASCII, with
        // spaces between words.
        .args(["-iutf8", "-outf8", "-Ja", "-Ha", "-Ka", "-Ea", "-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                "Romaji needs kakasi — install it with: sudo pacman -S kakasi".to_string()
            }
            _ => format!("could not run kakasi: {e}"),
        })?;

    let input = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n") + "\n";
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).await.map_err(|e| format!("kakasi: {e}"))?;
    } // dropped here: kakasi sees end of input
    let out = child.wait_with_output().await.map_err(|e| format!("kakasi: {e}"))?;
    if !out.status.success() {
        return Err(format!("kakasi failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut romaji = text.split('\n');
    Ok(lines
        .into_iter()
        .map(|l| LyricLine { time: l.time, text: spoken_particles(romaji.next().unwrap_or("")) })
        .collect())
}

// ── Furigana ──────────────────────────────────────────────────────────────────

/// A piece of a lyric line; kanji pieces carry their reading (furigana).
#[derive(Debug, Clone, PartialEq, Hash)]
pub struct Ruby {
    pub text: String,
    pub reading: Option<String>,
}

fn is_kanji(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '々' | 'ヶ')
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30ff}')
}

/// Whether lines are Japanese with kanji to annotate (kana rules out
/// Chinese, which kakasi would read the Japanese way).
pub fn wants_furigana(lines: &[LyricLine]) -> bool {
    let text: String = lines.iter().map(|l| l.text.as_str()).collect();
    text.chars().any(is_kanji) && text.chars().any(is_kana)
}

/// Furigana for each line, from kakasi's furigana mode. A line comes back as
/// a single plain piece when its reading can't be lined up with the text:
/// the Japanese itself is never changed, at worst a reading is missing.
pub async fn furigana(lines: Vec<String>) -> Result<Vec<Vec<Ruby>>, String> {
    use tokio::io::AsyncWriteExt;

    // kakasi puts spaces between words (-s), so the lyrics' own spaces can't
    // go through it: each space-separated part goes in as its own line, and
    // the parts are joined back with spaces.
    let parts: Vec<Vec<&str>> = lines.iter().map(|l| l.split(' ').collect()).collect();
    let input = parts.iter().flatten().copied().collect::<Vec<_>>().join("\n") + "\n";

    let mut child = tokio::process::Command::new("kakasi")
        .args(["-iutf8", "-outf8", "-JH", "-f", "-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not run kakasi: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).await.map_err(|e| format!("kakasi: {e}"))?;
    }
    let out = child.wait_with_output().await.map_err(|e| format!("kakasi: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut annotated = text.split('\n');

    Ok(lines
        .iter()
        .zip(&parts)
        .map(|(line, line_parts)| {
            let mut pieces: Vec<Ruby> = Vec::new();
            for (i, _) in line_parts.iter().enumerate() {
                if i > 0 {
                    pieces.push(Ruby { text: " ".into(), reading: None });
                }
                let words = annotated.next().unwrap_or("");
                pieces.extend(words.split(' ').filter(|w| !w.is_empty()).flat_map(word_pieces));
            }
            let rebuilt: String = pieces.iter().map(|p| p.text.as_str()).collect();
            if rebuilt == *line { merge_plain(pieces) } else { vec![Ruby { text: line.clone(), reading: None }] }
        })
        .collect())
}

/// One kakasi word: `夜空[よぞら]`, `光る[ひかる]`, or plain `が`.
fn word_pieces(word: &str) -> Vec<Ruby> {
    let Some((base, rest)) = word.split_once('[') else {
        return vec![Ruby { text: word.to_string(), reading: None }];
    };
    let Some(reading) = rest.strip_suffix(']') else {
        return vec![Ruby { text: word.to_string(), reading: None }];
    };
    // kakasi reads 君 as くん; in lyrics it is nearly always きみ.
    let reading = if base == "君" { "きみ" } else { reading };
    align(base, reading)
}

/// Splits a reading over the kanji of `base`, using its kana as anchors:
/// `光る`+`ひかる` → 光(ひか) る; `取り出す`+`とりだす` → 取(と) り 出(だ) す.
fn align(base: &str, reading: &str) -> Vec<Ruby> {
    let hira = |c: char| match c {
        'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        c => c,
    };
    let mut runs: Vec<(bool, String)> = Vec::new();
    for c in base.chars() {
        let kanji = is_kanji(c);
        match runs.last_mut() {
            Some((k, s)) if *k == kanji => s.push(c),
            _ => runs.push((kanji, c.to_string())),
        }
    }
    let r: Vec<char> = reading.chars().map(hira).collect();
    let unaligned = || vec![Ruby { text: base.to_string(), reading: None }];

    let mut pieces = Vec::new();
    let mut pos = 0;
    for (i, (kanji, text)) in runs.iter().enumerate() {
        if !kanji {
            let kana: Vec<char> = text.chars().map(hira).collect();
            if r.get(pos..pos + kana.len()) != Some(&kana[..]) {
                return unaligned();
            }
            pos += kana.len();
            pieces.push(Ruby { text: text.clone(), reading: None });
            continue;
        }
        let end = match runs.get(i + 1) {
            None => r.len(),
            Some((_, next)) => {
                let anchor: Vec<char> = next.chars().map(hira).collect();
                match (pos + 1..=r.len().saturating_sub(anchor.len())).find(|&j| r[j..j + anchor.len()] == anchor[..]) {
                    Some(j) => j,
                    None => return unaligned(),
                }
            }
        };
        if end <= pos {
            return unaligned();
        }
        pieces.push(Ruby { text: text.clone(), reading: Some(r[pos..end].iter().collect()) });
        pos = end;
    }
    if pos == r.len() { pieces } else { unaligned() }
}

/// Joins neighbouring pieces without readings.
fn merge_plain(pieces: Vec<Ruby>) -> Vec<Ruby> {
    let mut out: Vec<Ruby> = Vec::new();
    for p in pieces {
        match out.last_mut() {
            Some(last) if last.reading.is_none() && p.reading.is_none() => last.text.push_str(&p.text),
            _ => out.push(p),
        }
    }
    out
}

/// Romaji for Japanese lyrics. People's romaji beats kakasi's (which now and
/// then drops a conjugated kanji, and runs kana words together), so a romaji
/// upload on LRCLIB that fits the track wins; otherwise the kakasi version.
pub async fn romaji(q: Query, lines: Vec<LyricLine>, synced: bool) -> Lyrics {
    match romanize(lines).await {
        Ok(ours) => {
            if crate::config::get().lyrics_online {
                if let Some(found) = find_romaji(&q, &ours).await {
                    // The upload has its own uploader's timing; keep the
                    // timing the Japanese was shown with and take only its
                    // words, so switching never moves the highlight.
                    return match found {
                        Lyrics::Synced(theirs) | Lyrics::Plain(theirs) if synced => {
                            let romaji: Vec<String> = theirs.iter().map(|l| l.text.clone()).collect();
                            Lyrics::Synced(retime(&ours, &ours, &theirs, &romaji))
                        }
                        other => other,
                    };
                }
            }
            if synced { Lyrics::Synced(ours) } else { Lyrics::Plain(ours) }
        }
        Err(e) => Lyrics::Error(e),
    }
}

/// The Japanese version of lyrics that came in romaji, on the romaji's
/// timing (see `romaji`): the switch changes the words, never when they come.
pub async fn japanese(q: Query, lines: Vec<LyricLine>, synced: bool) -> Lyrics {
    let found = find_japanese(q).await;
    let (Lyrics::Synced(theirs), true) = (&found, synced) else { return found };
    // Lining the lines up needs both sides in romaji; without kakasi the
    // upload is shown as it is.
    let Ok(theirs_romaji) = romanize(theirs.clone()).await else { return found };
    let romaji: Vec<String> = theirs_romaji.into_iter().map(|l| l.text).collect();
    Lyrics::Synced(retime(&lines, &lines, theirs, &romaji))
}

/// `primary`'s timing with `alt`'s words: each primary line takes the text of
/// the alt line it matches, comparing both in romaji (`primary_romaji` and
/// `alt_romaji` are the same lines romanised), in order, so a repeated chorus
/// pairs up chorus by chorus. A primary line without a counterpart keeps
/// `fallback`'s line (the same timing, so nothing drifts); alt lines without
/// one are dropped, as there is no time to show them at.
pub fn retime(primary: &[LyricLine], fallback: &[LyricLine], alt: &[LyricLine], alt_romaji: &[String]) -> Vec<LyricLine> {
    let ours: Vec<_> = fallback.iter().map(|l| line_grams(&l.text)).collect();
    let theirs: Vec<_> = alt_romaji.iter().map(|t| line_grams(t)).collect();
    let score = |i: usize, j: usize| {
        let (a, b) = (&ours[i], &theirs[j]);
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }
        let s = 2.0 * a.intersection(b).count() as f32 / (a.len() + b.len()) as f32;
        if s >= 0.35 { s } else { 0.0 }
    };
    // Best in-order pairing (like a diff, weighted by similarity).
    let (n, m) = (primary.len(), alt.len().min(alt_romaji.len()));
    let mut best = vec![vec![0.0f32; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            let pair = match score(i - 1, j - 1) { s if s > 0.0 => best[i - 1][j - 1] + s, _ => 0.0 };
            best[i][j] = best[i - 1][j].max(best[i][j - 1]).max(pair);
        }
    }
    let mut text: Vec<Option<&str>> = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        let s = score(i - 1, j - 1);
        if s > 0.0 && best[i][j] == best[i - 1][j - 1] + s {
            text[i - 1] = Some(&alt[j - 1].text);
            i -= 1;
            j -= 1;
        } else if best[i][j] == best[i - 1][j] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    primary
        .iter()
        .zip(fallback)
        .zip(text)
        .map(|((p, f), t)| LyricLine { time: p.time, text: t.unwrap_or(&f.text).to_string() })
        .collect()
}

/// Every 3-letter run of one line's romaji, normalised like `romaji_grams`.
fn line_grams(text: &str) -> std::collections::HashSet<String> {
    let chars: Vec<char> = romaji_letters(text).chars().collect();
    chars.windows(3).map(|w| w.iter().collect()).collect()
}

/// A synced romaji upload for this track, checked against `ours` (kakasi's
/// rough romaji of the same lyrics) so that an English translation, or
/// another song of the same length, is never taken for it. Cached on disk.
async fn find_romaji(q: &Query, ours: &[LyricLine]) -> Option<Lyrics> {
    let cache = cache_file(&q.path, "romaji");
    if let Some(r) = std::fs::read_to_string(&cache).ok().and_then(|s| serde_json::from_str::<Record>(&s).ok()) {
        return Some(r.into_lyrics());
    }
    let secs = q.duration.as_secs_f64();
    let (artist, title) = search_terms(&q.title, &q.artist);
    // Romaji uploads are usually titled in romaji, so also ask with kakasi's
    // reading of the title.
    let mut queries = vec![format!("{artist} {title}")];
    if has_japanese(&format!("{artist} {title}")) {
        let names = vec![
            LyricLine { time: Duration::ZERO, text: artist.clone() },
            LyricLine { time: Duration::ZERO, text: title.clone() },
        ];
        if let Ok(r) = romanize(names).await {
            queries.push(format!("{} {}", r[0].text, r[1].text));
        }
    }
    let reference = romaji_grams(&ours.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"));
    let mut candidates: Vec<Record> = Vec::new();
    for query in &queries {
        if let Ok(Some(body)) = lrclib_get("search", &[("q", query.trim())]).await {
            candidates.extend(serde_json::from_str::<Vec<Record>>(&body).unwrap_or_default());
        }
    }
    let romaji = candidates
        .into_iter()
        .filter(|r| {
            r.synced_lyrics.as_deref().is_some_and(|s| {
                !has_japanese(s) && shared_share(&reference, &romaji_grams(s)) >= 0.3
            })
        })
        .collect();
    let found = check_timing(pick(romaji, secs, "")?, secs);
    found.synced_lyrics.as_ref()?;
    store(&cache, &found);
    Some(found.into_lyrics())
}

/// Every 5-letter run of the romaji, ignoring spaces and spelling variants
/// ("tōi", "toui" and "tooi" all read "toi"), so kakasi's run-together words
/// still match a person's spaced-out ones.
fn romaji_grams(text: &str) -> std::collections::HashSet<String> {
    let chars: Vec<char> = romaji_letters(text).chars().collect();
    chars.windows(5).map(|w| w.iter().collect()).collect()
}

/// Lowercase letters only, LRC timestamps dropped, long vowels shortened.
fn romaji_letters(text: &str) -> String {
    let letters: String = text
        .lines()
        .map(|l| l.rsplit(']').next().unwrap_or(l))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .map(|c| match c { 'ā' => 'a', 'ī' => 'i', 'ū' => 'u', 'ē' => 'e', 'ō' => 'o', c => c })
        .collect();
    ["ou", "oo", "uu", "aa", "ii", "ee"]
        .iter()
        .fold(letters, |t, long| t.replace(long, &long[..1]))
}

/// Share of `reference`'s fragments that also appear in `other`.
fn shared_share(reference: &std::collections::HashSet<String>, other: &std::collections::HashSet<String>) -> f32 {
    if reference.is_empty() {
        return 0.0;
    }
    reference.intersection(other).count() as f32 / reference.len() as f32
}

/// kakasi spells the particles は and を as written ("ha", "wo"); lyrics
/// romaji spells them as sung ("wa", "o"). It may also write the long-vowel
/// mark ー as "^" ("pa^ka^"), which becomes a doubled vowel ("paakaa").
/// Also tidies the spacing.
fn spoken_particles(line: &str) -> String {
    let mut long = String::with_capacity(line.len());
    for c in line.chars() {
        match (c, long.chars().last()) {
            ('^', Some(v @ ('a' | 'e' | 'i' | 'o' | 'u'))) => long.push(v),
            ('^', _) => {}
            (c, _) => long.push(c),
        }
    }
    long.split_whitespace()
        .map(|w| match w {
            "ha" => "wa",
            "wo" => "o",
            w => w,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The Japanese-script lyrics for a track whose lyrics came back in romaji.
/// LRCLIB has far more Japanese uploads than romaji ones. Cached on disk.
pub async fn find_japanese(q: Query) -> Lyrics {
    let cache = cache_file(&q.path, "ja");
    if let Some(r) = std::fs::read_to_string(&cache).ok().and_then(|s| serde_json::from_str::<Record>(&s).ok()) {
        return r.into_lyrics();
    }
    match fetch_japanese(&q).await {
        Ok(Some(record)) => {
            store(&cache, &record);
            record.into_lyrics()
        }
        Ok(None) => Lyrics::NotFound,
        Err(e) => Lyrics::Error(e),
    }
}

async fn fetch_japanese(q: &Query) -> Result<Option<Record>, String> {
    let secs = q.duration.as_secs_f64();
    let (artist, title) = search_terms(&q.title, &q.artist);
    let query = format!("{artist} {title}");
    let Some(body) = lrclib_get("search", &[("q", query.trim())]).await? else {
        return Ok(None);
    };
    let candidates: Vec<Record> =
        serde_json::from_str(&body).map_err(|e| format!("unexpected LRCLIB response: {e}"))?;
    // Some uploads interleave a translation after every line; skip those when
    // a plain Japanese upload exists. English phrases in the song itself
    // still leave well over 60% of lines Japanese.
    let share = |r: &Record| japanese_line_share(r.synced_lyrics.as_deref().or(r.plain_lyrics.as_deref()).unwrap_or(""));
    let japanese: Vec<Record> = candidates.into_iter().filter(|r| share(r) > 0.0).collect();
    let pure: Vec<Record> = japanese.iter().filter(|r| share(r) >= 0.6).cloned().collect();
    if let Some(found) = pick(pure, secs, &title).or_else(|| pick(japanese, secs, &title)) {
        return Ok(Some(check_timing(found, secs)));
    }
    // Covers are often filed only under the original artist: the song's name
    // alone, keeping only uploads mostly in Japanese script.
    let Some(body) = lrclib_get("search", &[("track_name", title.as_str())]).await? else {
        return Ok(None);
    };
    let candidates: Vec<Record> =
        serde_json::from_str(&body).map_err(|e| format!("unexpected LRCLIB response: {e}"))?;
    let japanese = candidates.iter().filter(|r| share(r) >= 0.6).cloned().collect();
    if let Some(found) = pick_by_name(japanese, secs, &title, true) {
        return Ok(Some(check_timing(found, secs)));
    }

    // Still nothing: the original artist's name, taken from same-name,
    // same-length uploads in romaji (Ado's "Overdose" → "natori"), finds the
    // Japanese uploads filed under it.
    let want = normalize(&title);
    let mut artists: Vec<String> = Vec::new();
    for r in &candidates {
        let romaji = r.synced_lyrics.as_deref().and_then(parse_lrc).is_some_and(|l| lines_look_like_romaji(&l));
        let same_song = normalize(r.track_name.as_deref().unwrap_or("")) == want
            && (r.duration.unwrap_or(f64::MAX) - secs).abs() <= MAX_DURATION_DIFF;
        if let (true, true, Some(artist)) = (romaji, same_song, r.artist_name.as_ref()) {
            if !artists.contains(artist) {
                artists.push(artist.clone());
            }
        }
    }
    for artist in artists.iter().take(2) {
        let query = format!("{artist} {title}");
        let Some(body) = lrclib_get("search", &[("q", query.as_str())]).await? else { continue };
        let more: Vec<Record> = serde_json::from_str(&body).unwrap_or_default();
        let japanese = more.into_iter().filter(|r| share(r) >= 0.6).collect();
        if let Some(found) = pick_by_name(japanese, secs, &title, true) {
            return Ok(Some(check_timing(found, secs)));
        }
    }
    Ok(None)
}

/// Share of the non-empty lyric lines that contain Japanese script.
fn japanese_line_share(lyrics: &str) -> f32 {
    let lines: Vec<&str> = lyrics
        .lines()
        .map(|l| l.rsplit(']').next().unwrap_or(l).trim())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return 0.0;
    }
    let japanese = lines.iter().filter(|l| l.chars().any(is_japanese_char)).count();
    japanese as f32 / lines.len() as f32
}

// ── Cache ─────────────────────────────────────────────────────────────────────

fn cache_path(audio: &Path) -> PathBuf {
    cache_file(audio, "")
}

/// `<hash>.json`, or `<hash>.<kind>.json` for an alternative version.
fn cache_file(audio: &Path, kind: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let key = fnv1a(audio.as_os_str().as_encoded_bytes());
    let kind = if kind.is_empty() { String::new() } else { format!(".{kind}") };
    PathBuf::from(home).join(format!(".cache/lavanda/lyrics/{key:016x}{kind}.json"))
}

fn store_cache(audio: &Path, record: &Record) {
    store(&cache_path(audio), record);
}

fn store(path: &Path, record: &Record) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    if let Ok(json) = serde_json::to_string(record) {
        std::fs::write(path, json).ok();
    }
}

/// FNV-1a: a stable hash, so cache file names survive compiler upgrades
/// (std's `DefaultHasher` makes no such promise).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn parses_lrclib_word_synced_lines() {
        let lrc = "[00:18.89] <00:18.89>Paper <00:19.25>boats <00:19.53>drifting\n\
                   [00:22.82] <00:22.82>Down <00:23.23>river";
        let lines = parse_lrc(lrc).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], LyricLine { time: ms(18_890), text: "Paper boats drifting".into() });
        assert_eq!(lines[1].text, "Down river");
    }

    #[test]
    fn handles_metadata_offset_repeats_and_bracketed_text() {
        let lrc = "\u{feff}[ar:Someone]\n[offset:+500]\n\
                   [00:30.00]second\n[00:10.00][01:00.00]chorus\n[00:20.00][Bridge]\n[00:40.00]";
        let lines = parse_lrc(lrc).unwrap();
        let got: Vec<(Duration, &str)> = lines.iter().map(|l| (l.time, l.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (ms(9_500), "chorus"),
                (ms(19_500), "[Bridge]"),
                (ms(29_500), "second"),
                (ms(39_500), ""),
                (ms(59_500), "chorus"),
            ]
        );
    }

    #[test]
    fn timestamp_variants() {
        assert_eq!(parse_timestamp("01:02.5"), Some(ms(62_500)));
        assert_eq!(parse_timestamp("01:02.345"), Some(ms(62_345)));
        assert_eq!(parse_timestamp("01:02:50"), Some(ms(62_500)));
        assert_eq!(parse_timestamp("01:02"), Some(ms(62_000)));
        assert_eq!(parse_timestamp("ar:Ado"), None);
        assert_eq!(parse_timestamp("00:-1"), None);
    }

    #[test]
    fn plain_text_is_not_lrc() {
        let lyrics = from_text("Line one\n\n\n\nLine two\n\n").unwrap();
        let Lyrics::Plain(lines) = lyrics else { panic!("expected plain lyrics") };
        let got: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(got, ["Line one", "", "Line two"]);
        assert_eq!(from_text(" \n "), None);
    }

    #[test]
    fn search_terms_from_youtube_tags() {
        let terms = search_terms;
        let pair = |a: &str, s: &str| (a.to_string(), s.to_string());
        assert_eq!(terms("【Ado】患者A (Patient A)", "Ado"), pair("Ado", "患者A"));
        assert_eq!(terms("米津玄師 MV「Lemon」", "米津玄師"), pair("米津玄師", "Lemon"));
        assert_eq!(
            terms("Rick Astley - Never Gonna Give You Up (Official Video) [4K]", "RickAstleyVEVO"),
            pair("Rick Astley", "Never Gonna Give You Up")
        );
        assert_eq!(terms("Song Feat. Someone [MV]", "Band - Topic"), pair("Band", "Song"));
        assert_eq!(terms("Song - Live", "Band"), pair("Band", "Song - Live"));
        assert_eq!(terms("【MV】", "X"), pair("X", "【MV】"));
        // Cover labels (these three songs found no lyrics before).
        assert_eq!(terms("【Ado】Overdose 歌いました", "Ado"), pair("Ado", "Overdose"));
        assert_eq!(terms("【Ado】ラブカ？ 歌いました", "Ado"), pair("Ado", "ラブカ？"));
        assert_eq!(terms("【Ado】罪と罰 歌いました", "Ado"), pair("Ado", "罪と罰"));
        assert_eq!(terms("Some Song (Cover)", "X"), pair("X", "Some Song"));
        assert_eq!(terms("Some Song - Cover", "X"), pair("X", "Some Song"));
        assert_eq!(terms("Some Song covered by Someone", "X"), pair("X", "Some Song"));
        assert_eq!(terms("ある歌 歌ってみた", "X"), pair("X", "ある歌"));
    }

    #[test]
    fn detects_japanese_script() {
        assert!(has_japanese("夜空に星が光る ねえ"));
        assert!(has_japanese("【Ado】愛して愛して愛して"));
        assert!(has_japanese("Ah-ah 私のコートには")); // mixed, mostly Japanese
        assert!(!has_japanese("yozora ni hikaru hoshi o miageta"));
        assert!(!has_japanese("Never gonna give you up"));
        assert!(!has_japanese("♪ 123"));
    }

    #[test]
    fn matches_romaji_uploads_by_their_letters() {
        // kakasi's run-together romaji vs a person's spaced romaji, for the
        // same (made-up) lines.
        let ours = romaji_grams("yozora nihikaruhoshiwo miageta\nkimi nokoe gamada kikoeru");
        let theirs = romaji_grams("[00:01.00] yozora ni hikaru hoshi o miageta\n[00:04.00] kimi no koe ga mada kikoeru");
        let english = romaji_grams("[00:01.00] I looked up at the stars in the night sky\n[00:04.00] I can still hear your voice");
        let other_song = romaji_grams("[00:01.00] ame no naka de odoru kage\n[00:04.00] tooku made hashiru michi");
        assert!(shared_share(&ours, &theirs) >= 0.6);
        assert!(shared_share(&ours, &english) < 0.3);
        assert!(shared_share(&ours, &other_song) < 0.3);
    }

    #[test]
    fn spots_translated_uploads() {
        let pure = "[00:01.00]夜空に星が光る\n[00:05.00]Oh yeah\n[00:09.00]君の声がまだ聞こえる";
        let translated = "[00:01.00]夜空に星が光る\n[00:01.00]Stars shine in the night sky\n\
                          [00:05.00]君の声がまだ聞こえる\n[00:05.00]I can still hear your voice";
        assert!(japanese_line_share(pure) >= 0.6);
        assert!(japanese_line_share(translated) < 0.6);
        assert_eq!(japanese_line_share(""), 0.0);
    }

    #[test]
    fn lines_up_readings_with_kanji() {
        let r = |text: &str, reading: Option<&str>| Ruby { text: text.into(), reading: reading.map(Into::into) };
        assert_eq!(word_pieces("夜空[よぞら]"), vec![r("夜空", Some("よぞら"))]);
        assert_eq!(word_pieces("光る[ひかる]"), vec![r("光", Some("ひか")), r("る", None)]);
        assert_eq!(word_pieces("取り出す[とりだす]"), vec![r("取", Some("と")), r("り", None), r("出", Some("だ")), r("す", None)]);
        assert_eq!(word_pieces("手が[てが]"), vec![r("手", Some("て")), r("が", None)]);
        assert_eq!(word_pieces("君[くん]"), vec![r("君", Some("きみ"))]);
        // kakasi's partial reading can't be lined up: no furigana, text intact.
        assert_eq!(word_pieces("揃った[た]"), vec![r("揃った", None)]);
        assert_eq!(word_pieces("が"), vec![r("が", None)]);
        assert_eq!(merge_plain(vec![r("a", None), r("b", None), r("空", Some("そら")), r("c", None)]),
                   vec![r("ab", None), r("空", Some("そら")), r("c", None)]);
    }

    #[test]
    fn recognises_romaji_lyrics() {
        let lines = |text: &str| -> Vec<LyricLine> {
            text.lines().map(|t| LyricLine { time: Duration::ZERO, text: t.into() }).collect()
        };
        // Made-up lines in romaji (with a long vowel, ん, っ and "tch").
        let romaji = lines("yozora ni hikaru hoshi o miageta\nkimi no koe ga mada kikoeru\n\
                            tōi michi wo hashitte kita\nzutto matchi o sagashiteru\nshinpai shinaide ne");
        let english = lines("I looked up at the stars in the night sky\nI can still hear your voice tonight\n\
                             we walked along the river for hours\nnothing ever stays the same forever");
        assert!(lines_look_like_romaji(&romaji));
        assert!(!lines_look_like_romaji(&english));
        assert!(is_romaji_word("shinpai") && is_romaji_word("hashitte") && is_romaji_word("tōi"));
        assert!(!is_romaji_word("forever") && !is_romaji_word("strength"));
    }

    #[test]
    fn particles_are_spelled_as_sung() {
        assert_eq!(spoken_particles("watashi ha  anata wo  mitsumeru"), "watashi wa anata o mitsumeru");
        assert_eq!(spoken_particles("hana  wa saku"), "hana wa saku");
        assert_eq!(spoken_particles("watashi no pa^ka^ niha"), "watashi no paakaa niha");
    }

    #[test]
    fn switching_script_keeps_the_timing() {
        let at = |s: u64, t: &str| LyricLine { time: Duration::from_secs(s), text: t.into() };
        // The song's own (Japanese) lines, and kakasi's romaji of them.
        let primary = vec![at(10, "夜空"), at(14, "星"), at(18, "君の声"), at(30, "夜空"), at(34, "星")];
        let ours = vec![
            at(10, "yozora ni hikaru"), at(14, "hoshi wo miageta"), at(18, "kiminokoe ga kikoeru"),
            at(30, "yozora ni hikaru"), at(34, "hoshi wo miageta"),
        ];
        // An upload timed differently, with an extra line and one missing.
        let theirs = vec![
            at(12, "Yozora ni hikaru"), at(16, "Hoshi o miageta"), at(19, "(la la la)"),
            at(21, "Kimi no koe ga kikoeru"), at(33, "Yozora ni hikaru"),
        ];
        let romaji: Vec<String> = theirs.iter().map(|l| l.text.clone()).collect();
        let out = retime(&primary, &ours, &theirs, &romaji);
        let times: Vec<u64> = out.iter().map(|l| l.time.as_secs()).collect();
        assert_eq!(times, [10, 14, 18, 30, 34], "timing is the song's own");
        let words: Vec<&str> = out.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(words, [
            "Yozora ni hikaru", "Hoshi o miageta", "Kimi no koe ga kikoeru",
            "Yozora ni hikaru", "hoshi wo miageta", // no counterpart: kakasi's line
        ]);
    }

    #[test]
    fn name_only_search_rejects_other_songs_of_the_same_name() {
        let rec = |name: &str, artist: &str, duration: f64, synced: &str| Record {
            track_name: Some(name.into()),
            artist_name: Some(artist.into()),
            duration: Some(duration),
            synced_lyrics: Some(synced.into()),
            plain_lyrics: Some("words".into()),
            ..Record::default()
        };
        let candidates = vec![
            rec("Some Name", "English Band", 193.0, "[00:01.00]paper boats on the river"), // same length, wrong song
            rec("Other Name", "Someone", 194.0, "[00:01.00]夜空に星が光る"),              // wrong name
            rec("Some Name", "Original", 194.0, "[00:01.00]夜空に星が光る"),              // the one
            rec("Some Name", "Original", 230.0, "[00:01.00]夜空に星が光る"),              // other cut
        ];
        let picked = pick_by_name(candidates.clone(), 193.2, "Some Name", true).unwrap();
        assert_eq!(picked.duration, Some(194.0));
        assert!(picked.synced_lyrics.unwrap().contains('夜'));
        // Romaji counts as Japanese; English of the same length still doesn't.
        let romaji = rec("Some Name", "Original", 193.5,
            "[00:01.00]yozora ni hikaru hoshi o miageta\n[00:04.00]kimi no koe ga mada kikoeru\n\
             [00:07.00]tooi michi wo hashitte kita\n[00:10.00]shinpai shinaide matteru yo");
        let with_romaji = vec![candidates[0].clone(), romaji];
        assert_eq!(pick_by_name(with_romaji, 193.2, "Some Name", true).unwrap().duration, Some(193.5));
        // For a non-Japanese track the language isn't constrained.
        assert_eq!(pick_by_name(candidates.clone(), 193.2, "Some Name", false).unwrap().duration, Some(193.0));
        // Nothing within the length tolerance: nothing at all.
        assert!(pick_by_name(candidates, 150.0, "Some Name", true).is_none());
    }

    #[test]
    fn drops_synced_lines_timed_past_the_end() {
        let rec = || Record {
            synced_lyrics: Some("[00:10.00]a\n[03:58.59]b".into()),
            plain_lyrics: Some("a\nb".into()),
            ..Record::default()
        };
        // Lines run to 3:58 but the file is 200 s long: keep the words only.
        let r = check_timing(rec(), 200.0);
        assert!(r.synced_lyrics.is_none() && r.plain_lyrics.is_some());
        assert!(check_timing(rec(), 256.0).synced_lyrics.is_some());
        assert!(check_timing(rec(), 0.0).synced_lyrics.is_some());
    }

    fn rec(name: &str, duration: f64, synced: bool) -> Record {
        Record {
            track_name: Some(name.into()),
            artist_name: None,
            duration: Some(duration),
            instrumental: Some(false),
            plain_lyrics: Some("words".into()),
            synced_lyrics: synced.then(|| "[00:01.00]words".into()),
            ..Record::default()
        }
    }

    #[test]
    fn clean_uploads_win_over_ones_with_a_translation_woven_in() {
        // Every sung line followed by its translation at the same time.
        let woven = "[00:01.00]夜空に星\n[00:01.00]Stars in the night sky\n\
                     [00:05.00]君の声\n[00:05.00]Your voice\n[00:09.00]遠い道\n[00:09.00]A long road";
        let clean = "[00:01.20]夜空に星\n[00:05.10]君の声\n[00:09.30]遠い道";
        let with = |lrc: &str, duration: f64| Record { synced_lyrics: Some(lrc.into()), ..rec("Song", duration, true) };
        assert!(with(woven, 200.0).interleaved());
        assert!(!with(clean, 200.0).interleaved());
        // The woven one is closer in length, the clean one still wins.
        let got = pick(vec![with(woven, 200.0), with(clean, 201.5)], 200.0, "Song").unwrap();
        assert_eq!(got.duration, Some(201.5));
        // With nothing clean, the woven one is still better than nothing.
        assert!(pick(vec![with(woven, 200.0)], 200.0, "Song").is_some());
    }

    #[test]
    fn picks_synced_only_when_duration_matches() {
        // Closest synced cut within tolerance wins.
        let got = pick(vec![rec("Lemon", 274.0, true), rec("Lemon", 256.0, true), rec("Lemon", 255.0, true)], 255.4, "Lemon").unwrap();
        assert_eq!(got.duration, Some(255.0));
        assert!(got.synced_lyrics.is_some());

        // Only a different cut is synced: show its words unsynced.
        let got = pick(vec![rec("Lemon", 274.0, true)], 255.0, "Lemon").unwrap();
        assert!(got.synced_lyrics.is_none() && got.plain_lyrics.is_some());

        // Unrelated song with a matching name is not used for plain fallback.
        assert!(pick(vec![rec("Something Else", 300.0, false)], 255.0, "Lemon").is_none());
    }

    #[test]
    fn records_convert_and_round_trip_through_the_cache_format() {
        let r: Record = serde_json::from_str(
            r#"{"id":1,"trackName":"T","artistName":"A","duration":214.0,"instrumental":false,
                "plainLyrics":"a\nb","syncedLyrics":"[00:01.00]a\n[00:02.00]b"}"#,
        )
        .unwrap();
        let cached: Record = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert!(matches!(cached.into_lyrics(), Lyrics::Synced(l) if l.len() == 2));

        let instrumental: Record =
            serde_json::from_str(r#"{"instrumental":true,"plainLyrics":null,"syncedLyrics":null}"#).unwrap();
        assert_eq!(instrumental.into_lyrics(), Lyrics::Instrumental);
    }
}
