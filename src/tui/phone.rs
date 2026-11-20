//! Phone output: a small web player the terminal client controls.
//!
//! The laptop serves a page (`phone.html`) and the songs; the phone plays
//! them in its browser. The page learns what to play over server-sent events
//! and reports its position back. The server only listens on this computer
//! (reach it through an SSH tunnel) and on its Tailscale address, and every
//! request must carry the secret key from `~/.config/lavanda/phone-key`.

use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};

use crate::library::models::Track;
use crate::lyrics::LyricLine;

const PAGE: &str = include_str!("phone.html");
const ICON: &[u8] = include_bytes!("../../assets/tux.png");
const PAIR_PAGE: &str = include_str!("pair.html");

/// How long a pairing code works, and how many wrong tries it allows.
const CODE_LIFETIME: Duration = Duration::from_secs(10 * 60);
const CODE_TRIES: u32 = 5;

/// A short code to type on the phone instead of copying the long link.
struct Pairing {
    code: String,
    expires: std::time::Instant,
    tries: u32,
}

/// What the terminal client tells the page to do.
pub enum Cmd {
    /// Load the current song at `at` seconds, playing or paused.
    Load { at: f64, play: bool },
    Play,
    Pause,
    Seek(f64),
    /// Playback moved back to the laptop.
    Release,
}

/// What the page tells the terminal client.
#[derive(Debug)]
pub enum Event {
    Report { id: i64, at: f64, playing: bool },
    /// The page finished `id` and started `next` by itself.
    Ended { id: i64, next: Option<i64> },
    Toggle,
    Play,
    Pause,
    Next,
    Previous,
    /// The listener moved the position on the page.
    Seek(f64),
}

struct Shared {
    paths: HashMap<i64, PathBuf>,
    state: serde_json::Value,
    lyrics: serde_json::Value,
    cmd_n: u64,
    cmd: serde_json::Value,
    /// Where the song is, for a page that has just opened.
    position: f64,
    version: u64,
}

struct Ctx {
    token: String,
    shared: Mutex<Shared>,
    changed: watch::Sender<u64>,
    events: mpsc::UnboundedSender<Event>,
    /// One transcode at a time.
    transcoding: tokio::sync::Mutex<()>,
    pairing: Mutex<Option<Pairing>>,
}

pub struct Server {
    pub port: u16,
    ctx: Arc<Ctx>,
    events: mpsc::UnboundedReceiver<Event>,
}

impl Server {
    pub fn start(rt: &Handle, port: u16, tracks: &[Track]) -> Result<Server, String> {
        let token = load_or_create_token().map_err(|e| format!("Phone player: couldn't create its key: {e}"))?;
        let (changed, _) = watch::channel(0);
        let (events_tx, events) = mpsc::unbounded_channel();
        let ctx = Arc::new(Ctx {
            token: token.clone(),
            shared: Mutex::new(Shared {
                paths: HashMap::new(),
                state: json!({ "active": false, "track": null, "next": null, "playing": false }),
                lyrics: json!({ "id": null, "synced": false, "lines": [] }),
                cmd_n: 0,
                cmd: json!(null),
                position: 0.0,
                version: 0,
            }),
            changed,
            events: events_tx,
            transcoding: tokio::sync::Mutex::new(()),
            pairing: Mutex::new(None),
        });
        let server = Server { port, ctx, events };
        server.set_library(tracks);

        // This computer (for SSH tunnels), and the Tailscale address if any.
        let local = rt
            .block_on(TcpListener::bind(("127.0.0.1", port)))
            .map_err(|e| format!("Phone player: port {port} is unavailable ({e}); set phone_port in config.toml"))?;
        let mut listeners = vec![local];
        if let Some(ip) = rt.block_on(tailscale_ip()) {
            if let Ok(l) = rt.block_on(TcpListener::bind((ip.as_str(), port))) {
                listeners.push(l);
            }
        }
        for listener in listeners {
            let ctx = server.ctx.clone();
            rt.spawn(async move {
                loop {
                    let Ok((sock, _)) = listener.accept().await else { continue };
                    let ctx = ctx.clone();
                    tokio::spawn(async move {
                        let _ = handle(sock, ctx).await;
                    });
                }
            });
        }
        Ok(server)
    }

    /// A new 6-digit pairing code (the old one stops working).
    pub fn new_pairing_code(&self) -> String {
        use rand::Rng;
        let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
        *self.ctx.pairing.lock().unwrap() =
            Some(Pairing { code: code.clone(), expires: std::time::Instant::now() + CODE_LIFETIME, tries: 0 });
        code
    }

