//! State and behaviour of the terminal client. Drawing lives in `view.rs`,
//! key and mouse handling in `input.rs`, the phone player in `phone.rs`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mpris_server::{LoopStatus, PlaybackStatus};
use ratatui::layout::Rect;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

use crate::audio::{AudioCommand, AudioEvent, AudioPlayer, MprisCommand, MprisUpdate, PlaybackState};
use crate::library::models::Track;
use crate::lyrics::{LyricLine, Lyrics, Ruby};
use crate::youtube::YtResult;

// ── Background work ──────────────────────────────────────────────────────────

/// Results of work done off the UI thread.
pub enum Job {
    Lyrics(PathBuf, Lyrics),
    LyricsAlt(PathBuf, Lyrics),
    Furigana(u64, Option<Vec<Vec<Ruby>>>),
    YtResults(u64, Result<Vec<YtResult>, String>),
    YtResolved(u64, Result<crate::youtube::Resolved, String>),
    YtChecked(Option<String>),
    YtPreview(String, Option<image::RgbaImage>),
    YtDownloaded(String, Result<Option<PathBuf>, String>),
    ImportDownloaded(Result<Option<PathBuf>, String>),
    Scanned(Vec<Track>, Rescan),
    Cover(PathBuf, Option<image::RgbaImage>),
    PhoneAddresses(Vec<String>),
}

/// Why the library was scanned again.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Rescan {
    Manual,
    AfterDownload,
    AfterImport,
}

// ── Navigation ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Nav,
    List,
    Lyrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Artist,
    Album,
    Genre,
}

impl Group {
    pub fn plural(self) -> &'static str {
        match self {
            Group::Artist => "Artists",
            Group::Album => "Albums",
            Group::Genre => "Genres",
        }
    }
}

/// An entry of the left-hand library list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nav {
    Songs,
    Groups(Group),
    Liked,
    Recent,
    MostPlayed,
    Playlist(String),
}

impl Nav {
    pub fn label(&self) -> String {
        match self {
            Nav::Songs => "Songs".into(),
            Nav::Groups(g) => g.plural().into(),
            Nav::Liked => "Liked Songs".into(),
            Nav::Recent => "Recently Played".into(),
            Nav::MostPlayed => "Most Played".into(),
            Nav::Playlist(name) => name.clone(),
        }
    }
}

/// What the middle list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Nav(Nav),
    /// The songs of one artist, album or genre.
    Group(Group, String),
}

pub enum Items {
    Tracks(Vec<usize>),
    Groups(Group, Vec<(String, usize)>),
}

impl Items {
    pub fn len(&self) -> usize {
        match self {
            Items::Tracks(t) => t.len(),
            Items::Groups(_, g) => g.len(),
        }
    }
}

// ── Dialogs ──────────────────────────────────────────────────────────────────

/// A one-line text field.
#[derive(Default, Clone)]
pub struct Input {
    pub text: String,
    /// Cursor position in chars.
    pub cursor: usize,
}

impl Input {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.chars().count();
        Input { text, cursor }
    }
    fn byte(&self, char_idx: usize) -> usize {
        self.text.char_indices().nth(char_idx).map_or(self.text.len(), |(b, _)| b)
    }
    pub fn insert(&mut self, c: char) {
        let at = self.byte(self.cursor);
        self.text.insert(at, c);
        self.cursor += 1;
    }
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let at = self.byte(self.cursor);
            self.text.remove(at);
        }
    }
    pub fn delete(&mut self) {
        if self.cursor < self.text.chars().count() {
            let at = self.byte(self.cursor);
            self.text.remove(at);
        }
    }
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.text.chars().count());
    }
    pub fn home(&mut self) {
        self.cursor = 0;
    }
    pub fn end(&mut self) {
        self.cursor = self.text.chars().count();
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }
    /// Deletes the word before the cursor (Ctrl+W).
    pub fn delete_word(&mut self) {
        while self.cursor > 0 && self.text.chars().nth(self.cursor - 1) == Some(' ') {
            self.backspace();
        }
        while self.cursor > 0 && self.text.chars().nth(self.cursor - 1) != Some(' ') {
            self.backspace();
        }
    }
}

/// What a text prompt is for (all of them are about playlists).
#[derive(Clone)]
pub enum PlaylistPrompt {
    New { then_add: Option<PathBuf> },
    Rename(String),
    Export(String),
    Import,
}

pub struct Prompt {
    pub title: String,
    pub hint: String,
    pub input: Input,
    pub purpose: PlaylistPrompt,
}

pub struct YtDialog {
    pub input: Input,
    pub results: Vec<YtResult>,
    pub cursor: usize,
    /// The search box has the keyboard (else the result list).
    pub typing: bool,
    pub searching: bool,
    pub search_id: u64,
    pub message: Option<String>,
    pub downloading: HashSet<String>,
    pub saved: HashSet<String>,
    /// Set when the results are a pasted playlist or album: its name.
    pub list_title: Option<String>,
    /// Large cover of the selected result (16:9).
    pub preview: Option<(String, image::RgbaImage)>,
}

pub const TAG_FIELDS: [&str; 7] = ["Title", "Artist", "Album", "Genre", "Year", "Track", "Disc"];

pub struct TagEditor {
    pub path: PathBuf,
    pub fields: Vec<Input>,
    pub cursor: usize,
    pub error: Option<String>,
}

pub struct Settings {
    pub cursor: usize,
}

pub enum Modal {
    Help,
    Prompt(Prompt),
    AddToPlaylist { path: PathBuf, cursor: usize },
    ConfirmDelete(String),
    YouTube(YtDialog),
    Tags(TagEditor),
    Settings(Settings),
    Phone,
}

// ── Status line ──────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Status {
    pub text: String,
    pub error: bool,
    until: Option<Instant>,
}

// ── Output ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Laptop,
    Phone,
}

/// Screen areas from the last frame, for mouse clicks.
#[derive(Default, Clone)]
pub struct Areas {
    pub nav: Rect,
    pub list: Rect,
    /// First row of list items (below the header row).
    pub list_rows: Rect,
    pub list_offset: usize,
    pub nav_offset: usize,
    /// Screen row of each library entry.
    pub nav_rows: Vec<(u16, usize)>,
    /// The pane tabs shown on narrow screens.
    pub tabs: Vec<(Rect, Pane)>,
    pub lyrics: Rect,
    /// Line index of each screen row in the lyrics pane.
    pub lyric_rows: Vec<(u16, usize)>,
    pub progress: Rect,
    pub modal_rows: Rect,
    pub modal_offset: usize,
}

struct ImportJob {
    name: String,
    rows: Vec<crate::playlist_io::Row>,
    pending: VecDeque<String>,
    total: usize,
    done: usize,
    failed: usize,
}

// ── App ──────────────────────────────────────────────────────────────────────

/// What the phone page was last told: (song, next, skip, playing).
type PhoneKey = (Option<i64>, Option<i64>, Option<i64>, bool);

pub struct App {
    pub rt: Handle,
    jobs: UnboundedSender<Job>,

    // Library
    pub tracks: Vec<Track>,
    pub nav: Vec<Nav>,
    pub nav_cursor: usize,
    pub source: Source,
    pub items: Items,
    pub cursor: usize,
    pub query: Input,
    pub searching: bool,
    pub pane: Pane,
    /// Shown one pane at a time (narrow terminal, e.g. a phone).
    pub narrow: bool,
    /// Big cover and lyrics instead of the library (`t`).
    pub now_view: bool,

    // Playback
    pub player: AudioPlayer,
    pub state: PlaybackState,
    pub current: Option<usize>,
    queue: Vec<usize>,
    queue_pos: usize,
    /// Chosen when a song starts, so the phone can load it in advance.
    shuffle_next: Option<usize>,
    pub shuffle: bool,
    pub repeat: bool,
    pub position: Duration,
    pub duration: Duration,
    pub volume: f32,
    pub spectrum: Vec<f32>,
    pub show_spectrum: bool,
    pub spectrum_mirrored: bool,

    // Lyrics
    pub show_lyrics: bool,
    pub lyrics: HashMap<PathBuf, Lyrics>,
    lyrics_loading: HashSet<PathBuf>,
    pub lyrics_alt: HashMap<PathBuf, Lyrics>,
    alt_loading: HashSet<PathBuf>,
    pub furigana: HashMap<u64, Option<Vec<Vec<Ruby>>>>,
    furigana_loading: HashSet<u64>,
    pub romaji: bool,
    pub show_furigana: bool,
    pub lyrics_offset: f32,
    /// Selected lyric line while the lyrics pane has the keyboard.
    pub lyric_cursor: Option<usize>,
    /// When the listener last moved through the lyrics; after
    /// `LYRIC_PAUSE` without that, they follow the song again.
    pub lyric_touched: Option<Instant>,
    /// Bumped whenever the shown lyrics may have changed (for the phone).
    lyrics_rev: u64,

