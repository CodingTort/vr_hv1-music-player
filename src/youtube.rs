// ── YouTube search & download via yt-dlp ─────────────────────────────────────
//
// lavanda itself has no networking code, so this module shells out to yt-dlp,
// which is kept separate from the rest of the app and is entirely optional:
// if yt-dlp is not installed every entry point returns a clear error instead
// of failing silently.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;

use crate::config::Config;

/// How long to wait for a search before giving up.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for a download before giving up.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// A single result from a YouTube search.
#[derive(Debug, Clone)]
pub struct YtResult {
    /// Kept for deduplication and future use; not currently rendered.
    #[allow(dead_code)]
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration: String,
    pub url: String,
}

/// Raw shape of the entry objects inside yt-dlp's search JSON.
///
/// Every field is optional: YouTube results vary a lot (live streams have no
/// duration, some uploads have no uploader, etc.) and a missing field should
/// degrade to a blank cell rather than fail the whole search.
#[derive(Deserialize)]
struct RawEntry {
    id: Option<String>,
    title: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    duration_string: Option<String>,
    duration: Option<f64>,
    webpage_url: Option<String>,
    /// Entries of a playlist read with --flat-playlist carry their link here.
    url: Option<String>,
}

/// `ytsearchN:` returns a playlist wrapper whose results live under `entries`.
/// Plain (non-search) extractions come back as a bare entry, so accept both.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawPayload {
    Playlist {
        entries: Vec<RawEntry>,
        #[serde(default)]
        title: Option<String>,
    },
    Single(Box<RawEntry>),
}

/// Set once yt-dlp has been found working, so later checks are free.
static YT_DLP_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Returns an error string if yt-dlp is unusable, otherwise `None`. Runs
/// `yt-dlp --version` (a fraction of a second) only until it has worked once.
pub fn preflight(cfg: &Config) -> Option<String> {
    use std::sync::atomic::Ordering;
    if YT_DLP_OK.load(Ordering::Relaxed) {
        return None;
    }
    let out = std::process::Command::new(&cfg.yt_dlp_path)
        .arg("--version")
        .stdin(Stdio::null())
        .output();

    match out {
        Ok(o) if o.status.success() => {
            YT_DLP_OK.store(true, Ordering::Relaxed);
            None
        }
        Ok(_) => Some(format!(
            "yt-dlp returned an error (is '{}' working?)",
            cfg.yt_dlp_path
        )),
        Err(_e) => Some(format!(
            "yt-dlp not found at '{}' — install it with: sudo pacman -S yt-dlp",
            cfg.yt_dlp_path
        )),
    }
}

/// Formats a second count as m:ss (or h:mm:ss).
fn fmt_duration(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Condenses yt-dlp's multi-line stderr into something readable in a small
/// dialog. Bot checks and age-gates bury the useful line in the noise, so keep
/// the first few meaningful lines rather than the very last one.
fn clean_error(stderr: &str, stdout: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for src in [stderr, stdout] {
        for line in src.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('[') || l.starts_with('{') {
                continue;
            }
            lines.push(l);
            if lines.len() == 4 {
                break;
            }
        }
        if !lines.is_empty() {
            break;
        }
    }
    if lines.is_empty() {
        "yt-dlp failed with no output".to_string()
    } else {
        lines.join(" · ")
    }
}

/// Searches YouTube and returns up to `limit` results.
///
/// Note: this queries YouTube proper. YouTube Music has no public search API,
/// so results come from the regular YouTube catalogue.
pub async fn search(query: &str, limit: usize) -> Result<Vec<YtResult>, String> {
    let results = search_once(query, limit).await?;
    // YouTube sometimes serves a search-page layout from which the quick
    // (--flat-playlist) results carry no channel names. Asking again costs
    // about a second and usually gets them back.
    if !results.is_empty() && results.iter().all(|r| r.uploader.is_empty()) {
        if let Ok(again) = search_once(query, limit).await {
            if again.iter().any(|r| !r.uploader.is_empty()) {
                return Ok(again);
            }
        }
    }
    Ok(results)
}