    pub fn try_event(&mut self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    fn update(&self, f: impl FnOnce(&mut Shared)) {
        let version = {
            let mut s = self.ctx.shared.lock().unwrap();
            f(&mut s);
            s.version += 1;
            s.version
        };
        self.ctx.changed.send_replace(version);
    }

    pub fn set_library(&self, tracks: &[Track]) {
        let paths = tracks.iter().map(|t| (t.id, t.path.clone())).collect();
        self.update(|s| s.paths = paths);
    }

    pub fn command(&self, cmd: Cmd) {
        let cmd = match cmd {
            Cmd::Load { at, play } => json!({ "kind": "load", "at": at, "play": play }),
            Cmd::Play => json!({ "kind": "play" }),
            Cmd::Pause => json!({ "kind": "pause" }),
            Cmd::Seek(at) => json!({ "kind": "seek", "at": at }),
            Cmd::Release => json!({ "kind": "release" }),
        };
        self.update(|s| {
            s.cmd_n += 1;
            s.cmd = cmd;
        });
    }

    /// Kept up to date without waking the event streams.
    pub fn set_position(&self, at: f64) {
        self.ctx.shared.lock().unwrap().position = at;
    }

    /// `next` follows the song when it ends, `skip` is where Next goes
    /// (they differ when repeat is on).
    pub fn set_now(&self, track: Option<&Track>, next: Option<i64>, skip: Option<i64>, playing: bool, active: bool) {
        let track = track.map(|t| {
            json!({
                "id": t.id,
                "title": t.title,
                "artist": crate::app::display_artist(t),
                "album": t.album,
                "duration": t.duration.as_secs_f64(),
            })
        });
        self.update(|s| s.state = json!({ "active": active, "track": track, "next": next, "skip": skip, "playing": playing }));
    }

    /// Lyric times are shifted by `offset` here, so the page can match them
    /// against its own clock directly.
    pub fn set_lyrics(&self, id: Option<i64>, lines: Option<(Vec<LyricLine>, bool)>, offset: f64) {
        let (lines, synced) = lines.unwrap_or_default();
        let lines: Vec<_> = lines.iter().map(|l| json!([l.time.as_secs_f64() + offset, l.text])).collect();
        self.update(|s| s.lyrics = json!({ "id": id, "synced": synced, "lines": lines }));
    }
}

/// Addresses to open on the phone, best first, without the key: the
/// computer's short Tailscale name, its Tailscale address, and this
/// computer itself (for an SSH tunnel).
pub async fn addresses(port: u16) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(name) = tailscale_name().await {
        // The first label works on every device in the tailnet (MagicDNS).
        let short = name.split('.').next().unwrap_or(&name).to_string();
        out.push(format!("http://{short}:{port}"));
    }
    if let Some(ip) = tailscale_ip().await {
        out.push(format!("http://{ip}:{port}"));
    }
    out.push(format!("http://localhost:{port}"));
    out
}

async fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = tokio::time::timeout(Duration::from_secs(3), tokio::process::Command::new(cmd).args(args).output()).await.ok()?.ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

async fn tailscale_ip() -> Option<String> {
    let out = run("tailscale", &["ip", "-4"]).await?;
    out.lines().next().map(|l| l.trim().to_string()).filter(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
}

async fn tailscale_name() -> Option<String> {
    let out = run("tailscale", &["status", "--json"]).await?;
    let v: serde_json::Value = serde_json::from_str(&out).ok()?;
    let name = v.get("Self")?.get("DNSName")?.as_str()?.trim_end_matches('.').to_string();
    (!name.is_empty()).then_some(name)
}

/// A random key, made once and kept so a bookmark on the phone keeps working.
fn load_or_create_token() -> std::io::Result<String> {
    let path = crate::config::config_path().with_file_name("phone-key");
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim().to_string();
        if t.len() >= 24 && t.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Ok(t);
        }
    }
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let token: String = (0..32).map(|_| char::from_digit(rng.gen_range(0..36), 36).unwrap_or('0')).collect();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(token)
}

// ── HTTP ─────────────────────────────────────────────────────────────────────

struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    range: Option<String>,
}