    // Cover art (square, 64 px) of the playing song
    pub cover: Option<(PathBuf, image::RgbaImage)>,

    // Dialogs
    pub modal: Option<Modal>,
    import_job: Option<ImportJob>,
    yt_search_seq: u64,

    // Phone
    pub output: Output,
    pub phone: Option<crate::phone::Server>,
    /// Where the phone can reach the player (no key in them).
    pub phone_addresses: Vec<String>,
    /// The pairing code shown in the phone dialog.
    pub phone_code: Option<String>,
    /// Last state sent to the page; `None` = send it again.
    phone_published: Option<PhoneKey>,
    phone_lyrics_published: (Option<i64>, u64, i64),
    pub phone_seen: Option<Instant>,

    // Look
    pub theme_mode: String,
    pub theme_preset: String,
    pub lyrics_online: bool,
    theme_name: String,
    theme_checked: Instant,

    // MPRIS
    mpris_tx: UnboundedSender<MprisUpdate>,

    pub status: Status,
    pub areas: Areas,
    pub last_click: Option<(Instant, u16, u16)>,
    pub should_quit: bool,
}

impl App {
    pub fn new(rt: Handle, jobs: UnboundedSender<Job>, player: AudioPlayer, mpris_tx: UnboundedSender<MprisUpdate>) -> Self {
        let cfg = crate::config::get();
        let saved = crate::state::load();
        let tracks = crate::library::scanner::scan_folder(&cfg.music_path());
        let mut app = App {
            rt,
            jobs,
            tracks,
            nav: Vec::new(),
            nav_cursor: 0,
            source: Source::Nav(Nav::Songs),
            items: Items::Tracks(Vec::new()),
            cursor: 0,
            query: Input::default(),
            searching: false,
            pane: Pane::List,
            narrow: false,
            now_view: false,
            player,
            state: PlaybackState::Stopped,
            current: None,
            queue: Vec::new(),
            queue_pos: 0,
            shuffle_next: None,
            shuffle: cfg.shuffle,
            repeat: cfg.repeat,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            volume: saved.volume.unwrap_or(cfg.volume).clamp(0.0, 1.0),
            spectrum: vec![0.0; crate::audio::spectrum::NUM_BARS],
            show_spectrum: true,
            spectrum_mirrored: cfg.spectrum_style != "bottom",
            show_lyrics: saved.show_lyrics.unwrap_or(true),
            lyrics: HashMap::new(),
            lyrics_loading: HashSet::new(),
            lyrics_alt: HashMap::new(),
            alt_loading: HashSet::new(),
            furigana: HashMap::new(),
            furigana_loading: HashSet::new(),
            romaji: saved.lyrics_romaji.unwrap_or(false),
            show_furigana: true,
            lyrics_offset: 0.0,
            lyric_cursor: None,
            lyric_touched: None,
            lyrics_rev: 0,
            cover: None,
            modal: None,
            import_job: None,
            yt_search_seq: 0,
            output: Output::Laptop,
            phone: None,
            phone_addresses: Vec::new(),
            phone_code: None,
            phone_published: None,
            phone_lyrics_published: (None, u64::MAX, 0),
            phone_seen: None,
            theme_mode: cfg.theme_mode.clone(),
            theme_preset: cfg.theme_preset.clone(),
            lyrics_online: cfg.lyrics_online,
            theme_name: crate::theme::read_current_theme_name(),
            theme_checked: Instant::now(),
            mpris_tx,
            status: Status::default(),
            areas: Areas::default(),
            last_click: None,
            should_quit: false,
        };
        app.player.send(AudioCommand::SetVolume(app.volume));
        app.rebuild_nav();
        app.refresh_items();
        app.publish_initial_mpris();
        app
    }