async fn search_once(query: &str, limit: usize) -> Result<Vec<YtResult>, String> {
    let cfg = crate::config::get();
    if let Some(err) = preflight(cfg) {
        return Err(err);
    }

    let target = format!("ytsearch{limit}:{query}");

    let out = tokio::time::timeout(
        SEARCH_TIMEOUT,
        Command::new(&cfg.yt_dlp_path)
            // --flat-playlist reads the results off the search page instead of
            // loading every video: ~1 s instead of ~30–60 s for 15 results.
            .args(["--no-warnings", "--ignore-config", "--flat-playlist", "-J"])
            .arg(&target)
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| "search timed out after 60s".to_string())?
    .map_err(|e| format!("could not run yt-dlp: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    if !out.status.success() {
        return Err(clean_error(&stderr, &stdout));
    }

    let payload: RawPayload = serde_json::from_str(&stdout)
        .map_err(|e| format!("could not parse yt-dlp response: {e}"))?;

    let entries = match payload {
        RawPayload::Playlist { entries, .. } => entries,
        RawPayload::Single(e) => vec![*e],
    };
    Ok(to_results(entries))
}

fn to_results(entries: Vec<RawEntry>) -> Vec<YtResult> {
    entries
        .into_iter()
        .filter_map(|e| {
            let id = e.id.clone()?;
            let url = e
                .webpage_url
                .clone()
                .or(e.url.clone().filter(|u| u.starts_with("http")))
                .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));
            Some(YtResult {
                id,
                title: e.title.unwrap_or_else(|| "unknown".into()),
                uploader: e.uploader.or(e.channel).unwrap_or_default(),
                duration: e
                    .duration_string
                    .or_else(|| e.duration.map(fmt_duration))
                    .unwrap_or_default(),
                url,
            })
        })
        .collect()
}

/// Whether the search box holds a link rather than words to search for.
pub fn looks_like_url(text: &str) -> bool {
    let t = text.trim();
    !t.contains(char::is_whitespace)
        && (t.starts_with("http://")
            || t.starts_with("https://")
            || t.starts_with("youtu.be/")
            || t.starts_with("www.youtube.com/")
            || t.starts_with("youtube.com/")
            || t.starts_with("music.youtube.com/")
            || t.starts_with("m.youtube.com/"))
}

/// What a pasted link points at: one song, or a playlist / album with its
/// title. Read with --flat-playlist, so even long playlists load quickly.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The playlist's or album's name; `None` for a single song.
    pub list_title: Option<String>,
    pub items: Vec<YtResult>,
}

pub async fn resolve(link: &str) -> Result<Resolved, String> {
    let cfg = crate::config::get();
    if let Some(err) = preflight(cfg) {
        return Err(err);
    }
    let link = link.trim();
    let link = if link.starts_with("http") { link.to_string() } else { format!("https://{link}") };
    let out = tokio::time::timeout(
        SEARCH_TIMEOUT,
        Command::new(&cfg.yt_dlp_path)
            .args(["--no-warnings", "--ignore-config", "--flat-playlist", "-J"])
            .arg(&link)
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| "reading the link timed out after 60s".to_string())?
    .map_err(|e| format!("could not run yt-dlp: {e}"))?;
    if !out.status.success() {
        return Err(clean_error(&String::from_utf8_lossy(&out.stderr), &String::from_utf8_lossy(&out.stdout)));
    }
    let payload: RawPayload = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("could not read yt-dlp's answer: {e}"))?;
    Ok(match payload {
        RawPayload::Playlist { entries, title } => Resolved {
            list_title: Some(title.map(|t| t.trim_start_matches("Album - ").to_string()).unwrap_or_else(|| "Imported".into())),
            items: to_results(entries),
        },
        RawPayload::Single(e) => Resolved { list_title: None, items: to_results(vec![*e]) },
    })
}

/// A large thumbnail for a preview: full size (1280×720) when the video has
/// one, else the 320×180 one.
pub async fn thumbnail_large(id: String) -> Option<Vec<u8>> {
    let valid = id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return None;
    }
    for name in ["maxresdefault", "mqdefault"] {
        let out = tokio::time::timeout(
            Duration::from_secs(15),
            Command::new("curl")
                .args(["--silent", "--fail", "--max-time", "10"])
                .arg(format!("https://i.ytimg.com/vi/{id}/{name}.jpg"))
                .stdin(Stdio::null())
                .output(),
        )
        .await
        .ok()?
        .ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            return Some(out.stdout);
        }
    }
    None
}