async fn read_request(sock: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    loop {
        let n = tokio::time::timeout(Duration::from_secs(10), sock.read(&mut chunk)).await.ok()?.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(&buf) {
            Ok(httparse::Status::Complete(_)) => {
                let target = req.path?.to_string();
                let (path, query) = target.split_once('?').unwrap_or((&target, ""));
                let range = req
                    .headers
                    .iter()
                    .find(|h| h.name.eq_ignore_ascii_case("range"))
                    .and_then(|h| std::str::from_utf8(h.value).ok())
                    .map(str::to_string);
                return Some(Request {
                    method: req.method?.to_string(),
                    path: percent_decode(path),
                    query: parse_query(query),
                    range,
                });
            }
            Ok(httparse::Status::Partial) if buf.len() < 16 * 1024 => continue,
            _ => return None,
        }
    }
}

fn parse_query(q: &str) -> HashMap<String, String> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

enum CodeCheck {
    Right,
    Wrong,
    /// None shown, expired, or used up by wrong tries.
    NoCode,
}

fn check_code(ctx: &Ctx, typed: &str) -> CodeCheck {
    let mut pairing = ctx.pairing.lock().unwrap();
    let Some(p) = pairing.as_mut() else { return CodeCheck::NoCode };
    if std::time::Instant::now() >= p.expires || p.tries >= CODE_TRIES {
        *pairing = None;
        return CodeCheck::NoCode;
    }
    if same_key(typed, &p.code) {
        return CodeCheck::Right;
    }
    p.tries += 1;
    if p.tries >= CODE_TRIES {
        *pairing = None;
        return CodeCheck::NoCode;
    }
    CodeCheck::Wrong
}

/// Compares in time independent of where the strings differ.
fn same_key(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn respond(sock: &mut TcpStream, status: &str, headers: &[(&str, String)], body: &[u8]) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    sock.write_all(head.as_bytes()).await?;
    sock.write_all(body).await?;
    sock.flush().await
}

async fn handle(mut sock: TcpStream, ctx: Arc<Ctx>) -> std::io::Result<()> {
    let Some(req) = read_request(&mut sock).await else { return Ok(()) };

    // The home-screen icon is the only thing served without the key.
    if req.path == "/icon.png" {
        return respond(&mut sock, "200 OK", &[("Content-Type", "image/png".into()), ("Cache-Control", "max-age=86400".into())], ICON).await;
    }
    let authorised = req.query.get("k").is_some_and(|k| same_key(k, &ctx.token));
    if !authorised {
        return match (req.method.as_str(), req.path.as_str()) {
            // Without the key: the page asking for the pairing code.
            ("GET", "/") => {
                let headers = [("Content-Type", "text/html; charset=utf-8".into()), ("Cache-Control", "no-store".into())];
                respond(&mut sock, "200 OK", &headers, PAIR_PAGE.as_bytes()).await
            }
            ("GET", "/pair") => {
                let typed: String = req.query.get("code").map(|c| c.chars().filter(char::is_ascii_digit).collect()).unwrap_or_default();
                let outcome = check_code(&ctx, &typed);
                let to = match outcome {
                    CodeCheck::Right => format!("/?k={}", ctx.token),
                    CodeCheck::Wrong => "/?wrong=1".into(),
                    CodeCheck::NoCode => "/?expired=1".into(),
                };
                respond(&mut sock, "303 See Other", &[("Location", to), ("Cache-Control", "no-store".into())], b"").await
            }
            _ => respond(&mut sock, "403 Forbidden", &[("Content-Type", "text/plain".into())], b"This needs the player's key: open the address shown by lavanda-tui.").await,
        };
    }

    let segments: Vec<&str> = req.path.trim_start_matches('/').split('/').collect();
    match (req.method.as_str(), segments.as_slice()) {
        ("GET", [""]) => {
            let headers = [("Content-Type", "text/html; charset=utf-8".into()), ("Cache-Control", "no-store".into()), ("Referrer-Policy", "no-referrer".into())];
            respond(&mut sock, "200 OK", &headers, PAGE.as_bytes()).await
        }
        ("GET", ["events"]) => events(sock, ctx).await,
        ("GET", ["audio", id]) => {
            let path = id.parse::<i64>().ok().and_then(|id| ctx.shared.lock().unwrap().paths.get(&id).cloned());
            match path {
                Some(path) => audio(sock, &ctx, &path, req.range.as_deref()).await,
                None => respond(&mut sock, "404 Not Found", &[], b"").await,
            }
        }
        ("GET", ["cover", id]) => {
            let path = id.parse::<i64>().ok().and_then(|id| ctx.shared.lock().unwrap().paths.get(&id).cloned());
            let bytes = match path {
                Some(p) => tokio::task::spawn_blocking(move || crate::library::scanner::load_cover(&p)).await.ok().flatten(),
                None => None,
            };
            match bytes {
                Some(b) => {
                    let headers = [("Content-Type", image_type(&b).into()), ("Cache-Control", "max-age=3600".into())];
                    respond(&mut sock, "200 OK", &headers, &b).await
                }
                None => respond(&mut sock, "404 Not Found", &[], b"").await,
            }
        }
        ("POST", ["report"]) => {
            let q = &req.query;
            if let (Some(id), Some(at)) = (q.get("id").and_then(|v| v.parse().ok()), q.get("t").and_then(|v| v.parse::<f64>().ok())) {
                let playing = q.get("p").is_some_and(|p| p == "1");
                let _ = ctx.events.send(Event::Report { id, at, playing });
            }
            respond(&mut sock, "204 No Content", &[], b"").await
        }
        ("POST", ["ended"]) => {
            if let Some(id) = req.query.get("id").and_then(|v| v.parse().ok()) {
                let next = req.query.get("next").and_then(|v| v.parse().ok());
                let _ = ctx.events.send(Event::Ended { id, next });
            }
            respond(&mut sock, "204 No Content", &[], b"").await
        }
        ("POST", ["cmd"]) => {
            let ev = match req.query.get("c").map(String::as_str) {
                Some("toggle") => Some(Event::Toggle),
                Some("play") => Some(Event::Play),
                Some("pause") => Some(Event::Pause),
                Some("next") => Some(Event::Next),
                Some("prev") => Some(Event::Previous),
                Some("seek") => req.query.get("t").and_then(|t| t.parse().ok()).map(Event::Seek),
                _ => None,
            };
            if let Some(ev) = ev {
                let _ = ctx.events.send(ev);
            }
            respond(&mut sock, "204 No Content", &[], b"").await
        }
        _ => respond(&mut sock, "404 Not Found", &[], b"").await,
    }
}