    // ── Status ───────────────────────────────────────────────────────────────

    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Status { text: text.into(), error: false, until: Some(Instant::now() + Duration::from_secs(4)) };
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.status = Status { text: text.into(), error: true, until: Some(Instant::now() + Duration::from_secs(8)) };
    }

    /// Once per frame: expire the status line, let the lyrics follow the
    /// song again after a pause in browsing them, follow the Omarchy theme.
    pub fn tick(&mut self) {
        if self.status.until.is_some_and(|t| Instant::now() >= t) {
            self.status = Status::default();
        }
        if self.lyric_cursor.is_some() && self.lyric_touched.is_none_or(|t| t.elapsed() >= LYRIC_PAUSE) {
            self.lyric_cursor = None;
        }
        if self.theme_checked.elapsed() >= Duration::from_secs(3) {
            self.theme_checked = Instant::now();
            if self.theme_mode != "manual" {
                let name = crate::theme::read_current_theme_name();
                if name != self.theme_name {
                    self.theme_name = name;
                    crate::theme::reload_with_mode(&self.theme_mode, &self.theme_preset);
                }
            }
        }
    }

    // ── Library ──────────────────────────────────────────────────────────────

    pub fn rebuild_nav(&mut self) {
        let selected = self.nav.get(self.nav_cursor).cloned();
        let playlists = crate::persist::playlist_names();
        self.nav = vec![
            Nav::Songs,
            Nav::Groups(Group::Artist),
            Nav::Groups(Group::Album),
            Nav::Groups(Group::Genre),
            Nav::Liked,
            Nav::Recent,
            Nav::MostPlayed,
        ];
        self.nav.extend(playlists.into_iter().map(Nav::Playlist));
        self.nav_cursor = selected.and_then(|s| self.nav.iter().position(|n| *n == s)).unwrap_or(0);
    }

    /// The playlists, in the order the library list shows them.
    pub fn playlists(&self) -> Vec<String> {
        self.nav.iter().filter_map(|n| if let Nav::Playlist(p) = n { Some(p.clone()) } else { None }).collect()
    }

    pub fn first_playlist_nav(&self) -> usize {
        self.nav.iter().position(|n| matches!(n, Nav::Playlist(_))).unwrap_or(self.nav.len())
    }

    /// Shows what the library list's selection points at.
    pub fn select_nav(&mut self, idx: usize) {
        if let Some(nav) = self.nav.get(idx).cloned() {
            self.nav_cursor = idx;
            self.source = Source::Nav(nav);
            self.query.clear();
            self.searching = false;
            self.cursor = 0;
            self.refresh_items();
        }
    }

    fn group_key(t: &Track, g: Group) -> &str {
        match g {
            Group::Artist => &t.artist,
            Group::Album => &t.album,
            Group::Genre => &t.genre,
        }
    }

    fn library_order(&self, mut idx: Vec<usize>) -> Vec<usize> {
        idx.sort_by(|&a, &b| {
            let (x, y) = (&self.tracks[a], &self.tracks[b]);
            x.artist
                .to_lowercase()
                .cmp(&y.artist.to_lowercase())
                .then(x.album.to_lowercase().cmp(&y.album.to_lowercase()))
                .then(x.disc_number.cmp(&y.disc_number))
                .then(x.track_number.cmp(&y.track_number))
                .then(x.title.to_lowercase().cmp(&y.title.to_lowercase()))
        });
        idx
    }

    fn by_paths(&self, paths: &[PathBuf]) -> Vec<usize> {
        let index: HashMap<&Path, usize> = self.tracks.iter().enumerate().map(|(i, t)| (t.path.as_path(), i)).collect();
        paths.iter().filter_map(|p| index.get(p.as_path()).copied()).collect()
    }

    /// Recomputes the middle list from `source` and the search text.
    pub fn refresh_items(&mut self) {
        let q = self.query.text.to_lowercase();
        let matches = |t: &Track| {
            q.is_empty()
                || t.title.to_lowercase().contains(&q)
                || t.artist.to_lowercase().contains(&q)
                || t.album.to_lowercase().contains(&q)
                || t.genre.to_lowercase().contains(&q)
        };
        let all: Vec<usize> = (0..self.tracks.len()).collect();
        self.items = match &self.source {
            Source::Nav(Nav::Songs) => Items::Tracks(self.library_order(all.into_iter().filter(|&i| matches(&self.tracks[i])).collect())),
            Source::Nav(Nav::Groups(g)) => {
                let g = *g;
                let mut counts: HashMap<String, usize> = HashMap::new();
                for t in &self.tracks {
                    *counts.entry(Self::group_key(t, g).to_string()).or_default() += 1;
                }
                let mut groups: Vec<(String, usize)> =
                    counts.into_iter().filter(|(name, _)| q.is_empty() || name.to_lowercase().contains(&q)).collect();
                groups.sort_by_key(|(name, _)| name.to_lowercase());
                Items::Groups(g, groups)
            }
            Source::Group(g, name) => {
                let (g, name) = (*g, name.clone());
                Items::Tracks(self.library_order(
                    all.into_iter().filter(|&i| Self::group_key(&self.tracks[i], g) == name && matches(&self.tracks[i])).collect(),
                ))
            }
            Source::Nav(Nav::Liked) => {
                let liked = crate::persist::get(|db| db.favorites.clone());
                Items::Tracks(self.library_order(
                    all.into_iter().filter(|&i| liked.contains(&self.tracks[i].path) && matches(&self.tracks[i])).collect(),
                ))
            }
            Source::Nav(Nav::Recent) => {
                let paths: Vec<PathBuf> = crate::persist::get(|db| db.recently_played.iter().map(|(p, _)| p.clone()).collect());
                Items::Tracks(self.by_paths(&paths).into_iter().filter(|&i| matches(&self.tracks[i])).collect())
            }
            Source::Nav(Nav::MostPlayed) => {
                let counts = crate::persist::get(|db| db.play_counts.clone());
                let mut idx: Vec<usize> = all
                    .into_iter()
                    .filter(|&i| counts.get(&self.tracks[i].path).copied().unwrap_or(0) > 0 && matches(&self.tracks[i]))
                    .collect();
                idx.sort_by_key(|&i| std::cmp::Reverse(counts.get(&self.tracks[i].path).copied().unwrap_or(0)));
                idx.truncate(100);
                Items::Tracks(idx)
            }
            Source::Nav(Nav::Playlist(name)) => {
                let paths = crate::persist::get(|db| db.playlists.get(name).cloned().unwrap_or_default());
                Items::Tracks(self.by_paths(&paths).into_iter().filter(|&i| matches(&self.tracks[i])).collect())
            }
        };
        self.cursor = self.cursor.min(self.items.len().saturating_sub(1));
    }

    pub fn list_title(&self) -> String {
        match &self.source {
            Source::Nav(n) => n.label(),
            Source::Group(g, name) => format!("{} › {}", g.plural(), if name.is_empty() { "(none)" } else { name }),
        }
    }

    pub fn selected_track(&self) -> Option<usize> {
        match &self.items {
            Items::Tracks(t) => t.get(self.cursor).copied(),
            Items::Groups(..) => None,
        }
    }

    pub fn current_playlist(&self) -> Option<String> {
        match &self.source {
            Source::Nav(Nav::Playlist(p)) => Some(p.clone()),
            _ => None,
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let n = self.items.len();
        if n > 0 {
            self.cursor = (self.cursor as isize + delta).clamp(0, n as isize - 1) as usize;
        }
    }

    /// Enter on the list: play a song, or open a group.
    pub fn activate(&mut self) {
        match &self.items {
            Items::Tracks(list) => {
                if !list.is_empty() {
                    let (queue, pos) = (list.clone(), self.cursor);
                    self.play_from(queue, pos);
                }
            }
            Items::Groups(g, groups) => {
                if let Some((name, _)) = groups.get(self.cursor) {
                    self.source = Source::Group(*g, name.clone());
                    self.query.clear();
                    self.cursor = 0;
                    self.refresh_items();
                }
            }
        }
    }

    /// Back out of a group (or clear the search).
    pub fn back(&mut self) -> bool {
        if let Source::Group(g, name) = self.source.clone() {
            self.source = Source::Nav(Nav::Groups(g));
            self.query.clear();
            self.refresh_items();
            if let Items::Groups(_, groups) = &self.items {
                self.cursor = groups.iter().position(|(n, _)| *n == name).unwrap_or(0);
            }
            return true;
        }
        if !self.query.text.is_empty() {
            self.query.clear();
            self.refresh_items();
            return true;
        }
        false
    }

    pub fn rescan(&mut self, why: Rescan) {
        if why == Rescan::Manual {
            self.info("Scanning the library…");
        }
        let dir = crate::config::get().music_path();
        let jobs = self.jobs.clone();
        self.rt.spawn(async move {
            let tracks = tokio::task::spawn_blocking(move || crate::library::scanner::scan_folder(&dir)).await.unwrap_or_default();
            let _ = jobs.send(Job::Scanned(tracks, why));
        });
    }

    fn on_scanned(&mut self, tracks: Vec<Track>, why: Rescan) {
        // Keep the playing song, the queue and the selection by path.
        let path_of = |i: usize, t: &[Track]| t.get(i).map(|t| t.path.clone());
        let current = self.current.and_then(|i| path_of(i, &self.tracks));
        let queue: Vec<PathBuf> = self.queue.iter().filter_map(|&i| path_of(i, &self.tracks)).collect();
        let next = self.shuffle_next.and_then(|i| path_of(i, &self.tracks));
        self.tracks = tracks;
        let find = |p: &PathBuf, t: &[Track]| t.iter().position(|x| &x.path == p);
        self.current = current.and_then(|p| find(&p, &self.tracks));
        self.queue = queue.iter().filter_map(|p| find(p, &self.tracks)).collect();
        if let Some(c) = self.current {
            self.queue_pos = self.queue.iter().position(|&i| i == c).unwrap_or(0);
        }
        self.shuffle_next = next.and_then(|p| find(&p, &self.tracks));
        self.refresh_items();
        if let Some(phone) = &self.phone {
            phone.set_library(&self.tracks);
        }
        match why {
            Rescan::Manual => {
                let n = self.tracks.len();
                self.info(format!("Library rescanned: {n} songs"));
            }
            Rescan::AfterDownload => {}
            Rescan::AfterImport => self.finish_import(),
        }
    }

    // ── Playback ─────────────────────────────────────────────────────────────

    fn play_from(&mut self, queue: Vec<usize>, pos: usize) {
        self.queue = queue;
        self.queue_pos = pos.min(self.queue.len().saturating_sub(1));
        if let Some(&idx) = self.queue.get(self.queue_pos) {
            self.start(idx);
        }
    }

    /// Starts a song from its beginning, on the current output.
    fn start(&mut self, idx: usize) {
        let Some(path) = self.tracks.get(idx).map(|t| t.path.clone()) else { return };
        self.start_bookkeeping(idx);
        match self.output {
            Output::Laptop => self.player.send(AudioCommand::Play(path)),
            Output::Phone => {
                if let Some(phone) = &self.phone {
                    phone.command(crate::phone::Cmd::Load { at: 0.0, play: true });
                }
            }
        }
    }

    fn pick_shuffle_next(&self) -> Option<usize> {
        if !self.shuffle || self.queue.len() < 2 {
            return None;
        }
        use rand::Rng;
        let mut rng = rand::thread_rng();
        loop {
            let i = self.queue[rng.gen_range(0..self.queue.len())];
            if Some(i) != self.current {
                return Some(i);
            }
        }
    }

    /// The song that follows the current one at its end, if any.
    pub fn up_next(&self) -> Option<usize> {
        let cur = self.current?;
        if self.repeat {
            return Some(cur);
        }
        if self.shuffle {
            return self.shuffle_next;
        }
        self.queue.get(self.queue_pos + 1).copied()
    }

    /// Where Next goes: like `up_next`, but ignoring repeat and wrapping
    /// around at the end of the list.
    pub fn skip_target(&self) -> Option<usize> {
        if self.shuffle {
            self.shuffle_next
        } else if self.queue.is_empty() {
            None
        } else {
            Some(self.queue[(self.queue_pos + 1) % self.queue.len()])
        }
    }

    pub fn next(&mut self) {
        match self.skip_target() {
            Some(i) => self.start(i),
            None => self.play_selected_or_resume(),
        }
    }

    pub fn previous(&mut self) {
        if self.position > Duration::from_secs(3) || self.queue.is_empty() {
            self.seek_to(Duration::ZERO);
            return;
        }
        let pos = (self.queue_pos + self.queue.len() - 1) % self.queue.len();
        let idx = self.queue[pos];
        self.start(idx);
    }

    fn on_track_ended(&mut self) {
        match self.up_next() {
            Some(i) => self.start(i),
            None => self.stop(),
        }
    }

    pub fn stop(&mut self) {
        self.player.send(AudioCommand::Stop);
        if let Some(phone) = &self.phone {
            phone.command(crate::phone::Cmd::Pause);
        }
        self.state = PlaybackState::Stopped;
        self.position = Duration::ZERO;
        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Stopped));
    }

    fn play_selected_or_resume(&mut self) {
        if let Some(i) = self.selected_track() {
            let queue = match &self.items {
                Items::Tracks(t) => t.clone(),
                _ => vec![i],
            };
            self.play_from(queue, self.cursor);
        }
    }

    pub fn toggle_play(&mut self) {
        match self.state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused => self.resume(),
            PlaybackState::Stopped => match self.current {
                Some(i) => self.start(i),
                None => self.play_selected_or_resume(),
            },
        }
    }

    pub fn pause(&mut self) {
        if self.state != PlaybackState::Playing {
            return;
        }
        match self.output {
            Output::Laptop => self.player.send(AudioCommand::Pause),
            Output::Phone => {
                if let Some(phone) = &self.phone {
                    phone.command(crate::phone::Cmd::Pause);
                }
            }
        }
        self.state = PlaybackState::Paused;
        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Paused));
    }

    pub fn resume(&mut self) {
        if self.state != PlaybackState::Paused {
            return;
        }
        match self.output {
            Output::Laptop => self.player.send(AudioCommand::Resume),
            Output::Phone => {
                if let Some(phone) = &self.phone {
                    phone.command(crate::phone::Cmd::Play);
                }
            }
        }
        self.state = PlaybackState::Playing;
        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Playing));
    }

    pub fn seek_to(&mut self, at: Duration) {
        if self.current.is_none() {
            return;
        }
        let at = if self.duration > Duration::ZERO { at.min(self.duration) } else { at };
        match self.output {
            Output::Laptop => self.player.send(AudioCommand::Seek(at)),
            Output::Phone => {
                if let Some(phone) = &self.phone {
                    phone.command(crate::phone::Cmd::Seek(at.as_secs_f64()));
                }
            }
        }
        self.position = at;
    }

    pub fn seek_by(&mut self, secs: f64) {
        let target = (self.position.as_secs_f64() + secs).max(0.0);
        self.seek_to(Duration::from_secs_f64(target));
    }

    pub fn volume_by(&mut self, delta: f32) {
        if self.output == Output::Phone {
            self.info("Playing on the phone: use its volume buttons");
            return;
        }
        self.volume = ((self.volume + delta) * 100.0).round() / 100.0;
        self.volume = self.volume.clamp(0.0, 1.0);
        self.player.send(AudioCommand::SetVolume(self.volume));
        self.send_mpris(MprisUpdate::Volume(self.volume as f64));
        self.save_state();
        let v = (self.volume * 100.0).round();
        self.info(format!("Volume {v}%"));
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.shuffle_next = self.pick_shuffle_next();
        self.send_mpris(MprisUpdate::Shuffle(self.shuffle));
        self.info(if self.shuffle { "Shuffle on" } else { "Shuffle off" });
    }

    pub fn toggle_repeat(&mut self) {
        self.repeat = !self.repeat;
        let status = if self.repeat { LoopStatus::Track } else { LoopStatus::None };
        self.send_mpris(MprisUpdate::Loop(status));
        self.info(if self.repeat { "Repeat this song on" } else { "Repeat off" });
    }

    pub fn toggle_like(&mut self) {
        let Some(idx) = self.selected_track().or(self.current) else { return };
        let path = self.tracks[idx].path.clone();
        let liked = crate::persist::toggle_favorite(path);
        self.tracks[idx].liked = liked;
        let title = self.tracks[idx].title.clone();
        self.info(if liked { format!("Liked “{title}”") } else { format!("Unliked “{title}”") });
        if matches!(self.source, Source::Nav(Nav::Liked)) {
            self.refresh_items();
        }
    }

    pub fn is_liked(&self, idx: usize) -> bool {
        let path = &self.tracks[idx].path;
        crate::persist::get(|db| db.favorites.contains(path))
    }

    pub fn play_count(&self, idx: usize) -> u32 {
        let path = &self.tracks[idx].path;
        crate::persist::get(|db| db.play_counts.get(path).copied().unwrap_or(0))
    }

    pub fn on_audio(&mut self, ev: AudioEvent) {
        if self.output == Output::Phone {
            return; // the laptop's player is idle while the phone plays
        }
        match ev {
            AudioEvent::Playing => self.state = PlaybackState::Playing,
            AudioEvent::Paused => self.state = PlaybackState::Paused,
            AudioEvent::Stopped => {}
            AudioEvent::Progress { position, duration } => {
                self.position = position;
                if duration > Duration::ZERO {
                    self.duration = duration;
                }
            }
            AudioEvent::Error(e) => self.error(format!("Playback: {e}")),
            AudioEvent::TrackEnded => self.on_track_ended(),
        }
    }

    fn save_state(&self) {
        let mut s = crate::state::load();
        s.volume = Some(self.volume);
        s.show_lyrics = Some(self.show_lyrics);
        s.lyrics_romaji = Some(self.romaji);
        crate::state::save(&s);
    }

    // ── MPRIS and notifications ──────────────────────────────────────────────

    fn send_mpris(&self, update: MprisUpdate) {
        let _ = self.mpris_tx.send(update);
    }

    fn publish_initial_mpris(&self) {
        self.send_mpris(MprisUpdate::Volume(self.volume as f64));
        self.send_mpris(MprisUpdate::Shuffle(self.shuffle));
        self.send_mpris(MprisUpdate::Loop(if self.repeat { LoopStatus::Track } else { LoopStatus::None }));
        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Stopped));
    }

    fn publish_track_mpris(&self, track: &Track, art_url: Option<String>) {
        self.send_mpris(MprisUpdate::Metadata {
            title: track.title.clone(),
            artist: display_artist(track),
            album: track.album.clone(),
            duration_us: track.duration.as_micros() as i64,
            art_url,
        });
        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Playing));
    }

    pub fn on_mpris(&mut self, cmd: MprisCommand) {
        match cmd {
            MprisCommand::Play => {
                if self.state == PlaybackState::Paused {
                    self.resume();
                } else if self.state == PlaybackState::Stopped {
                    self.toggle_play();
                }
            }
            MprisCommand::Pause => self.pause(),
            MprisCommand::PlayPause => self.toggle_play(),
            MprisCommand::Next => self.next(),
            MprisCommand::Previous => self.previous(),
            MprisCommand::Stop => self.stop(),
        }
    }

    // ── Cover art ────────────────────────────────────────────────────────────

    fn load_cover(&mut self, path: &Path) {
        if self.cover.as_ref().is_some_and(|(p, _)| p == path) {
            return;
        }
        self.cover = None;
        let (path, jobs) = (path.to_path_buf(), self.jobs.clone());
        self.rt.spawn(async move {
            let p = path.clone();
            let art = tokio::task::spawn_blocking(move || {
                let bytes = crate::library::scanner::load_cover(&p)?;
                publish_cover_file(&bytes);
                let img = image::load_from_memory(&bytes).ok()?;
                Some(crate::cover::square_cover(&img, 96))
            })
            .await
            .ok()
            .flatten();
            let _ = jobs.send(Job::Cover(path, art));
        });
    }

    // ── Lyrics ───────────────────────────────────────────────────────────────

    pub fn current_track(&self) -> Option<&Track> {
        self.current.and_then(|i| self.tracks.get(i))
    }

    /// Japanese by its lyrics or by its title/artist.
    pub fn is_japanese_song(&self, track: &Track) -> bool {
        use crate::lyrics::{has_japanese, lines_have_japanese, lines_look_like_romaji};
        let lyrics_jp = match self.lyrics.get(&track.path) {
            Some(Lyrics::Synced(l) | Lyrics::Plain(l)) => lines_have_japanese(l) || lines_look_like_romaji(l),
            _ => false,
        };
        lyrics_jp || has_japanese(&format!("{} {}", track.title, track.artist))
    }

    /// `Some(true)`: show romaji made from these lyrics; `Some(false)`: show
    /// Japanese found for romaji lyrics; `None`: the lyrics are right as they are.
    fn wanted_alt(&self, track: &Track) -> Option<bool> {
        let Some(Lyrics::Synced(lines) | Lyrics::Plain(lines)) = self.lyrics.get(&track.path) else { return None };
        let japanese = crate::lyrics::lines_have_japanese(lines);
        if self.romaji && japanese {
            Some(true)
        } else if !self.romaji && !japanese && self.is_japanese_song(track) {
            Some(false)
        } else {
            None
        }
    }

    /// The lyrics to show for `track`, and a note when the wanted version
    /// isn't available (yet).
    pub fn shown_lyrics(&self, track: &Track) -> (Option<&Lyrics>, Option<String>) {
        let primary = self.lyrics.get(&track.path);
        let Some(romaji) = self.wanted_alt(track) else { return (primary, None) };
        match self.lyrics_alt.get(&track.path) {
            Some(alt @ (Lyrics::Synced(_) | Lyrics::Plain(_))) => (Some(alt), None),
            Some(Lyrics::Error(e)) => (primary, Some(e.clone())),
            Some(_) => (primary, Some("No Japanese version on LRCLIB".into())),
            None if romaji => (primary, Some("Converting to romaji…".into())),
            None => (primary, Some("Looking for the Japanese lyrics…".into())),
        }
    }

    /// Starts every lookup the playing song's lyrics still need.
    pub fn ensure_lyrics(&mut self) {
        let Some(track) = self.current_track().cloned() else { return };
        let path = track.path.clone();
        let done = matches!(self.lyrics.get(&path), Some(l) if !matches!(l, Lyrics::Error(_)));
        if !done {
            if self.lyrics_loading.insert(path.clone()) {
                let (q, jobs) = (crate::lyrics::Query::from(&track), self.jobs.clone());
                self.rt.spawn(async move {
                    let lyrics = crate::lyrics::load(q).await;
                    let _ = jobs.send(Job::Lyrics(path, lyrics));
                });
            }
            return;
        }
        // The other script, when the switch asks for it.
        if let Some(romaji) = self.wanted_alt(&track) {
            let have = matches!(self.lyrics_alt.get(&path), Some(l) if !matches!(l, Lyrics::Error(_)));
            if !have && self.alt_loading.insert(path.clone()) {
                let (lines, synced) = match self.lyrics.get(&path) {
                    Some(Lyrics::Synced(l)) => (l.clone(), true),
                    Some(Lyrics::Plain(l)) => (l.clone(), false),
                    _ => return,
                };
                let (q, jobs) = (crate::lyrics::Query::from(&track), self.jobs.clone());
                self.rt.spawn(async move {
                    let alt = if romaji {
                        crate::lyrics::romaji(q, lines, synced).await
                    } else {
                        crate::lyrics::japanese(q, lines, synced).await
                    };
                    let _ = jobs.send(Job::LyricsAlt(path, alt));
                });
            }
        }
        // Furigana for Japanese on screen.
        if self.show_furigana {
            let lines = match self.shown_lyrics(&track).0 {
                Some(Lyrics::Synced(l) | Lyrics::Plain(l)) if crate::lyrics::wants_furigana(l) => l.clone(),
                _ => return,
            };
            let key = furigana_key(&lines);
            if !self.furigana.contains_key(&key) && self.furigana_loading.insert(key) {
                let texts: Vec<String> = lines.into_iter().map(|l| l.text).collect();
                let jobs = self.jobs.clone();
                self.rt.spawn(async move {
                    let ruby = crate::lyrics::furigana(texts).await.ok();
                    let _ = jobs.send(Job::Furigana(key, ruby));
                });
            }
        }
    }

    pub fn furigana_for(&self, lines: &[LyricLine]) -> Option<&Vec<Vec<Ruby>>> {
        if !self.show_furigana {
            return None;
        }
        self.furigana.get(&furigana_key(lines))?.as_ref()
    }

    pub fn toggle_lyrics(&mut self) {
        self.show_lyrics = !self.show_lyrics;
        if !self.show_lyrics && self.pane == Pane::Lyrics {
            self.pane = Pane::List;
        }
        self.save_state();
    }

    pub fn toggle_romaji(&mut self) {
        let Some(track) = self.current_track().cloned() else {
            self.info("Play a Japanese song to switch between Japanese and romaji");
            return;
        };
        if !self.is_japanese_song(&track) {
            self.info("This song's lyrics aren't Japanese");
            return;
        }
        self.romaji = !self.romaji;
        self.lyrics_rev += 1;
        self.save_state();
        self.ensure_lyrics();
        self.info(if self.romaji { "Lyrics in romaji" } else { "Lyrics in Japanese" });
    }

    pub fn toggle_furigana(&mut self) {
        self.show_furigana = !self.show_furigana;
        self.ensure_lyrics();
        self.info(if self.show_furigana { "Furigana on" } else { "Furigana off" });
    }

    pub fn set_lyrics_offset(&mut self, secs: f32) {
        let Some(track) = self.current_track() else { return };
        let path = track.path.clone();
        if !matches!(self.lyrics.get(&path), Some(Lyrics::Synced(_))) {
            self.info("This song has no synced lyrics to shift");
            return;
        }
        self.lyrics_offset = (secs * 10.0).round() / 10.0;
        crate::persist::set_lyrics_offset(path, self.lyrics_offset);
        self.lyrics_rev += 1;
        let o = self.lyrics_offset;
        self.info(if o == 0.0 { "Lyrics timing reset".to_string() } else { format!("Lyrics shifted {o:+.1} s") });
    }

    /// Makes `line_time` the line being sung now.
    pub fn sync_lyric_line(&mut self, line_time: Duration) {
        let secs = self.position.as_secs_f64() - line_time.as_secs_f64();
        self.set_lyrics_offset(secs as f32);
    }

    /// The song position the lyrics are matched against.
    pub fn lyric_position(&self) -> Duration {
        Duration::from_secs_f64((self.position.as_secs_f64() - self.lyrics_offset as f64).max(0.0))
    }

    /// Seeks to where a lyric line is sung.
    pub fn seek_to_line(&mut self, line_time: Duration) {
        let at = (line_time.as_secs_f64() + self.lyrics_offset as f64).max(0.0);
        self.seek_to(Duration::from_secs_f64(at));
    }

    // ── Background results ───────────────────────────────────────────────────

    pub fn on_job(&mut self, job: Job) {
        match job {
            Job::Lyrics(path, lyrics) => {
                self.lyrics_loading.remove(&path);
                self.lyrics.insert(path, lyrics);
                self.lyrics_rev += 1;
                self.ensure_lyrics();
            }
            Job::LyricsAlt(path, alt) => {
                self.alt_loading.remove(&path);
                self.lyrics_alt.insert(path, alt);
                self.lyrics_rev += 1;
                self.ensure_lyrics();
            }
            Job::Furigana(key, ruby) => {
                self.furigana_loading.remove(&key);
                self.furigana.insert(key, ruby);
            }
            Job::Cover(path, art) => {
                if self.current_track().is_some_and(|t| t.path == path) {
                    if let Some(img) = art {
                        self.cover = Some((path, img));
                        if let Some(track) = self.current_track().cloned() {
                            if let Some(url) = cover_file_url() {
                                self.send_mpris(MprisUpdate::Metadata {
                                    title: track.title.clone(),
                                    artist: display_artist(&track),
                                    album: track.album.clone(),
                                    duration_us: track.duration.as_micros() as i64,
                                    art_url: Some(url),
                                });
                            }
                        }
                    }
                }
            }
            Job::YtResults(id, result) => {
                if let Some(Modal::YouTube(d)) = &mut self.modal {
                    if d.search_id == id {
                        d.searching = false;
                        d.list_title = None;
                        match result {
                            Ok(results) => {
                                d.message = results.is_empty().then(|| "Nothing found".to_string());
                                d.results = results;
                                d.cursor = 0;
                                d.preview = None;
                                d.typing = d.results.is_empty();
                            }
                            Err(e) => d.message = Some(e),
                        }
                    }
                }
                self.youtube_preview();
            }
            Job::YtResolved(id, result) => {
                let still = matches!(&self.modal, Some(Modal::YouTube(d)) if d.search_id == id);
                if still {
                    match result {
                        Ok(r) => {
                            let (title, n) = (r.list_title.clone(), r.items.len());
                            self.on_job(Job::YtResults(id, Ok(r.items)));
                            if let Some(Modal::YouTube(d)) = &mut self.modal {
                                d.list_title = title.clone();
                                if n > 0 {
                                    d.message = Some(match title {
                                        Some(t) => format!("“{t}”: {n} songs. Enter downloads one, A downloads all into a playlist"),
                                        None => "Enter downloads it".to_string(),
                                    });
                                }
                            }
                        }
                        Err(e) => self.on_job(Job::YtResults(id, Err(e))),
                    }
                }
            }
            Job::YtChecked(err) => {
                if let (Some(err), Some(Modal::YouTube(_))) = (err, &self.modal) {
                    self.modal = None;
                    self.error(err);
                }
            }
            Job::YtPreview(id, img) => {
                if let (Some(Modal::YouTube(d)), Some(img)) = (&mut self.modal, img) {
                    if d.results.get(d.cursor).is_some_and(|r| r.id == id) {
                        d.preview = Some((id, img));
                    }
                }
            }
            Job::YtDownloaded(url, result) => {
                if let Some(Modal::YouTube(d)) = &mut self.modal {
                    d.downloading.remove(&url);
                    if result.is_ok() {
                        d.saved.insert(url.clone());
                    }
                }
                match result {
                    Ok(_) => {
                        self.info("Downloaded into your library");
                        self.rescan(Rescan::AfterDownload);
                    }
                    Err(e) => self.error(format!("Download failed: {e}")),
                }
            }
            Job::ImportDownloaded(result) => {
                if let Some(job) = &mut self.import_job {
                    match result {
                        Ok(_) => job.done += 1,
                        Err(_) => job.failed += 1,
                    }
                }
                self.import_next();
            }
            Job::Scanned(tracks, why) => self.on_scanned(tracks, why),
            Job::PhoneAddresses(a) => self.phone_addresses = a,
        }
    }

    // ── YouTube ──────────────────────────────────────────────────────────────

    pub fn open_youtube(&mut self) {
        // yt-dlp is checked in the background: the dialog opens at once.
        let jobs = self.jobs.clone();
        self.rt.spawn(async move {
            let err = tokio::task::spawn_blocking(|| crate::youtube::preflight(crate::config::get())).await.ok().flatten();
            let _ = jobs.send(Job::YtChecked(err));
        });
        self.modal = Some(Modal::YouTube(YtDialog {
            input: Input::default(),
            results: Vec::new(),
            cursor: 0,
            typing: true,
            searching: false,
            search_id: 0,
            message: None,
            downloading: HashSet::new(),
            saved: HashSet::new(),
            list_title: None,
            preview: None,
        }));
    }

    /// Loads the large cover of the selected YouTube result.
    pub fn youtube_preview(&mut self) {
        let Some(Modal::YouTube(d)) = &self.modal else { return };
        let Some(r) = d.results.get(d.cursor) else { return };
        if d.preview.as_ref().is_some_and(|(id, _)| *id == r.id) {
            return;
        }
        let (id, jobs) = (r.id.clone(), self.jobs.clone());
        self.rt.spawn(async move {
            let bytes = crate::youtube::thumbnail_large(id.clone()).await;
            let img = tokio::task::spawn_blocking(move || {
                let img = image::load_from_memory(bytes.as_deref()?).ok()?;
                Some(img.resize_to_fill(160, 90, image::imageops::FilterType::Triangle).to_rgba8())
            })
            .await
            .ok()
            .flatten();
            let _ = jobs.send(Job::YtPreview(id, img));
        });
    }

    /// Downloads every song of a pasted playlist or album into a playlist.
    pub fn youtube_download_all(&mut self) {
        let Some(Modal::YouTube(d)) = &self.modal else { return };
        let Some(stem) = d.list_title.clone() else {
            self.info("Paste a playlist or album link to download all of it");
            return;
        };
        let links: HashMap<String, String> = d.results.iter().map(|r| (r.id.clone(), r.url.clone())).collect();
        let rows: Vec<crate::playlist_io::Row> =
            d.results.iter().map(|r| crate::playlist_io::Row { id: r.id.clone(), title: r.title.clone(), artist: r.uploader.clone() }).collect();
        self.modal = None;
        self.begin_import(stem, rows, move |r| links.get(&r.id).cloned().unwrap_or_default());
    }

    pub fn youtube_search(&mut self) {
        self.yt_search_seq += 1;
        let id = self.yt_search_seq;
        let Some(Modal::YouTube(d)) = &mut self.modal else { return };
        let query = d.input.text.trim().to_string();
        if query.is_empty() {
            d.message = Some("Type something to search for".into());
            return;
        }
        d.searching = true;
        d.search_id = id;
        d.message = None;
        let (jobs, limit) = (self.jobs.clone(), crate::config::get().yt_search_results);
        if crate::youtube::looks_like_url(&query) {
            self.rt.spawn(async move {
                let result = crate::youtube::resolve(&query).await;
                let _ = jobs.send(Job::YtResolved(id, result));
            });
            return;
        }
        self.rt.spawn(async move {
            let result = crate::youtube::search(&query, limit).await;
            let _ = jobs.send(Job::YtResults(id, result));
        });
    }

    pub fn youtube_download(&mut self) {
        let Some(Modal::YouTube(d)) = &mut self.modal else { return };
        let Some(r) = d.results.get(d.cursor) else { return };
        if d.downloading.contains(&r.url) || d.saved.contains(&r.url) {
            return;
        }
        let (url, title) = (r.url.clone(), r.title.clone());
        d.downloading.insert(url.clone());
        let jobs = self.jobs.clone();
        self.rt.spawn(async move {
            let result = crate::youtube::download(&url).await;
            let _ = jobs.send(Job::YtDownloaded(url, result));
        });
        self.info(format!("Downloading “{title}”…"));
    }

    // ── Playlists ────────────────────────────────────────────────────────────

    pub fn prompt(&mut self, title: &str, hint: &str, text: &str, purpose: PlaylistPrompt) {
        self.modal = Some(Modal::Prompt(Prompt { title: title.into(), hint: hint.into(), input: Input::new(text), purpose }));
    }

    pub fn submit_prompt(&mut self) {
        let Some(Modal::Prompt(p)) = self.modal.take() else { return };
        let text = p.input.text.trim().to_string();
        match p.purpose {
            PlaylistPrompt::New { then_add } => {
                if text.is_empty() {
                    return;
                }
                if self.playlists().contains(&text) {
                    self.error(format!("There's already a playlist called “{text}”"));
                    return;
                }
                crate::persist::create_playlist(text.clone());
                if let Some(path) = then_add {
                    crate::persist::add_to_playlist(text.clone(), path);
                    self.info(format!("Added to “{text}”"));
                } else {
                    self.info(format!("Created “{text}”"));
                }
                self.rebuild_nav();
                self.refresh_items();
            }
            PlaylistPrompt::Rename(old) => {
                if text.is_empty() || text == old {
                    return;
                }
                if self.playlists().contains(&text) {
                    self.error(format!("There's already a playlist called “{text}”"));
                    return;
                }
                crate::persist::rename_playlist(old, text.clone());
                if matches!(&self.source, Source::Nav(Nav::Playlist(_))) {
                    self.source = Source::Nav(Nav::Playlist(text.clone()));
                }
                self.rebuild_nav();
                if let Some(i) = self.nav.iter().position(|n| *n == Nav::Playlist(text.clone())) {
                    self.nav_cursor = i;
                }
                self.refresh_items();
                self.info(format!("Renamed to “{text}”"));
            }
            PlaylistPrompt::Export(name) => self.export_playlist(&name, &text),
            PlaylistPrompt::Import => self.import_playlist(&text),
        }
    }

    pub fn add_to_playlist(&mut self, name: &str, path: PathBuf) {
        crate::persist::add_to_playlist(name.to_string(), path);
        self.info(format!("Added to “{name}”"));
        if self.current_playlist().as_deref() == Some(name) {
            self.refresh_items();
        }
    }

    pub fn remove_from_playlist(&mut self) {
        let (Some(name), Some(idx)) = (self.current_playlist(), self.selected_track()) else { return };
        let path = self.tracks[idx].path.clone();
        crate::persist::write(|db| {
            if let Some(list) = db.playlists.get_mut(&name) {
                list.retain(|p| p != &path);
            }
        });
        let title = self.tracks[idx].title.clone();
        self.refresh_items();
        self.info(format!("Removed “{title}” from “{name}”"));
    }

    pub fn delete_playlist(&mut self, name: &str) {
        crate::persist::delete_playlist(name.to_string());
        if self.current_playlist().as_deref() == Some(name) {
            self.source = Source::Nav(Nav::Songs);
        }
        self.rebuild_nav();
        self.refresh_items();
        self.info(format!("Deleted “{name}”"));
    }

    fn export_playlist(&mut self, name: &str, path: &str) {
        let path = crate::config::expand_tilde(path);
        let paths = crate::persist::get(|db| db.playlists.get(name).cloned().unwrap_or_default());
        let tracks: Vec<&Track> = self.by_paths(&paths).into_iter().map(|i| &self.tracks[i]).collect();
        let csv = crate::playlist_io::to_csv(tracks);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        match std::fs::write(&path, csv) {
            Ok(()) => self.info(format!("Saved “{name}” to {}", path.display())),
            Err(e) => self.error(format!("Couldn't save {}: {e}", path.display())),
        }
    }

    fn import_paths(&self, rows: &[crate::playlist_io::Row]) -> Vec<PathBuf> {
        rows.iter()
            .filter_map(|r| {
                self.tracks.iter().find(|t| {
                    if r.id.is_empty() {
                        t.title.eq_ignore_ascii_case(&r.title) && t.artist.eq_ignore_ascii_case(&r.artist)
                    } else {
                        t.yt_id.as_deref() == Some(r.id.as_str())
                    }
                })
            })
            .map(|t| t.path.clone())
            .collect()
    }

    fn import_playlist(&mut self, path: &str) {
        if self.import_job.is_some() {
            self.error("An import is already running");
            return;
        }
        let path = crate::config::expand_tilde(path);
        let rows = match std::fs::read_to_string(&path) {
            Ok(text) => crate::playlist_io::parse_csv(&text),
            Err(e) => {
                self.error(format!("Couldn't read {}: {e}", path.display()));
                return;
            }
        };
        if rows.is_empty() {
            self.error("No songs in that file");
            return;
        }
        // Named after the file, never replacing an existing playlist.
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Imported".into());
        self.begin_import(stem, rows, |r| format!("https://www.youtube.com/watch?v={}", r.id));
    }

    /// Makes a playlist named after `stem` (never replacing one) from `rows`:
    /// songs already in the library are added now, the rest are downloaded
    /// one by one from `link(row)` and added when all are done.
    fn begin_import(&mut self, stem: String, rows: Vec<crate::playlist_io::Row>, link: impl Fn(&crate::playlist_io::Row) -> String) {
        if self.import_job.is_some() {
            self.error("An import is already running");
            return;
        }
        let existing: HashSet<String> = self.playlists().into_iter().collect();
        let name = (1..)
            .map(|n| if n == 1 { stem.clone() } else { format!("{stem} ({n})") })
            .find(|n| !existing.contains(n))
            .unwrap_or(stem);
        let have: HashSet<&str> = self.tracks.iter().filter_map(|t| t.yt_id.as_deref()).collect();
        let mut queued = HashSet::new();
        let pending: VecDeque<String> = rows
            .iter()
            .filter(|r| !r.id.is_empty() && !have.contains(r.id.as_str()) && queued.insert(r.id.clone()))
            .map(&link)
            .collect();
        let paths = self.import_paths(&rows);
        let found = paths.len();
        crate::persist::write(|db| {
            db.playlists.insert(name.clone(), paths);
        });
        self.rebuild_nav();
        if pending.is_empty() {
            self.info(format!("Imported “{name}” ({found} songs)"));
            return;
        }
        if crate::youtube::preflight(crate::config::get()).is_some() {
            self.error(format!("Imported “{name}” ({found} songs); install yt-dlp to download the {} missing", pending.len()));
            return;
        }
        let total = pending.len();
        self.import_job = Some(ImportJob { name, rows, pending, total, done: 0, failed: 0 });
        self.import_next();
    }

    fn import_next(&mut self) {
        let Some(job) = &mut self.import_job else { return };
        match job.pending.pop_front() {
            Some(link) => {
                let n = job.total - job.pending.len();
                let total = job.total;
                self.info(format!("Importing: downloading {n} of {total}…"));
                let jobs = self.jobs.clone();
                self.rt.spawn(async move {
                    let result = crate::youtube::download(&link).await;
                    let _ = jobs.send(Job::ImportDownloaded(result));
                });
            }
            None => self.rescan(Rescan::AfterImport),
        }
    }

    fn finish_import(&mut self) {
        let Some(job) = self.import_job.take() else { return };
        let paths = self.import_paths(&job.rows);
        let count = paths.len();
        crate::persist::write(|db| {
            db.playlists.insert(job.name.clone(), paths);
        });
        self.refresh_items();
        match job.failed {
            0 => self.info(format!("Imported “{}” ({count} songs)", job.name)),
            n => self.error(format!("Imported “{}” ({count} songs; {n} couldn't be downloaded)", job.name)),
        }
        let _ = job.done;
    }

    pub fn import_progress(&self) -> Option<String> {
        self.import_job.as_ref().map(|j| format!("Importing “{}”: {} of {}", j.name, j.done + j.failed, j.total))
    }

    // ── Tags ─────────────────────────────────────────────────────────────────

    pub fn open_tag_editor(&mut self) {
        let Some(idx) = self.selected_track() else {
            self.info("Select a song to edit its tags");
            return;
        };
        let t = &self.tracks[idx];
        let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
        let fields = vec![
            Input::new(&t.title),
            Input::new(&t.artist),
            Input::new(&t.album),
            Input::new(&t.genre),
            Input::new(num(t.year)),
            Input::new(num(t.track_number)),
            Input::new(num(t.disc_number)),
        ];
        self.modal = Some(Modal::Tags(TagEditor { path: t.path.clone(), fields, cursor: 0, error: None }));
    }

    pub fn save_tags(&mut self) {
        let Some(Modal::Tags(ed)) = &mut self.modal else { return };
        let f: Vec<String> = ed.fields.iter().map(|i| i.text.trim().to_string()).collect();
        let num = |s: &str| -> Result<Option<u32>, ()> { if s.is_empty() { Ok(None) } else { s.parse().map(Some).map_err(|_| ()) } };
        let (Ok(year), Ok(track), Ok(disc)) = (num(&f[4]), num(&f[5]), num(&f[6])) else {
            ed.error = Some("Year, track and disc must be numbers".into());
            return;
        };
        let path = ed.path.clone();
        match crate::library::scanner::write_tags(&path, &f[0], &f[1], &f[2], &f[3], track, disc, None, year) {
            Ok(()) => {
                if let Some(t) = self.tracks.iter_mut().find(|t| t.path == path) {
                    t.title = f[0].clone();
                    t.artist = f[1].clone();
                    t.album = f[2].clone();
                    t.genre = f[3].clone();
                    t.year = year;
                    t.track_number = track;
                    t.disc_number = disc;
                }
                self.modal = None;
                self.refresh_items();
                self.info("Tags saved");
            }
            Err(e) => ed.error = Some(format!("Couldn't save: {e}")),
        }
    }

    // ── Settings ─────────────────────────────────────────────────────────────

    pub const SETTINGS: [&'static str; 4] = ["Theme", "Visualiser", "Online lyrics", "Volume"];

    pub fn settings_value(&self, row: usize) -> String {
        match row {
            0 => {
                if self.theme_mode == "manual" {
                    self.theme_preset.clone()
                } else {
                    "follow Omarchy".into()
                }
            }
            1 => if self.spectrum_mirrored { "mirrored" } else { "bottom" }.into(),
            2 => if self.lyrics_online { "on (lrclib.net)" } else { "off (local files only)" }.into(),
            _ => format!("{}%", (self.volume * 100.0).round()),
        }
    }

    /// Changes a setting by one step (`dir` is ±1) and saves it.
    pub fn change_setting(&mut self, row: usize, dir: i32) {
        match row {
            0 => {
                let presets = crate::theme::get_available_presets();
                let mut choices = vec!["auto".to_string()];
                choices.extend(presets.iter().map(|p| p.to_string()));
                let now = if self.theme_mode == "manual" { self.theme_preset.clone() } else { "auto".into() };
                let i = choices.iter().position(|c| *c == now).unwrap_or(0) as i32;
                let next = &choices[(i + dir).rem_euclid(choices.len() as i32) as usize];
                if next == "auto" {
                    self.theme_mode = "auto".into();
                } else {
                    self.theme_mode = "manual".into();
                    self.theme_preset = next.clone();
                }
                crate::theme::reload_with_mode(&self.theme_mode, &self.theme_preset);
            }
            1 => self.spectrum_mirrored = !self.spectrum_mirrored,
            2 => self.lyrics_online = !self.lyrics_online,
            _ => {
                self.volume_by(0.05 * dir as f32);
                return;
            }
        }
        self.save_config();
    }

    fn save_config(&mut self) {
        // Re-read the file so changes made by the GUI meanwhile are kept.
        let path = crate::config::config_path();
        let mut cfg: crate::config::Config = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_else(|| crate::config::get().clone());
        cfg.theme_mode = self.theme_mode.clone();
        cfg.theme_preset = self.theme_preset.clone();
        cfg.spectrum_style = if self.spectrum_mirrored { "mirrored" } else { "bottom" }.into();
        cfg.lyrics_online = self.lyrics_online;
        if let Err(e) = crate::persist::save_config(&cfg) {
            self.error(format!("Couldn't save settings: {e}"));
        } else if !self.lyrics_online || cfg.lyrics_online != crate::config::get().lyrics_online {
            self.info("Saved. Online lyrics take effect next time you start the app");
        }
    }

    // ── Phone ────────────────────────────────────────────────────────────────

    /// Starts the phone player's server if it isn't running.
    pub fn ensure_phone_server(&mut self) -> bool {
        if self.phone.is_some() {
            return true;
        }
        match crate::phone::Server::start(&self.rt, crate::config::get().phone_port, &self.tracks) {
            Ok(server) => {
                let (jobs, port) = (self.jobs.clone(), server.port);
                self.rt.spawn(async move {
                    let addresses = crate::phone::addresses(port).await;
                    let _ = jobs.send(Job::PhoneAddresses(addresses));
                });
                self.phone_addresses = vec![format!("http://localhost:{port}")];
                self.phone = Some(server);
                true
            }
            Err(e) => {
                self.error(e);
                false
            }
        }
    }

    /// Shows how to connect the phone, with a fresh pairing code.
    pub fn open_phone_dialog(&mut self) {
        if !self.ensure_phone_server() {
            return;
        }
        self.phone_code = self.phone.as_ref().map(|s| s.new_pairing_code());
        self.modal = Some(Modal::Phone);
    }

    /// Moves playback between the laptop and the phone, at the same spot.
    pub fn toggle_output(&mut self) {
        match self.output {
            Output::Laptop => {
                if !self.ensure_phone_server() {
                    return;
                }
                self.player.send(AudioCommand::Stop);
                self.output = Output::Phone;
                let play = self.state == PlaybackState::Playing;
                if self.current.is_some() {
                    if let Some(phone) = &self.phone {
                        phone.command(crate::phone::Cmd::Load { at: self.position.as_secs_f64(), play });
                    }
                }
                self.spectrum.iter_mut().for_each(|b| *b = 0.0);
                self.open_phone_dialog();
            }
            Output::Phone => {
                self.output = Output::Laptop;
                if let Some(phone) = &self.phone {
                    phone.command(crate::phone::Cmd::Release);
                }
                if let Some(track) = self.current_track().cloned() {
                    let was_playing = self.state == PlaybackState::Playing;
                    self.player.send(AudioCommand::Play(track.path));
                    self.player.send(AudioCommand::Seek(self.position));
                    if !was_playing {
                        self.player.send(AudioCommand::Pause);
                    }
                }
                self.info("Playing on this computer");
            }
        }
        self.phone_published = None;
    }

    /// Once per frame: handle the phone page's messages, publish changes.
    pub fn sync_phone(&mut self) {
        let mut events = Vec::new();
        if let Some(phone) = &mut self.phone {
            while let Some(ev) = phone.try_event() {
                events.push(ev);
            }
        }
        for ev in events {
            self.on_phone(ev);
        }
        let Some(phone) = &self.phone else { return };

        let id_of = |i: Option<usize>| i.and_then(|i| self.tracks.get(i)).map(|t| t.id);
        let now = if self.output == Output::Phone { id_of(self.current) } else { None };
        let next = if self.output == Output::Phone { id_of(self.up_next()) } else { None };
        let skip = if self.output == Output::Phone { id_of(self.skip_target()) } else { None };
        let playing = self.state == PlaybackState::Playing;
        phone.set_position(self.position.as_secs_f64());
        let key = (now, next, skip, playing);
        if self.phone_published != Some(key) {
            let track = if self.output == Output::Phone { self.current_track() } else { None };
            phone.set_now(track, next, skip, playing, self.output == Output::Phone);
            self.phone_published = Some(key);
        }

        // Lyrics for the page, with this song's timing fix applied.
        let lyrics_key = (now, self.lyrics_rev, (self.lyrics_offset * 10.0) as i64);
        if lyrics_key != self.phone_lyrics_published {
            let lines = self.current_track().filter(|_| now.is_some()).and_then(|t| match self.shown_lyrics(t).0 {
                Some(Lyrics::Synced(l)) => Some((l.clone(), true)),
                Some(Lyrics::Plain(l)) => Some((l.clone(), false)),
                _ => None,
            });
            phone.set_lyrics(now, lines, self.lyrics_offset as f64);
            self.phone_lyrics_published = lyrics_key;
        }
    }

    fn on_phone(&mut self, ev: crate::phone::Event) {
        use crate::phone::Event;
        self.phone_seen = Some(Instant::now());
        if self.output != Output::Phone {
            return;
        }
        let current_id = self.current_track().map(|t| t.id);
        match ev {
            Event::Report { id, at, playing } => {
                if Some(id) == current_id {
                    self.position = Duration::from_secs_f64(at.max(0.0));
                    let state = if playing { PlaybackState::Playing } else { PlaybackState::Paused };
                    if state != self.state {
                        self.state = state;
                        self.send_mpris(MprisUpdate::Status(if playing { PlaybackStatus::Playing } else { PlaybackStatus::Paused }));
                    }
                }
            }
            Event::Ended { id, next } => {
                if Some(id) != current_id {
                    return;
                }
                // The page has already started the song it was told comes next.
                match next.and_then(|n| self.tracks.iter().position(|t| t.id == n)) {
                    Some(i) => self.start_bookkeeping(i),
                    None => self.on_track_ended(),
                }
            }
            Event::Toggle => self.toggle_play(),
            Event::Play => self.resume(),
            Event::Pause => self.pause(),
            Event::Next => self.next(),
            Event::Previous => self.previous(),
            Event::Seek(at) => self.position = Duration::from_secs_f64(at.max(0.0)),
        }
    }

    /// Everything `start` does except telling a player to play.
    fn start_bookkeeping(&mut self, idx: usize) {
        let Some(track) = self.tracks.get(idx).cloned() else { return };
        self.current = Some(idx);
        if let Some(p) = self.queue.iter().position(|&i| i == idx) {
            self.queue_pos = p;
        }
        self.position = Duration::ZERO;
        self.duration = track.duration;
        self.state = PlaybackState::Playing;
        self.shuffle_next = self.pick_shuffle_next();
        crate::persist::increment_play_count(track.path.clone());
        crate::persist::add_to_recently_played(track.path.clone());
        self.lyrics_offset = crate::persist::lyrics_offset(&track.path);
        self.lyric_cursor = None;
        self.lyrics_rev += 1;
        self.ensure_lyrics();
        self.load_cover(&track.path);
        self.publish_track_mpris(&track, None);
        notify(&track.title, &display_artist(&track));
        // These lists change order as songs are played.
        if matches!(self.source, Source::Nav(Nav::Recent | Nav::MostPlayed)) {
            let keep = self.cursor;
            self.refresh_items();
            self.cursor = keep.min(self.items.len().saturating_sub(1));
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// How long the lyrics stay where the listener scrolled them.
pub const LYRIC_PAUSE: Duration = Duration::from_secs(10);

pub fn furigana_key(lines: &[LyricLine]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for l in lines {
        l.text.hash(&mut h);
    }
    h.finish()
}

pub fn fmt_time(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn display_artist(track: &Track) -> String {
    if track.artist.trim().is_empty() {
        "Unknown Artist".to_string()
    } else {
        track.artist.clone()
    }
}

fn cover_file() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("HOME").ok()?).join(".cache/lavanda/cover.jpg"))
}

fn cover_file_url() -> Option<String> {
    let path = cover_file()?;
    path.exists().then(|| format!("file://{}", path.display()))
}

/// The same file the GUI writes, for MPRIS album art.
fn publish_cover_file(bytes: &[u8]) {
    if let Some(path) = cover_file() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        std::fs::write(path, bytes).ok();
    }
}

/// Desktop notification for a new song. Waited on in a thread so the
/// process doesn't linger as a zombie.
fn notify(title: &str, artist: &str) {
    let child = std::process::Command::new("notify-send")
        .args([crate::audio::mpris::APP_NAME, &format!("{title} — {artist}"), "--icon=audio-x-generic", "--expire-time=3000", "--urgency=low"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Ok(mut child) = child {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}