/// A search result's thumbnail (a 320×180 JPEG) from YouTube's image server.
pub async fn thumbnail(id: String) -> Option<Vec<u8>> {
    let valid = id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return None;
    }
    let out = tokio::time::timeout(
        Duration::from_secs(15),
        Command::new("curl")
            .args(["--silent", "--fail", "--max-time", "10"])
            .arg(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"))
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
}

/// Downloads the audio of `url` into the library directory.
///
/// On success returns the final path when yt-dlp reported one. The caller
/// triggers a library rescan regardless — lavanda reads the tags, not the
/// filename.
pub async fn download(url: &str) -> Result<Option<PathBuf>, String> {
    let cfg = crate::config::get();
    if let Some(err) = preflight(cfg) {
        return Err(err);
    }

    let dest = cfg.music_path();
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("could not create {}: {e}", dest.display()))?;

    // Partial files and the thumbnail are staged here and only moved into the
    // library once the download succeeds, so a failure leaves no stray
    // `.webp` next to the music.
    let staging = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
        .join(".cache/lavanda/downloading");
    let home = format!("home:{}", dest.display());
    let temp = format!("temp:{}", staging.display());

    // YouTube now and then refuses a stream with HTTP 403. A fresh attempt
    // fetches new stream URLs and almost always works, so retry a few times.
    const ATTEMPTS: u32 = 3;
    let mut attempt = 1;
    let out = loop {
        // `--print after_move:filepath` reports the post-conversion path, but
        // it lands on stderr alongside the logs, so both streams are captured
        // and the path is picked out opportunistically. Losing it is not fatal.
        let out = tokio::time::timeout(
            DOWNLOAD_TIMEOUT,
            Command::new(&cfg.yt_dlp_path)
                .args([
                    "-x",
                    // Plain stereo audio: everything ends up as stereo
                    // `yt_dlp_format` anyway, and YouTube's surround streams
                    // are 3× larger.
                    "-f",
                    "bestaudio[audio_channels<=2]/bestaudio",
                    "--audio-format",
                    &cfg.yt_dlp_format,
                    "--audio-quality",
                    &cfg.yt_dlp_quality,
                    "--embed-thumbnail",
                    "--add-metadata",
                    "--no-playlist",
                    "--no-warnings",
                    "--ignore-config",
                    "--no-progress",
                    "--print",
                    "after_move:%(filepath)s",
                    "-P",
                    &home,
                    "-P",
                    &temp,
                    "-o",
                    &cfg.yt_dlp_output,
                ])
                .arg(url)
                .stdin(Stdio::null())
                .output(),
        )
        .await
        .map_err(|_| "download timed out after 15 minutes".to_string())?
        .map_err(|e| format!("could not run yt-dlp: {e}"))?;

        let refused = String::from_utf8_lossy(&out.stderr).contains("HTTP Error 403");
        if out.status.success() || !refused || attempt == ATTEMPTS {
            break out;
        }
        attempt += 1;
    };

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    if !out.status.success() {
        return Err(clean_error(&stderr, &stdout));
    }

    // Recover the reported filepath: last line that actually exists on disk.
    let reported = format!("{stderr}\n{stdout}")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('[') && !l.starts_with('{'))
        .rev()
        .find_map(|l| {
            let p = PathBuf::from(l);
            p.is_file().then_some(p)
        });

    Ok(reported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_told_apart_from_searches() {
        for link in [
            "https://www.youtube.com/watch?v=abcdefghijk",
            "https://youtu.be/abcdefghijk",
            "youtu.be/abcdefghijk",
            "https://music.youtube.com/playlist?list=OLAK5uy_example",
            "music.youtube.com/watch?v=abcdefghijk",
            "  https://soundcloud.com/someone/a-song  ",
        ] {
            assert!(looks_like_url(link), "{link}");
        }
        for words in ["lofi hip hop", "youtube.com is great", "Ado うっせぇわ", "https://a b"] {
            assert!(!looks_like_url(words), "{words}");
        }
    }

    #[test]
    fn playlist_and_single_answers_parse() {
        // Shapes of yt-dlp's -J --flat-playlist output (trimmed).
        let list = r#"{"_type":"playlist","title":"Album - Night Songs","entries":[
            {"id":"abcdefghijk","title":"One","url":"https://www.youtube.com/watch?v=abcdefghijk","duration":201},
            {"id":"bcdefghijkl","title":"Two","channel":"Band"}]}"#;
        let RawPayload::Playlist { entries, title } = serde_json::from_str(list).unwrap() else { panic!() };
        assert_eq!(title.as_deref(), Some("Album - Night Songs"));
        let items = to_results(entries);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].url, "https://www.youtube.com/watch?v=abcdefghijk");
        assert_eq!(items[0].duration, "3:21");
        assert_eq!(items[1].uploader, "Band");
        assert_eq!(items[1].url, "https://www.youtube.com/watch?v=bcdefghijkl");
        let single = r#"{"id":"abcdefghijk","title":"One","webpage_url":"https://www.youtube.com/watch?v=abcdefghijk","duration_string":"3:21"}"#;
        let RawPayload::Single(e) = serde_json::from_str(single).unwrap() else { panic!() };
        assert_eq!(to_results(vec![*e])[0].title, "One");
    }
}