/// Server-sent events: the playback state, the latest command and the
/// lyrics, sent again whenever they change.
async fn events(mut sock: TcpStream, ctx: Arc<Ctx>) -> std::io::Result<()> {
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    sock.write_all(head.as_bytes()).await?;
    let mut changed = ctx.changed.subscribe();
    let mut sent_lyrics = String::new();
    let mut sent_state = String::new();
    loop {
        let (state, state_key, lyrics) = {
            let s = ctx.shared.lock().unwrap();
            let mut state = s.state.clone();
            state["cmd"] = json!({ "n": s.cmd_n, "do": s.cmd });
            let without_position = state.to_string();
            state["at"] = json!(s.position);
            (state.to_string(), without_position, s.lyrics.to_string())
        };
        if lyrics != sent_lyrics {
            sock.write_all(format!("event: lyrics\ndata: {lyrics}\n\n").as_bytes()).await?;
            sent_lyrics = lyrics;
        }
        // The position alone changing isn't news; it rides along otherwise.
        if state_key != sent_state {
            sock.write_all(format!("event: state\ndata: {state}\n\n").as_bytes()).await?;
            sent_state = state_key;
        }
        sock.flush().await?;
        match tokio::time::timeout(Duration::from_secs(15), changed.changed()).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Ok(()),
            // Keeps proxies and the phone from closing an idle connection.
            Err(_) => sock.write_all(b": ping\n\n").await?,
        }
    }
}

fn image_type(b: &[u8]) -> &'static str {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// Formats Safari plays as they are; anything else is converted to MP3 once.
fn audio_type(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" | "aac" | "alac" => "audio/mp4",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "aif" | "aiff" => "audio/aiff",
        _ => return None,
    })
}

async fn audio(mut sock: TcpStream, ctx: &Ctx, path: &Path, range: Option<&str>) -> std::io::Result<()> {
    let (file_path, mime) = match audio_type(path) {
        Some(mime) => (path.to_path_buf(), mime),
        None => match transcoded(ctx, path).await {
            Some(p) => (p, "audio/mpeg"),
            None => {
                return respond(&mut sock, "415 Unsupported Media Type", &[("Content-Type", "text/plain".into())], b"This format needs ffmpeg to play on a phone.").await;
            }
        },
    };
    let mut file = tokio::fs::File::open(&file_path).await?;
    let size = file.metadata().await?.len();
    let (start, end) = match range.and_then(|r| parse_range(r, size)) {
        Some(r) => r,
        None if range.is_some() => {
            return respond(&mut sock, "416 Range Not Satisfiable", &[("Content-Range", format!("bytes */{size}"))], b"").await;
        }
        None => (0, size.saturating_sub(1)),
    };
    let len = if size == 0 { 0 } else { end - start + 1 };
    let status = if range.is_some() { "206 Partial Content" } else { "200 OK" };
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n"
    );
    if range.is_some() {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{size}\r\n"));
    }
    head.push_str("\r\n");
    sock.write_all(head.as_bytes()).await?;
    file.seek(SeekFrom::Start(start)).await?;
    let mut limited = file.take(len);
    tokio::io::copy(&mut limited, &mut sock).await?;
    sock.flush().await
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n`, as an inclusive range.
fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if size == 0 {
        return None;
    }
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (size.saturating_sub(n), size - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() { size - 1 } else { b.parse::<u64>().ok()?.min(size - 1) };
        (start, end)
    };
    (start <= end && start < size).then_some((start, end))
}

/// An MP3 copy of `path` in `~/.cache/lavanda/phone/`, made with ffmpeg.
async fn transcoded(ctx: &Ctx, path: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&path, &mut h);
    let key = std::hash::Hasher::finish(&h);
    let dir = PathBuf::from(std::env::var("HOME").ok()?).join(".cache/lavanda/phone");
    let out = dir.join(format!("{key:016x}-{mtime}.mp3"));
    let _guard = ctx.transcoding.lock().await;
    if out.exists() {
        return Some(out);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let tmp = out.with_extension("part.mp3");
    let ok = tokio::process::Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
        .arg(path)
        .args(["-vn", "-c:a", "libmp3lame", "-q:a", "2"])
        .arg(&tmp)
        .status()
        .await
        .ok()?
        .success();
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    std::fs::rename(&tmp, &out).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_codes_expire_and_lock() {
        let (changed, _) = watch::channel(0);
        let (events, _rx) = mpsc::unbounded_channel();
        let ctx = Ctx {
            token: "k".repeat(32),
            shared: Mutex::new(Shared {
                paths: HashMap::new(),
                state: json!(null),
                lyrics: json!(null),
                cmd_n: 0,
                cmd: json!(null),
                position: 0.0,
                version: 0,
            }),
            changed,
            events,
            transcoding: tokio::sync::Mutex::new(()),
            pairing: Mutex::new(None),
        };
        let set = |code: &str, expires_in: Duration| {
            *ctx.pairing.lock().unwrap() = Some(Pairing { code: code.into(), expires: std::time::Instant::now() + expires_in, tries: 0 });
        };
        assert!(matches!(check_code(&ctx, "123456"), CodeCheck::NoCode), "no code shown yet");
        set("123456", CODE_LIFETIME);
        assert!(matches!(check_code(&ctx, "654321"), CodeCheck::Wrong));
        assert!(matches!(check_code(&ctx, "123456"), CodeCheck::Right));
        assert!(matches!(check_code(&ctx, "123456"), CodeCheck::Right), "works for a second device too");
        // Five wrong tries use the code up, even the right one afterwards.
        set("123456", CODE_LIFETIME);
        for _ in 0..4 {
            assert!(matches!(check_code(&ctx, "000000"), CodeCheck::Wrong));
        }
        assert!(matches!(check_code(&ctx, "000000"), CodeCheck::NoCode));
        assert!(matches!(check_code(&ctx, "123456"), CodeCheck::NoCode));
        // Expired.
        set("123456", Duration::ZERO);
        assert!(matches!(check_code(&ctx, "123456"), CodeCheck::NoCode));
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-1", 100), Some((0, 1)));
        assert_eq!(parse_range("bytes=10-", 100), Some((10, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=50-500", 100), Some((50, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=5-2", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
    }

    #[test]
    fn query_and_keys() {
        let q = parse_query("k=abc&t=12.5&name=a%20b+c&bad=%zz");
        assert_eq!(q["k"], "abc");
        assert_eq!(q["t"], "12.5");
        assert_eq!(q["name"], "a b c");
        assert_eq!(q["bad"], "%zz");
        assert!(same_key("abc", "abc"));
        assert!(!same_key("abc", "abd"));
        assert!(!same_key("abc", "abcd"));
        assert_eq!(percent_decode("%E5%A4%9C"), "夜");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
