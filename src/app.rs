use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::futures::{SinkExt, Stream};
use iced::widget::{column, container, row, text, Space, stack};
use iced::{Alignment, Element, Length, Subscription, Task, Theme};
use mpris_server::{LoopStatus, PlaybackStatus};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::audio::mpris;
use crate::audio::{
    AudioCommand, AudioEvent, AudioPlayer, MprisCommand, MprisUpdate, PlaybackState,
};
use crate::library::models::Track;
use crate::library::{load_cover, scan_folder};
use crate::ui::{theme, views};

type Shared<T> = Arc<Mutex<Option<UnboundedReceiver<T>>>>;

pub use crate::audio::mpris::APP_NAME;
/// The app icon: Tux by Larry Ewing (made with The GIMP).
const TUX_PNG: &[u8] = include_bytes!("../assets/tux.png");

/// Size limits for the album cover in the player panel.
const COVER_MIN: f32 = 140.0;
const COVER_MAX: f32 = 400.0;

/// Width limits for the lyrics side panel.
const LYRICS_MIN_W: f32 = 240.0;
const LYRICS_MAX_W: f32 = 720.0;

// ── Enums de navegação ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Layout {
    Standard,
    Focus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Artists,
    Albums,
    Genres,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    TrackNumber,
    Title,
    Artist,
    Album,
    Genre,
    Year,
    DiscNumber,
    Duration,
    Plays,
    DatePlayed,
}

#[derive(Debug, Clone)]
pub enum ContextMenuTarget {
    Artist(String),
    Album(String),
    Track(Track),
    MultipleTracks(Vec<Track>),
    Header(crate::persist::TableColumn),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistTab {
    Playlists,
    Autoplaylists,
}

// ── Playlist dialog ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum PlaylistDialogMode {
    Create,
    AddTrack(Track),
    Rename(String),
}

#[derive(Debug, Clone)]
pub struct PlaylistDialogState {
    pub mode: PlaylistDialogMode,
    pub name_input: String,
    pub selected_playlist: Option<String>,
    pub add_album: bool,
}

// ── YouTube search dialog ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct YtSearchState {
    pub query: String,
    pub results: Vec<crate::youtube::YtResult>,
    /// Index of the highlighted result.
    pub selected: Option<usize>,
    pub searching: bool,
    /// Set when a search or download fails.
    pub error: Option<String>,
    /// Transient progress/success line.
    pub status: Option<String>,
    /// Index of the result currently downloading.
    pub downloading: Option<usize>,
    /// yt-dlp missing/unusable — shown before any search is attempted.
    pub preflight_error: Option<String>,
    /// Result thumbnails by video id, as they arrive.
    pub thumbs: HashMap<String, iced::widget::image::Handle>,
    /// Results downloaded since the dialog opened.
    pub saved: HashSet<usize>,
    /// Large cover of the selected result.
    pub preview: Option<(String, iced::widget::image::Handle)>,
    /// Set when the results are a pasted playlist or album: its name.
    pub list_title: Option<String>,
    /// yt-dlp is being checked when the dialog opens.
    pub checking: bool,
    /// The text the shown results are for: Enter on the same text downloads
    /// the selected result instead of searching again.
    pub searched: String,
}

// ── Playlist import ───────────────────────────────────────────────────────────

pub struct ImportJob {
    pub name: String,
    rows: Vec<crate::playlist_io::Row>,
    /// Links of the songs still to download.
    pending: std::collections::VecDeque<String>,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
}

// ── Tag editor ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TagEditorState {
    pub tracks: Vec<Track>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub track_number: String,
    pub disc_number: String,
    pub cover_path: Option<String>,
    pub apply_to_album: bool,
    pub year: String,
    pub apply_title: bool,
    pub apply_artist: bool,
    pub apply_album: bool,
    pub apply_year: bool,
    pub apply_genre: bool,
    pub apply_track_num: bool,
    pub apply_disc_num: bool,
    pub apply_cover: bool,
}

// ── Mensagens ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    // Biblioteca
    LibraryScanned(Vec<Track>),
    RescanLibrary,

    // Reprodução
    PlayPause,
    NextTrack,
    PreviousTrack,
    Seek(Duration),
    SeekRelative(i64),
    VolumeChanged(f32),
    VolumeStep(f32),
    ToggleShuffle,
    ToggleRepeat,

    // Sidebar drag
    SidebarDragStart,
    SidebarDragMove(f32),
    SidebarDragEnd,

    // Playlist drag (resize vertical)
    PlaylistDragStart,
    PlaylistDragMove(f32),
    PlaylistDragEnd,

    // Eventos internos
    Audio(AudioEvent),
    Mpris(MprisCommand),
    SpectrumData(Vec<f32>),
    TracklistScrolled(f32, f32),
    CheckTheme,
    WindowResized(f32, f32),

    // Navegação ViewMode
    SelectViewMode(ViewMode),
    SelectArtist(String),
    SelectAlbum(String),
    SelectGenre(String),
    SelectPlaylist(String),
    SelectPlaylistTab(PlaylistTab),

    // Double-click
    DoubleClickTrack(Track),
    DoubleClickArtist(String),
    DoubleClickAlbum(String),
    DoubleClickGenre(String),
    DoubleClickPlaylist(String),

    // Player navigation
    FocusSongName,
    FocusArtistName,
    FocusAlbumName,

    // Track selection
    SelectTrack(Track),
    ModifiersChanged(iced::keyboard::Modifiers),

    // Busca
    SearchChanged(String),
    ToggleFilterTitle,
    ToggleFilterArtist,
    ToggleFilterAlbum,
    ToggleFilterGenre,

    // Sidebar search
    SidebarSearchChanged(String),

    // Sort
    SortBy(SortColumn),

    // Like
    ToggleLikeTrack(Track),

    // Playlists
    OpenPlaylistDialog(PlaylistDialogMode),
    ClosePlaylistDialog,
    PlaylistInputChanged(String),
    PlaylistDialogSelect(String),
    PlaylistDialogToggleAddAlbum(bool),
    PlaylistDialogSubmit,
    DeletePlaylist(String),
    AddTracksToPlaylist(String, Vec<Track>),
    CreatePlaylistFromContext(String, bool),
    CreatePlaylistWithTracks(String, Vec<Track>),

    // YouTube search (yt-dlp)
    OpenYtSearch,
    CloseYtSearch,
    YtQueryChanged(String),
    YtSubmit,
    YtChecked(Option<String>),
    YtResolved(Result<crate::youtube::Resolved, String>),
    YtPreview(String, Option<iced::widget::image::Handle>),
    YtDownloadAll,
    YtSearched(Result<Vec<crate::youtube::YtResult>, String>),
    YtSelect(usize),
    YtDownload(usize),
    YtDownloaded(Result<Option<std::path::PathBuf>, String>),
    YtThumb(String, Option<iced::widget::image::Handle>),

    // Tag editor
    OpenTagEditor(Vec<Track>),
    CloseTagEditor,
    UpdateTagFieldTitle(String),
    UpdateTagFieldArtist(String),
    UpdateTagFieldAlbum(String),
    UpdateTagFieldGenre(String),
    UpdateTagFieldTrackNumber(String),
    UpdateTagFieldDiscNumber(String),
    UpdateTagFieldYear(String),
    UpdateTagFieldCoverPath(String),
    UpdateTagFieldApplyToAlbum(bool),
    ToggleTagFieldApplyTitle(bool),
    ToggleTagFieldApplyArtist(bool),
    ToggleTagFieldApplyAlbum(bool),
    ToggleTagFieldApplyYear(bool),
    ToggleTagFieldApplyGenre(bool),
    ToggleTagFieldApplyTrackNum(bool),
    ToggleTagFieldApplyDiscNum(bool),
    ToggleTagFieldApplyCover(bool),
    SaveTags,
    SearchCoverOnline,

    // Contexto
    ToggleContextMenu(Option<ContextMenuTarget>),
    HideAlbumOrArtist(String, bool),
    RestoreHiddenItems,

    // Hover / UI
    HoverTracklist(bool),
    HoverSidebarList(bool),

    // Group by album
    ToggleGroupByAlbum,

    // Columns
    ToggleColumnVisibility(crate::persist::TableColumn),
    MoveColumnLeft(crate::persist::TableColumn),
    MoveColumnRight(crate::persist::TableColumn),

    // Teclado (atalhos especiais do omatunes)
    KeyPressed(iced::keyboard::Key),
    KeyboardLike,
    KeyboardEdit,
    KeyboardAdd,
    KeyboardArrowUp,
    KeyboardArrowDown,

    // Help / shortcuts
    OpenShortcuts,
    CloseShortcuts,

    // Layout / espectro (lavanda original)
    ToggleSpectrum,
    ToggleLayout,

    // Playlist files (CSV of YouTube ids)
    ExportPlaylist(String),
    /// CSV text, and where the user chose to save it.
    ExportPlaylistTo(String, Option<PathBuf>),
    ImportPlaylist,
    ImportPlaylistFrom(Option<PathBuf>),
    ImportNext,
    ImportDownloaded(Result<Option<PathBuf>, String>),
    ImportScanned(Vec<Track>),

    /// A click on the system tray icon or its menu.
    Tray(crate::tray::TrayCommand),

    // Lyrics
    ToggleLyrics,
    LyricsLoaded(PathBuf, crate::lyrics::Lyrics),
    LyricsDragStart,
    LyricsDragMove(f32),
    LyricsDragEnd,
    /// The line with this timestamp is being sung right now.
    LyricsSyncLine(Duration),
    LyricsNudge(f32),
    LyricsOffsetReset,
    ToggleLyricsRomaji(bool),
    /// Furigana for the lyrics with this content key (None: none to show).
    FuriganaLoaded(u64, Option<Vec<Vec<crate::lyrics::Ruby>>>),
    /// The playing track's lyrics in the other script (romaji or Japanese).
    LyricsAltLoaded(PathBuf, crate::lyrics::Lyrics),

    // Open local folder
    OpenLocalFolder(PathBuf),
    // Settings
    OpenSettings,
    CloseSettings,
    SettingsThemeModeChanged(String),
    SettingsThemePresetChanged(String),
    SettingsCornerRadiusChanged(f32),
    SettingsGlassmorphismToggled(bool),
    SettingsBackgroundKindChanged(String),
    SettingsBackgroundColorChanged(String),
    SettingsBackgroundGradientFromChanged(String),
    SettingsBackgroundGradientToChanged(String),
    SettingsBackgroundGradientAngleChanged(f32),
    SettingsBackgroundImageChanged(String),
    SettingsSelectBackgroundImage,
    SettingsBackgroundImageSelected(Option<std::path::PathBuf>),
    SettingsSpectrumStyleChanged(String),
    SettingsCoverSizeChanged(f32),
    SettingsApply,
    /// Blurred copy of a track's cover, made off the UI thread.
    /// Blurred copy and rounded-corner copy of a track's cover.
    CoverBlurred(PathBuf, Option<(iced::widget::image::Handle, CoverTone)>, Option<iced::widget::image::Handle>),
}

// ── Estado global ─────────────────────────────────────────────────────────────

pub struct AppState {
    // Áudio
    pub playback_state: PlaybackState,
    pub current_track: Option<Track>,
    pub queue: Vec<Track>,
    pub position: Duration,
    pub duration: Duration,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: bool,

    // Biblioteca
    pub all_tracks: Vec<Track>,
    pub tracks: Vec<Track>,

    // ViewMode
    pub view_mode: ViewMode,
    pub selected_artist: Option<String>,
    pub selected_album: Option<String>,
    pub selected_genre: Option<String>,
    pub selected_playlist: Option<String>,
    pub playlist_tab: PlaylistTab,

    // Busca
    pub search_query: String,
    pub filter_title: bool,
    pub filter_artist: bool,
    pub filter_album: bool,
    pub filter_genre: bool,
    pub sidebar_search: String,

    // Seleção
    pub selected_track: Option<Track>,
    pub selected_tracks: Vec<Track>,
    pub last_clicked_track: Option<Track>,
    pub modifiers: iced::keyboard::Modifiers,

    // Double-click detection
    pub last_click_track: Option<(i64, Instant)>,
    pub last_click_artist: Option<(String, Instant)>,
    pub last_click_album: Option<(String, Instant)>,
    pub last_click_genre: Option<(String, Instant)>,
    pub last_click_playlist: Option<(String, Instant)>,

    // Sort
    pub sort_column: Option<SortColumn>,
    pub sort_ascending: bool,

    // Group
    pub group_by_album: bool,

    // Tag editor
    pub show_tag_editor: Option<TagEditorState>,

    // Playlist dialog
    pub playlist_dialog: Option<PlaylistDialogState>,

    // YouTube search dialog
    pub yt_dialog: Option<YtSearchState>,

    // Context menu
    pub show_context_menu: Option<ContextMenuTarget>,

    // Hidden items
    pub hidden_artists_albums: Vec<(String, bool)>,

    // Playlist panel height
    pub playlist_height: f32,
    pub playlist_height_initialized: bool,
    pub dragging_playlist_split: bool,
    pub window_height: f32,
    /// Scroll position and visible height of the track list, so that only
    /// the rows on screen are built.
    pub tracklist_offset: f32,
    pub tracklist_view_h: f32,

    // Sidebar
    pub sidebar_width: f32,
    dragging_sidebar: bool,
    pub is_hovering_tracklist: bool,
    pub is_hovering_sidebar_list: bool,

    // Shortcuts modal
    pub show_shortcuts: bool,

    // Layout / espectro (lavanda)
    pub layout: Layout,
    pub spectrum: Vec<f32>,
    pub show_spectrum: bool,

    /// A playlist import downloading the songs it is missing.
    import_job: Option<ImportJob>,
    /// A short message in the header (export/import results), and until when.
    status: Option<(String, Instant)>,

    // Lyrics
    pub show_lyrics: bool,
    /// Lyrics looked up this session, by track path.
    lyrics: HashMap<PathBuf, crate::lyrics::Lyrics>,
    lyrics_loading: HashSet<PathBuf>,
    /// Show Japanese lyrics in romaji (the switch in the lyrics panel).
    pub lyrics_romaji: bool,
    /// Other-script versions: romaji made from Japanese lyrics, or Japanese
    /// lyrics found for songs whose lyrics came back in romaji.
    lyrics_alt: HashMap<PathBuf, crate::lyrics::Lyrics>,
    lyrics_alt_loading: HashSet<PathBuf>,
    /// Furigana per set of Japanese lyrics, keyed by their content.
    furigana: HashMap<u64, Option<Vec<Vec<crate::lyrics::Ruby>>>>,
    furigana_loading: HashSet<u64>,
    /// Seconds the playing track's synced lyrics are delayed by.
    pub lyrics_offset: f32,
    pub lyrics_width: f32,
    dragging_lyrics: bool,
    window_width: f32,

    // Tema
    pub iced_theme: Theme,
    loaded_theme_name: String,
    pub theme_mode: String,
    pub theme_preset: String,
    pub use_glassmorphism: bool,
    pub corner_radius: f32,
    pub background_kind: String,
    pub background_color: String,
    pub background_gradient_from: String,
    pub background_gradient_to: String,
    pub background_gradient_angle: f32,
    pub background_image: Option<String>,
    pub spectrum_style: String,
    pub cover_size: f32,
    pub show_settings: bool,
    /// The playing track's cover, decoded once per track. (`Handle::from_bytes`
    /// gets a new id on every call, so building it in `view` re-decoded the
    /// image on every frame.)
    pub cover: Option<iced::widget::image::Handle>,
    /// Small blurred copy of the cover for the "album" background.
    blurred_cover: Option<iced::widget::image::Handle>,
    /// Brightness and main colour of the cover, for adjusting the colours.
    pub cover_tone: Option<CoverTone>,
    /// The cover cropped square with rounded corners (iced can't round an
    /// image itself), matching the corner radius setting.
    pub cover_rounded: Option<iced::widget::image::Handle>,
    pub logo: iced::widget::image::Handle,

    // i18n
    pub strings: &'static crate::locale::Strings,

    // Canais internos
    audio: AudioPlayer,
    audio_events: Shared<AudioEvent>,
    spectrum_rx: Shared<Vec<f32>>,
    mpris_cmds: Shared<MprisCommand>,
    mpris_update_tx: tokio::sync::mpsc::UnboundedSender<MprisUpdate>,
    tray_cmds: Shared<crate::tray::TrayCommand>,
    /// "Title — Artist" for the tray icon's tooltip.
    tray_song_tx: tokio::sync::mpsc::UnboundedSender<String>,
}

impl AppState {
    fn new() -> (Self, Task<Message>) {
        let mut audio = AudioPlayer::spawn();
        let audio_events = Arc::new(Mutex::new(Some(audio.take_events())));

        let (spectrum_tx, spectrum_rx) = tokio::sync::mpsc::unbounded_channel();
        crate::audio::spectrum::launch(audio.viz_buf.clone(), spectrum_tx);
        let spectrum_rx = Arc::new(Mutex::new(Some(spectrum_rx)));

        let cfg = crate::config::get();
        let saved = crate::state::load();

        let (mpris_cmd_tx, mpris_cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (mpris_update_tx, mpris_update_rx) = tokio::sync::mpsc::unbounded_channel();
        mpris::launch(mpris_cmd_tx, mpris_update_rx);
        let mpris_cmds = Arc::new(Mutex::new(Some(mpris_cmd_rx)));

        let (tray_cmd_tx, tray_cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (tray_song_tx, tray_song_rx) = tokio::sync::mpsc::unbounded_channel();
        crate::tray::launch(tray_cmd_tx, tray_song_rx);
        let tray_cmds = Arc::new(Mutex::new(Some(tray_cmd_rx)));

        let loaded_theme_name = crate::ui::theme::read_current_theme_name();
        let iced_theme = build_iced_theme();
        let volume = saved.volume.unwrap_or(cfg.volume).clamp(0.0, 1.0);

        let hidden = crate::persist::get(|db| db.hidden_artists_albums.clone());

        let music_dir = cfg.music_path();
        let scan_task = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || scan_folder(&music_dir))
                    .await
                    .unwrap_or_default()
            },
            Message::LibraryScanned,
        );

        let state = AppState {
            playback_state: PlaybackState::Stopped,
            current_track: None,
            queue: Vec::new(),
            position: Duration::ZERO,
            duration: Duration::ZERO,
            volume,
            shuffle: cfg.shuffle,
            repeat: cfg.repeat,
            all_tracks: Vec::new(),
            tracks: Vec::new(),
            view_mode: ViewMode::Artists,
            selected_artist: None,
            selected_album: None,
            selected_genre: None,
            selected_playlist: None,
            playlist_tab: PlaylistTab::Playlists,
            search_query: String::new(),
            filter_title: true,
            filter_artist: true,
            filter_album: true,
            filter_genre: true,
            sidebar_search: String::new(),
            selected_track: None,
            selected_tracks: Vec::new(),
            last_clicked_track: None,
            modifiers: Default::default(),
            last_click_track: None,
            last_click_artist: None,
            last_click_album: None,
            last_click_genre: None,
            last_click_playlist: None,
            sort_column: None,
            sort_ascending: true,
            group_by_album: false,
            show_tag_editor: None,
            playlist_dialog: None,
            yt_dialog: None,
            show_context_menu: None,
            hidden_artists_albums: hidden,
            playlist_height: 141.0,
            playlist_height_initialized: false,
            dragging_playlist_split: false,
            window_height: 640.0,
            tracklist_offset: 0.0,
            tracklist_view_h: 0.0,
            sidebar_width: load_sidebar_width(),
            dragging_sidebar: false,
            is_hovering_tracklist: false,
            is_hovering_sidebar_list: false,
            show_shortcuts: false,
            layout: Layout::Standard,
            spectrum: vec![0.0; crate::audio::spectrum::NUM_BARS],
            show_spectrum: true,
            import_job: None,
            status: None,
            show_lyrics: saved.show_lyrics.unwrap_or(true),
            lyrics: HashMap::new(),
            lyrics_loading: HashSet::new(),
            lyrics_romaji: saved.lyrics_romaji.unwrap_or(false),
            lyrics_alt: HashMap::new(),
            lyrics_alt_loading: HashSet::new(),
            furigana: HashMap::new(),
            furigana_loading: HashSet::new(),
            lyrics_offset: 0.0,
            lyrics_width: saved.lyrics_width.unwrap_or(340.0).clamp(LYRICS_MIN_W, LYRICS_MAX_W),
            dragging_lyrics: false,
            window_width: 960.0,
            iced_theme,
            loaded_theme_name,
            theme_mode: cfg.theme_mode.clone(),
            theme_preset: cfg.theme_preset.clone(),
            use_glassmorphism: cfg.use_glassmorphism,
            corner_radius: cfg.corner_radius,
            background_kind: cfg.background_kind.clone(),
            background_color: cfg.background_color.clone(),
            background_gradient_from: cfg.background_gradient_from.clone(),
            background_gradient_to: cfg.background_gradient_to.clone(),
            background_gradient_angle: cfg.background_gradient_angle,
            background_image: {
                let b = cfg.background_image.trim();
                if b.is_empty() { None } else { Some(b.to_string()) }
            },
            spectrum_style: cfg.spectrum_style.clone(),
            cover_size: cfg.cover_size.clamp(COVER_MIN, COVER_MAX),
            show_settings: false,
            cover: None,
            blurred_cover: None,
            cover_tone: None,
            cover_rounded: None,
            logo: iced::widget::image::Handle::from_bytes(TUX_PNG),
            strings: crate::locale::get(),
            audio,
            audio_events,
            spectrum_rx,
            mpris_cmds,
            mpris_update_tx,
            tray_cmds,
            tray_song_tx,
        };

        (state, scan_task)
    }

    fn persist_state(&self) {
        crate::state::save(&crate::state::SavedState {
            volume: Some(self.volume),
            show_lyrics: Some(self.show_lyrics),
            lyrics_width: Some(self.lyrics_width),
            lyrics_romaji: Some(self.lyrics_romaji),
        });
    }

    /// Whether the romaji switch applies to this track: its lyrics are in
    /// Japanese or in romaji, or its tags are in Japanese.
    pub fn is_japanese_song(&self, track: &Track) -> bool {
        use crate::lyrics::{has_japanese, lines_have_japanese, lines_look_like_romaji, Lyrics};
        let lyrics_jp = match self.lyrics.get(&track.path) {
            Some(Lyrics::Synced(l) | Lyrics::Plain(l)) => lines_have_japanese(l) || lines_look_like_romaji(l),
            _ => false,
        };
        lyrics_jp || has_japanese(&format!("{} {}", track.title, track.artist))
    }

    /// `Some(true)` when the panel should show romaji made from these
    /// lyrics, `Some(false)` when it should show Japanese found for romaji
    /// lyrics, `None` when the looked-up lyrics are already right.
    fn wanted_alt(&self, track: &Track) -> Option<bool> {
        use crate::lyrics::{lines_have_japanese, Lyrics};
        let Some(Lyrics::Synced(lines) | Lyrics::Plain(lines)) = self.lyrics.get(&track.path) else {
            return None;
        };
        let japanese = lines_have_japanese(lines);
        if self.lyrics_romaji && japanese {
            Some(true)
        } else if !self.lyrics_romaji && !japanese && self.is_japanese_song(track) {
            Some(false)
        } else {
            None
        }
    }

    /// The lyrics the panel shows for `track`, and a note when the version
    /// the romaji switch asks for isn't available (yet).
    pub fn shown_lyrics(&self, track: &Track) -> (Option<&crate::lyrics::Lyrics>, Option<String>) {
        use crate::lyrics::Lyrics;
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

    /// Scrolls the track list so the row at `y` is a little below the top.
    /// The position is recorded too: only rows near it are built, and a
    /// programmatic scroll isn't reported back like a manual one.
    fn scroll_tracklist_to(&mut self, y: f32) -> Task<Message> {
        let y = (y - 120.0).max(0.0);
        self.tracklist_offset = y;
        iced::widget::scrollable::scroll_to(
            iced::widget::scrollable::Id::new("tracklist_scroll"),
            iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
        )
    }

    /// Loads the large cover of the selected YouTube result, if not shown yet.
    fn yt_preview_task(&self) -> Task<Message> {
        let Some(d) = self.yt_dialog.as_ref() else { return Task::none() };
        let Some(r) = d.selected.and_then(|i| d.results.get(i)) else { return Task::none() };
        if d.preview.as_ref().is_some_and(|(id, _)| *id == r.id) {
            return Task::none();
        }
        let id = r.id.clone();
        Task::perform(
            async move {
                let bytes = crate::youtube::thumbnail_large(id.clone()).await;
                let handle = tokio::task::spawn_blocking(move || bytes.as_deref().and_then(preview_image)).await.ok().flatten();
                (id, handle)
            },
            |(id, handle)| Message::YtPreview(id, handle),
        )
    }

    /// Makes a playlist named after `stem` (never replacing one) from `rows`:
    /// songs already in the library are added now, the rest are downloaded
    /// one by one from `link(row)` and added when all are done.
    fn begin_import(&mut self, stem: String, rows: Vec<crate::playlist_io::Row>, link: impl Fn(&crate::playlist_io::Row) -> String) -> Task<Message> {
        if self.import_job.is_some() {
            self.set_status("An import is already running".into());
            return Task::none();
        }
        let existing = crate::persist::get(|db| db.playlists.keys().cloned().collect::<HashSet<_>>());
        let name = (1..)
            .map(|n| if n == 1 { stem.clone() } else { format!("{stem} ({n})") })
            .find(|n| !existing.contains(n))
            .unwrap_or(stem);
        let have: HashSet<&str> = self.all_tracks.iter().filter_map(|t| t.yt_id.as_deref()).collect();
        let mut queued = HashSet::new();
        let pending: std::collections::VecDeque<String> = rows
            .iter()
            .filter(|r| !r.id.is_empty() && !have.contains(r.id.as_str()) && queued.insert(r.id.clone()))
            .map(&link)
            .collect();
        let paths = self.import_paths(&rows);
        let found = paths.len();
        crate::persist::write(|db| {
            db.playlists.insert(name.clone(), paths);
        });
        if pending.is_empty() {
            self.set_status(format!("Imported “{name}” ({found} songs)"));
            return Task::none();
        }
        let total = pending.len();
        self.import_job = Some(ImportJob { name, rows, pending, total, done: 0, failed: 0 });
        Task::done(Message::ImportNext)
    }

    /// A playlist file's songs, in file order: matched by YouTube id, or by
    /// title and artist for songs that didn't come from YouTube.
    fn import_paths(&self, rows: &[crate::playlist_io::Row]) -> Vec<PathBuf> {
        rows.iter()
            .filter_map(|r| {
                self.all_tracks.iter().find(|t| {
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

    fn set_status(&mut self, message: String) {
        self.status = Some((message, Instant::now() + Duration::from_secs(6)));
    }

    /// Header note: a running import's progress, or a recent message.
    pub fn status_text(&self) -> Option<String> {
        if let Some(job) = &self.import_job {
            return Some(format!("Importing “{}”: {}/{} downloaded…", job.name, job.done + job.failed, job.total));
        }
        self.status.as_ref().filter(|(_, until)| Instant::now() < *until).map(|(s, _)| s.clone())
    }

        /// Colour for the lyrics: tinted with the cover when the cover is bright
    /// (white lyrics would wash out), the theme's text colour otherwise.
    pub fn lyrics_color(&self) -> iced::Color {
        match self.cover_tone {
            Some(tone) if self.background_kind == "album" && tone.is_bright() => tone.lyrics_tint(),
            _ => theme::text(),
        }
    }

    /// Makes the blurred and the rounded copies of a cover off the UI thread.
    fn cover_task(&self, path: &std::path::Path, bytes: Option<Vec<u8>>) -> Task<Message> {
        let path = path.to_path_buf();
        // Corner radius as a fraction of the cover's width, so it looks the
        // same at any display size.
        let radius = self.corner_radius / self.cover_size;
        Task::perform(
            async move {
                let made = tokio::task::spawn_blocking(move || {
                    let bytes = bytes?;
                    Some((blurred_cover(&bytes), rounded_cover(&bytes, radius)))
                })
                .await
                .ok()
                .flatten();
                let (blurred, rounded) = made.unwrap_or((None, None));
                (path, blurred, rounded)
            },
            |(path, blurred, rounded)| Message::CoverBlurred(path, blurred, rounded),
        )
    }

    /// Redoes the rounded cover after the corner radius or cover size changed.
    fn refresh_cover(&self) -> Task<Message> {
        match &self.current_track {
            Some(t) => self.cover_task(&t.path, t.cover_data.clone()),
            None => Task::none(),
        }
    }

    /// Furigana for these lines, once worked out.
    pub fn furigana_for(&self, lines: &[crate::lyrics::LyricLine]) -> Option<&Vec<Vec<crate::lyrics::Ruby>>> {
        self.furigana.get(&furigana_key(lines))?.as_ref()
    }

    /// Starts working out furigana for the Japanese lyrics on screen.
    fn ensure_furigana(&mut self) -> Task<Message> {
        use crate::lyrics::Lyrics;
        let Some(track) = self.current_track.as_ref().filter(|_| self.show_lyrics) else {
            return Task::none();
        };
        let lines = match self.shown_lyrics(track).0 {
            Some(Lyrics::Synced(l) | Lyrics::Plain(l)) if crate::lyrics::wants_furigana(l) => l.clone(),
            _ => return Task::none(),
        };
        let key = furigana_key(&lines);
        if self.furigana.contains_key(&key) || !self.furigana_loading.insert(key) {
            return Task::none();
        }
        let texts: Vec<String> = lines.into_iter().map(|l| l.text).collect();
        Task::perform(
            async move { (key, crate::lyrics::furigana(texts).await.ok()) },
            |(key, ruby)| Message::FuriganaLoaded(key, ruby),
        )
    }

    /// Starts converting to romaji, or looking up the Japanese version, when
    /// the romaji switch needs a version we don't have yet.
    fn ensure_lyrics_alt(&mut self) -> Task<Message> {
        use crate::lyrics::Lyrics;
        let Some(track) = self.current_track.as_ref().filter(|_| self.show_lyrics) else {
            return Task::none();
        };
        let Some(romaji) = self.wanted_alt(track) else { return Task::none() };
        let done = matches!(self.lyrics_alt.get(&track.path), Some(l) if !matches!(l, Lyrics::Error(_)));
        if done || !self.lyrics_alt_loading.insert(track.path.clone()) {
            return Task::none();
        }
        let path = track.path.clone();
        let (lines, synced) = match self.lyrics.get(&path) {
            Some(Lyrics::Synced(l)) => (l.clone(), true),
            Some(Lyrics::Plain(l)) => (l.clone(), false),
            _ => return Task::none(),
        };
        let query = crate::lyrics::Query::from(track);
        // Both directions keep the primary lyrics' timing.
        Task::perform(
            async move {
                let alt = if romaji {
                    crate::lyrics::romaji(query, lines, synced).await
                } else {
                    crate::lyrics::japanese(query, lines, synced).await
                };
                (path, alt)
            },
            |(path, alt)| Message::LyricsAltLoaded(path, alt),
        )
    }

    /// Sets the playing track's lyrics delay; only meaningful for synced lyrics.
    fn set_lyrics_offset(&mut self, secs: f32) -> Task<Message> {
        let Some(track) = &self.current_track else { return Task::none() };
        if !matches!(self.lyrics.get(&track.path), Some(crate::lyrics::Lyrics::Synced(_))) {
            return Task::none();
        }
        // Tenths of a second: finer than anyone can hear, and tidy to show.
        self.lyrics_offset = (secs * 10.0).round() / 10.0;
        crate::persist::set_lyrics_offset(track.path.clone(), self.lyrics_offset);
        Task::none()
    }

    /// Starts a lyrics lookup for the playing track if the panel is shown and
    /// there is no result yet. Failed lookups are retried.
    fn ensure_lyrics(&mut self) -> Task<Message> {
        let Some(track) = self.current_track.as_ref().filter(|_| self.show_lyrics) else {
            return Task::none();
        };
        let done = matches!(self.lyrics.get(&track.path), Some(l) if !matches!(l, crate::lyrics::Lyrics::Error(_)));
        if done || !self.lyrics_loading.insert(track.path.clone()) {
            return Task::none();
        }
        let query = crate::lyrics::Query::from(track);
        let path = track.path.clone();
        Task::perform(
            async move { (path, crate::lyrics::load(query).await) },
            |(path, lyrics)| Message::LyricsLoaded(path, lyrics),
        )
    }

    /// True when panels should use translucent "glass" surfaces. A custom
    /// background image forces this on so the image stays visible.
    pub fn glass_surfaces(&self) -> bool {
        self.use_glassmorphism || self.background_image.is_some()
    }

    fn persist_config(&self) {
        let mut cfg = crate::config::get().clone();
        cfg.theme_mode = self.theme_mode.clone();
        cfg.theme_preset = self.theme_preset.clone();
        cfg.use_glassmorphism = self.use_glassmorphism;
        cfg.corner_radius = self.corner_radius;
        cfg.background_kind = self.background_kind.clone();
        cfg.background_color = self.background_color.clone();
        cfg.background_gradient_from = self.background_gradient_from.clone();
        cfg.background_gradient_to = self.background_gradient_to.clone();
        cfg.background_gradient_angle = self.background_gradient_angle;
        cfg.background_image = self.background_image.clone().unwrap_or_default();
        cfg.spectrum_style = self.spectrum_style.clone();
        cfg.cover_size = self.cover_size;
        let _ = crate::persist::save_config(&cfg);
    }

    fn send_mpris(&self, update: MprisUpdate) {
        let _ = self.mpris_update_tx.send(update);
    }

    fn notify_mpris_track(&self, status: PlaybackStatus) {
        if let Some(track) = &self.current_track {
            self.send_mpris(MprisUpdate::Metadata {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                duration_us: track.duration.as_micros() as i64,
                art_url: None,
            });
        }
        self.send_mpris(MprisUpdate::Status(status));
    }

    // ── Computed lists ────────────────────────────────────────────────────────

    pub fn artists(&self) -> Vec<String> {
        let query = self.sidebar_search.to_lowercase();
        let mut artists: Vec<String> = self.all_tracks.iter()
            .map(|t| if t.artist.trim().is_empty() { "Unknown Artist".to_string() } else { t.artist.clone() })
            .collect();
        artists.sort();
        artists.dedup();
        if !query.is_empty() {
            artists.retain(|a| a.to_lowercase().contains(&query));
        }
        artists.retain(|a| !self.hidden_artists_albums.contains(&(a.clone(), true)));
        artists
    }

    pub fn albums(&self) -> Vec<String> {
        let query = self.sidebar_search.to_lowercase();
        let mut albums: Vec<String> = self.all_tracks.iter()
            .map(|t| if t.album.trim().is_empty() { "Unknown Album".to_string() } else { t.album.clone() })
            .collect();
        albums.sort();
        albums.dedup();
        if !query.is_empty() {
            albums.retain(|a| a.to_lowercase().contains(&query));
        }
        albums.retain(|a| !self.hidden_artists_albums.contains(&(a.clone(), false)));
        albums
    }

    pub fn genres(&self) -> Vec<String> {
        let query = self.sidebar_search.to_lowercase();
        let mut genres: Vec<String> = self.all_tracks.iter()
            .map(|t| if t.genre.trim().is_empty() { "Unknown Genre".to_string() } else { t.genre.clone() })
            .collect();
        genres.sort();
        genres.dedup();
        if !query.is_empty() {
            genres.retain(|g| g.to_lowercase().contains(&query));
        }
        genres
    }

    // ── Filtragem e ordenação ─────────────────────────────────────────────────

    pub fn update_filtered_tracks(&mut self) {
        if !self.search_query.is_empty() {
            let q = self.search_query.to_lowercase();
            self.tracks = self.all_tracks.iter().filter(|t| {
                (self.filter_title && t.title.to_lowercase().contains(&q))
                    || (self.filter_artist && t.artist.to_lowercase().contains(&q))
                    || (self.filter_album && t.album.to_lowercase().contains(&q))
                    || (self.filter_genre && t.genre.to_lowercase().contains(&q))
            }).cloned().collect();
        } else if let Some(ref playlist_name) = self.selected_playlist.clone() {
            if playlist_name == "Liked Songs" {
                self.tracks = self.all_tracks.iter().filter(|t| t.liked).cloned().collect();
            } else if playlist_name == "Most Played" {
                let mut temp = self.all_tracks.clone();
                temp.sort_by(|a, b| b.play_count.cmp(&a.play_count));
                self.tracks = temp.into_iter().filter(|t| t.play_count > 0).collect();
                return;
            } else if playlist_name == "Recently Played" {
                let rp = crate::persist::get(|db| db.recently_played.clone());
                let mut temp = Vec::new();
                for (path, date_str) in rp {
                    if let Some(mut t) = self.all_tracks.iter().find(|t| t.path == path).cloned() {
                        t.date_played = Some(date_str);
                        temp.push(t);
                    }
                }
                self.tracks = temp;
                return;
            } else {
                // In the playlist's own order (an imported album plays in
                // album order), each song once, skipping songs no longer in
                // the library.
                let paths = crate::persist::get(|db| db.playlists.get(playlist_name).cloned().unwrap_or_default());
                let by_path: HashMap<&PathBuf, &Track> = self.all_tracks.iter().map(|t| (&t.path, t)).collect();
                let mut seen = HashSet::new();
                self.tracks = paths
                    .iter()
                    .filter(|p| seen.insert(*p))
                    .filter_map(|p| by_path.get(p).map(|t| (*t).clone()))
                    .collect();
            }
        } else {
            match self.view_mode {
                ViewMode::Artists => {
                    if let Some(ref artist) = self.selected_artist.clone() {
                        self.tracks = self.all_tracks.iter().filter(|t| {
                            let a = if t.artist.trim().is_empty() { "Unknown Artist" } else { &t.artist };
                            a == artist
                        }).cloned().collect();
                    } else {
                        self.tracks = Vec::new();
                    }
                }
                ViewMode::Albums => {
                    if let Some(ref album) = self.selected_album.clone() {
                        self.tracks = self.all_tracks.iter().filter(|t| {
                            let al = if t.album.trim().is_empty() { "Unknown Album" } else { &t.album };
                            al == album
                        }).cloned().collect();
                    } else {
                        self.tracks = Vec::new();
                    }
                }
                ViewMode::Genres => {
                    if let Some(ref genre) = self.selected_genre.clone() {
                        self.tracks = self.all_tracks.iter().filter(|t| {
                            let g = if t.genre.trim().is_empty() { "Unknown Genre" } else { &t.genre };
                            g == genre
                        }).cloned().collect();
                    } else {
                        self.tracks = Vec::new();
                    }
                }
            }
        }

        if let Some(col) = self.sort_column {
            self.tracks.sort_by(|a, b| {
                let cmp = match col {
                    SortColumn::TrackNumber => a.track_number.unwrap_or(u32::MAX).cmp(&b.track_number.unwrap_or(u32::MAX)),
                    SortColumn::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                    SortColumn::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                    SortColumn::Album => a.album.to_lowercase().cmp(&b.album.to_lowercase()),
                    SortColumn::Genre => a.genre.to_lowercase().cmp(&b.genre.to_lowercase()),
                    SortColumn::Year => a.year.unwrap_or(u32::MAX).cmp(&b.year.unwrap_or(u32::MAX)),
                    SortColumn::DiscNumber => a.disc_number.unwrap_or(u32::MAX).cmp(&b.disc_number.unwrap_or(u32::MAX)),
                    SortColumn::Duration => a.duration.cmp(&b.duration),
                    SortColumn::Plays => a.play_count.cmp(&b.play_count),
                    SortColumn::DatePlayed => a.date_played.as_deref().unwrap_or("").cmp(b.date_played.as_deref().unwrap_or("")),
                };
                if self.sort_ascending { cmp } else { cmp.reverse() }
            });
        }
    }

    pub fn calculate_scroll_offset(&self, track_id: i64) -> Option<f32> {
        use crate::ui::views::library::{ALBUM_GAP_H, ALBUM_HEADER_H, ROW_H};
        let row_h = ROW_H;
        if self.group_by_album {
            let mut y = 0.0;
            let mut groups: Vec<(String, Vec<&Track>)> = Vec::new();
            for t in &self.tracks {
                if let Some(last) = groups.last_mut() {
                    if last.0 == t.album { last.1.push(t); continue; }
                }
                groups.push((t.album.clone(), vec![t]));
            }
            for (_album, tracks) in groups {
                if tracks.iter().any(|t| t.id == track_id) {
                    let idx = tracks.iter().position(|t| t.id == track_id).unwrap();
                    y += ALBUM_HEADER_H + idx as f32 * row_h;
                    return Some(y);
                }
                y += ALBUM_HEADER_H + tracks.len() as f32 * row_h + ALBUM_GAP_H;
            }
            None
        } else {
            self.tracks.iter().position(|t| t.id == track_id).map(|i| i as f32 * row_h)
        }
    }

    // ── Update ────────────────────────────────────────────────────────────────

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {

            Message::LibraryScanned(tracks) => {
                self.all_tracks = tracks;
                // Auto-select first artist
                if self.selected_artist.is_none() {
                    self.selected_artist = self.artists().first().cloned();
                }
                self.update_filtered_tracks();
                self.persist_state();
                Task::none()
            }

            // ── YouTube search (yt-dlp) ──────────────────────────────────────

            Message::OpenYtSearch => {
                // yt-dlp is checked in the background (it takes a moment the
                // first time), so the dialog opens at once.
                self.yt_dialog = Some(YtSearchState {
                    query: self.search_query.clone(),
                    checking: true,
                    ..YtSearchState::default()
                });
                // Typing goes straight into the search box (not to the
                // player's shortcuts behind the dialog).
                Task::batch([
                    iced::widget::text_input::focus(iced::widget::text_input::Id::new(crate::ui::components::yt_search::QUERY_ID)),
                    Task::perform(
                        async { tokio::task::spawn_blocking(|| crate::youtube::preflight(crate::config::get())).await.ok().flatten() },
                        Message::YtChecked,
                    ),
                ])
            }

            Message::YtChecked(err) => {
                if let Some(d) = self.yt_dialog.as_mut() {
                    d.checking = false;
                    d.preflight_error = err;
                }
                Task::none()
            }

            Message::CloseYtSearch => {
                self.yt_dialog = None;
                Task::none()
            }

            Message::YtQueryChanged(v) => {
                if let Some(ref mut d) = self.yt_dialog {
                    d.query = v;
                }
                Task::none()
            }

            Message::YtSubmit => {
                let Some(d) = self.yt_dialog.as_mut() else { return Task::none() };
                let query = d.query.trim().to_string();
                if query.is_empty() {
                    d.error = Some("Type something to search for, or paste a link.".into());
                    return Task::none();
                }
                if !d.results.is_empty() && query == d.searched {
                    if let Some(i) = d.selected {
                        return Task::done(Message::YtDownload(i));
                    }
                }
                d.searched = query.clone();
                d.searching = true;
                d.error = None;
                d.status = None;
                if crate::youtube::looks_like_url(&query) {
                    return Task::perform(async move { crate::youtube::resolve(&query).await }, Message::YtResolved);
                }
                Task::perform(
                    async move { crate::youtube::search(&query, crate::config::get().yt_search_results).await },
                    Message::YtSearched,
                )
            }

            Message::YtResolved(res) => match res {
                Ok(r) => {
                    let n = r.items.len();
                    let list_title = r.list_title.clone();
                    let status = match &list_title {
                        Some(t) => format!("“{t}”: {n} songs. Download one, or all of them into a playlist"),
                        None => "Press Enter or Download to save it".to_string(),
                    };
                    let task = self.update(Message::YtSearched(Ok(r.items)));
                    if let Some(d) = self.yt_dialog.as_mut() {
                        d.list_title = list_title;
                        if d.error.is_none() {
                            d.status = Some(status);
                        }
                    }
                    task
                }
                Err(e) => self.update(Message::YtSearched(Err(e))),
            },

            Message::YtSearched(res) => {
                match res {
                    Ok(results) => {
                        if let Some(ref mut d) = self.yt_dialog {
                            d.searching = false;
                            d.preflight_error = None;
                            d.list_title = None;
                            if results.is_empty() {
                                d.error = Some("No results found.".into());
                            } else {
                                d.results = results;
                                d.thumbs.clear();
                                d.saved.clear();
                                d.preview = None;
                                d.selected = Some(0);
                                d.status = Some(format!("{} results", d.results.len()));
                                // Thumbnails load in the background, all at once.
                                let radius = self.corner_radius;
                                let preview = self.yt_preview_task();
                                let Some(d) = self.yt_dialog.as_ref() else { return preview };
                                return Task::batch(std::iter::once(preview).chain(d.results.iter().map(|r| {
                                    let id = r.id.clone();
                                    Task::perform(
                                        async move {
                                            let bytes = crate::youtube::thumbnail(id.clone()).await;
                                            let handle = tokio::task::spawn_blocking(move || {
                                                bytes.as_deref().and_then(|b| rounded_thumbnail(b, radius))
                                            })
                                            .await
                                            .ok()
                                            .flatten();
                                            (id, handle)
                                        },
                                        |(id, handle)| Message::YtThumb(id, handle),
                                    )
                                })));
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(ref mut d) = self.yt_dialog {
                            d.searching = false;
                            d.error = Some(e);
                        }
                    }
                }
                Task::none()
            }

            Message::YtSelect(i) => {
                if let Some(ref mut d) = self.yt_dialog {
                    d.selected = Some(i);
                }
                self.yt_preview_task()
            }

            Message::YtPreview(id, handle) => {
                if let (Some(d), Some(handle)) = (self.yt_dialog.as_mut(), handle) {
                    // Only if that result is still the selected one.
                    if d.selected.and_then(|i| d.results.get(i)).is_some_and(|r| r.id == id) {
                        d.preview = Some((id, handle));
                    }
                }
                Task::none()
            }

            Message::YtDownloadAll => {
                let Some(d) = self.yt_dialog.as_ref() else { return Task::none() };
                let stem = d.list_title.clone().unwrap_or_else(|| "Imported".into());
                let links: HashMap<String, String> = d.results.iter().map(|r| (r.id.clone(), r.url.clone())).collect();
                let rows: Vec<crate::playlist_io::Row> = d
                    .results
                    .iter()
                    .map(|r| crate::playlist_io::Row { id: r.id.clone(), title: r.title.clone(), artist: r.uploader.clone() })
                    .collect();
                self.yt_dialog = None;
                self.begin_import(stem, rows, move |r| links.get(&r.id).cloned().unwrap_or_default())
            }

            Message::YtDownload(i) => {
                let Some(ref mut d) = self.yt_dialog else { return Task::none() };
                if d.downloading.is_some() {
                    return Task::none();
                }
                let Some(r) = d.results.get(i) else { return Task::none() };
                let url = r.url.clone();
                let title = r.title.clone();
                d.downloading = Some(i);
                d.error = None;
                d.status = Some(format!("Downloading “{title}”…"));
                let task = Task::perform(
                    async move { crate::youtube::download(&url).await },
                    Message::YtDownloaded,
                );
                Task::batch(vec![task, Task::none()])
            }

            Message::YtDownloaded(res) => {
                match res {
                    Ok(path) => {
                        let msg = match path {
                            // File names can be long and contain quotes; a
                            // compact tail is more useful in a small dialog.
                            Some(p) => {
                                let name = p
                                    .file_name()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                let tail: String = name.chars().take(48).collect();
                                format!("Saved: {tail}")
                            }
                            None => "Download complete.".to_string(),
                        };
                        if let Some(ref mut d) = self.yt_dialog {
                            if let Some(i) = d.downloading.take() {
                                d.saved.insert(i);
                            }
                            d.status = Some(msg);
                        }
                        // Pick the new file up in the library right away.
                        return Task::done(Message::RescanLibrary);
                    }
                    Err(e) => {
                        if let Some(ref mut d) = self.yt_dialog {
                            d.downloading = None;
                            d.error = Some(e);
                        }
                    }
                }
                Task::none()
            }

            Message::YtThumb(id, handle) => {
                if let (Some(d), Some(handle)) = (self.yt_dialog.as_mut(), handle) {
                    d.thumbs.insert(id, handle);
                }
                Task::none()
            }

            Message::RescanLibrary => {
                let music_dir = crate::config::get().music_path();
                Task::perform(async move { scan_folder(&music_dir) }, Message::LibraryScanned)
            }

            Message::PlayPause => match self.playback_state {
                PlaybackState::Playing => {
                    self.audio.send(AudioCommand::Pause);
                    self.playback_state = PlaybackState::Paused;
                    self.send_mpris(MprisUpdate::Status(PlaybackStatus::Paused));
                    Task::none()
                }
                PlaybackState::Paused => {
                    self.audio.send(AudioCommand::Resume);
                    self.playback_state = PlaybackState::Playing;
                    self.send_mpris(MprisUpdate::Status(PlaybackStatus::Playing));
                    Task::none()
                }
                PlaybackState::Stopped => {
                    if let Some(ref sel) = self.selected_track.clone() {
                        self.queue = self.tracks.clone();
                        self.play_track_internal(sel.clone())
                    } else if let Some(first) = self.tracks.first().cloned() {
                        self.queue = self.tracks.clone();
                        self.play_track_internal(first)
                    } else {
                        Task::none()
                    }
                }
            },

            Message::NextTrack => self.advance_track(1),
            Message::PreviousTrack => self.advance_track(-1),

            Message::Seek(dur) => {
                self.audio.send(AudioCommand::Seek(dur));
                self.position = dur;
                Task::none()
            }

            Message::SeekRelative(delta_secs) => {
                let new_pos = if delta_secs < 0 {
                    self.position.saturating_sub(Duration::from_secs(delta_secs.unsigned_abs()))
                } else {
                    (self.position + Duration::from_secs(delta_secs as u64)).min(self.duration)
                };
                self.audio.send(AudioCommand::Seek(new_pos));
                self.position = new_pos;
                Task::none()
            }

            Message::VolumeChanged(v) => {
                self.volume = v;
                self.audio.send(AudioCommand::SetVolume(v));
                self.send_mpris(MprisUpdate::Volume(v as f64));
                self.persist_state();
                Task::none()
            }

            Message::VolumeStep(delta) => {
                let v = (self.volume + delta).clamp(0.0, 1.0);
                self.volume = v;
                self.audio.send(AudioCommand::SetVolume(v));
                self.send_mpris(MprisUpdate::Volume(v as f64));
                self.persist_state();
                Task::none()
            }

            Message::ToggleShuffle => {
                self.shuffle = !self.shuffle;
                self.send_mpris(MprisUpdate::Shuffle(self.shuffle));
                Task::none()
            }

            Message::ToggleRepeat => {
                self.repeat = !self.repeat;
                let loop_status = if self.repeat { LoopStatus::Track } else { LoopStatus::None };
                self.send_mpris(MprisUpdate::Loop(loop_status));
                Task::none()
            }

            Message::SidebarDragStart => { self.dragging_sidebar = true; Task::none() }
            Message::SidebarDragMove(x) => { self.sidebar_width = x.clamp(120.0, 400.0); Task::none() }
            Message::SidebarDragEnd => {
                self.dragging_sidebar = false;
                save_sidebar_width(self.sidebar_width);
                Task::none()
            }

            Message::PlaylistDragStart => { self.dragging_playlist_split = true; Task::none() }
            Message::PlaylistDragMove(y) => {
                self.playlist_height = (self.window_height - y - 60.0).clamp(50.0, self.window_height - 200.0);
                Task::none()
            }
            Message::PlaylistDragEnd => { self.dragging_playlist_split = false; Task::none() }

            Message::WindowResized(w, h) => {
                self.window_width = w;
                self.window_height = h;
                if !self.playlist_height_initialized {
                    self.playlist_height = ((h - 212.0) * 0.33).max(50.0);
                    self.playlist_height_initialized = true;
                }
                Task::none()
            }

            // ── ViewMode ──────────────────────────────────────────────────────

            Message::SelectViewMode(mode) => {
                self.view_mode = mode;
                self.selected_playlist = None;
                self.selected_artist = None;
                self.selected_album = None;
                self.selected_genre = None;
                self.search_query.clear();
                match mode {
                    ViewMode::Artists => { self.selected_artist = self.artists().first().cloned(); }
                    ViewMode::Albums => { self.selected_album = self.albums().first().cloned(); }
                    ViewMode::Genres => { self.selected_genre = self.genres().first().cloned(); }
                }
                self.update_filtered_tracks();
                Task::none()
            }

            Message::SelectArtist(artist) => {
                let now = Instant::now();
                if let Some((ref prev, last)) = self.last_click_artist {
                    if prev == &artist && now.duration_since(last) < Duration::from_millis(350) {
                        self.last_click_artist = None;
                        return Task::done(Message::DoubleClickArtist(artist));
                    }
                }
                self.last_click_artist = Some((artist.clone(), now));
                self.selected_artist = Some(artist);
                self.selected_playlist = None;
                self.selected_album = None;
                self.search_query.clear();
                self.update_filtered_tracks();
                Task::none()
            }

            Message::SelectAlbum(album) => {
                let now = Instant::now();
                if let Some((ref prev, last)) = self.last_click_album {
                    if prev == &album && now.duration_since(last) < Duration::from_millis(350) {
                        self.last_click_album = None;
                        return Task::done(Message::DoubleClickAlbum(album));
                    }
                }
                self.last_click_album = Some((album.clone(), now));
                self.selected_album = Some(album);
                self.selected_playlist = None;
                self.selected_artist = None;
                self.search_query.clear();
                self.update_filtered_tracks();
                Task::none()
            }

            Message::SelectGenre(genre) => {
                let now = Instant::now();
                if let Some((ref prev, last)) = self.last_click_genre {
                    if prev == &genre && now.duration_since(last) < Duration::from_millis(350) {
                        self.last_click_genre = None;
                        return Task::done(Message::DoubleClickGenre(genre));
                    }
                }
                self.last_click_genre = Some((genre.clone(), now));
                self.selected_genre = Some(genre);
                self.selected_playlist = None;
                self.selected_artist = None;
                self.selected_album = None;
                self.search_query.clear();
                self.update_filtered_tracks();
                Task::none()
            }

            Message::SelectPlaylist(name) => {
                let now = Instant::now();
                if let Some((ref prev, last)) = self.last_click_playlist {
                    if prev == &name && now.duration_since(last) < Duration::from_millis(350) {
                        self.last_click_playlist = None;
                        return Task::done(Message::DoubleClickPlaylist(name));
                    }
                }
                self.last_click_playlist = Some((name.clone(), now));
                self.selected_playlist = Some(name);
                self.search_query.clear();
                self.update_filtered_tracks();
                Task::none()
            }

            Message::SelectPlaylistTab(tab) => {
                self.playlist_tab = tab;
                self.selected_artist = None;
                self.selected_album = None;
                self.selected_genre = None;
                self.search_query.clear();
                match tab {
                    PlaylistTab::Playlists => {
                        let playlists = crate::persist::playlist_names();
                        self.selected_playlist = playlists.first().cloned();
                    }
                    PlaylistTab::Autoplaylists => {
                        self.selected_playlist = Some("Liked Songs".to_string());
                    }
                }
                self.update_filtered_tracks();
                Task::none()
            }

            // ── Double-click ──────────────────────────────────────────────────

            Message::DoubleClickTrack(track) => {
                self.selected_track = Some(track.clone());
                self.queue = self.tracks.clone();
                self.play_track_internal(track)
            }

            Message::DoubleClickArtist(artist) => {
                self.view_mode = ViewMode::Artists;
                self.selected_artist = Some(artist);
                self.selected_playlist = None;
                self.selected_album = None;
                self.search_query.clear();
                self.update_filtered_tracks();
                self.shuffle = true;
                use rand::seq::SliceRandom;
                let mut rng = rand::thread_rng();
                let mut shuffled = self.tracks.clone();
                shuffled.shuffle(&mut rng);
                self.queue = shuffled.clone();
                if let Some(first) = shuffled.first().cloned() { self.play_track_internal(first) } else { Task::none() }
            }

            Message::DoubleClickAlbum(album) => {
                self.view_mode = ViewMode::Albums;
                self.selected_album = Some(album);
                self.selected_playlist = None;
                self.selected_artist = None;
                self.search_query.clear();
                self.update_filtered_tracks();
                self.tracks.sort_by_key(|t| t.track_number.unwrap_or(u32::MAX));
                self.queue = self.tracks.clone();
                if let Some(first) = self.tracks.first().cloned() { self.play_track_internal(first) } else { Task::none() }
            }

            Message::DoubleClickGenre(genre) => {
                self.view_mode = ViewMode::Genres;
                self.selected_genre = Some(genre);
                self.selected_playlist = None;
                self.search_query.clear();
                self.update_filtered_tracks();
                self.queue = self.tracks.clone();
                if let Some(first) = self.tracks.first().cloned() { self.play_track_internal(first) } else { Task::none() }
            }

            Message::DoubleClickPlaylist(playlist) => {
                // Playlists customizadas: clique duplo renomeia. Autoplaylists: toca.
                let is_custom = crate::persist::get(|db| db.playlists.contains_key(&playlist));
                if is_custom {
                    return Task::done(Message::OpenPlaylistDialog(PlaylistDialogMode::Rename(playlist)));
                }
                self.selected_playlist = Some(playlist);
                self.search_query.clear();
                self.update_filtered_tracks();
                self.queue = self.tracks.clone();
                if let Some(first) = self.tracks.first().cloned() { self.play_track_internal(first) } else { Task::none() }
            }

            // ── Player navigation ─────────────────────────────────────────────

            Message::FocusSongName | Message::FocusAlbumName => {
                if let Some(current) = self.current_track.clone() {
                    self.view_mode = ViewMode::Albums;
                    self.selected_album = Some(current.album.clone());
                    self.selected_playlist = None;
                        self.selected_artist = None;
                    self.search_query.clear();
                    self.update_filtered_tracks();
                    self.selected_track = Some(current.clone());
                    if let Some(y) = self.calculate_scroll_offset(current.id) {
                        return self.scroll_tracklist_to(y);
                    }
                }
                Task::none()
            }

            Message::FocusArtistName => {
                if let Some(current) = self.current_track.clone() {
                    self.view_mode = ViewMode::Artists;
                    self.selected_artist = Some(current.artist.clone());
                    self.selected_playlist = None;
                        self.selected_album = None;
                    self.search_query.clear();
                    self.update_filtered_tracks();
                }
                Task::none()
            }

            // ── Track selection ───────────────────────────────────────────────

            Message::SelectTrack(track) => {
                let now = Instant::now();
                if let Some((prev_id, last)) = self.last_click_track {
                    if prev_id == track.id && now.duration_since(last) < Duration::from_millis(350) {
                        self.last_click_track = None;
                        return Task::done(Message::DoubleClickTrack(track));
                    }
                }
                self.last_click_track = Some((track.id, now));
                let cover_data = load_cover(&track.path);
                let track = Track { cover_data, ..track };

                let shift = self.modifiers.shift();
                let ctrl = self.modifiers.control() || self.modifiers.command();

                if ctrl {
                    if self.selected_tracks.iter().any(|t| t.id == track.id) {
                        self.selected_tracks.retain(|t| t.id != track.id);
                    } else {
                        self.selected_tracks.push(track.clone());
                    }
                    self.last_clicked_track = Some(track.clone());
                } else if shift {
                    if let Some(ref start) = self.last_clicked_track {
                        let s = self.tracks.iter().position(|t| t.id == start.id);
                        let e = self.tracks.iter().position(|t| t.id == track.id);
                        if let (Some(s), Some(e)) = (s, e) {
                            let (lo, hi) = if s < e { (s, e) } else { (e, s) };
                            self.selected_tracks = self.tracks[lo..=hi].to_vec();
                        }
                    } else {
                        self.selected_tracks = vec![track.clone()];
                        self.last_clicked_track = Some(track.clone());
                    }
                } else {
                    self.selected_tracks = vec![track.clone()];
                    self.last_clicked_track = Some(track.clone());
                }

                self.selected_track = Some(track);
                Task::none()
            }

            Message::ModifiersChanged(mods) => { self.modifiers = mods; Task::none() }

            // ── Busca ─────────────────────────────────────────────────────────

            Message::SearchChanged(q) => {
                self.search_query = q;
                self.update_filtered_tracks();
                Task::none()
            }

            Message::ToggleFilterTitle => { self.filter_title = !self.filter_title; self.update_filtered_tracks(); Task::none() }
            Message::ToggleFilterArtist => { self.filter_artist = !self.filter_artist; self.update_filtered_tracks(); Task::none() }
            Message::ToggleFilterAlbum => { self.filter_album = !self.filter_album; self.update_filtered_tracks(); Task::none() }
            Message::ToggleFilterGenre => { self.filter_genre = !self.filter_genre; self.update_filtered_tracks(); Task::none() }
            Message::SidebarSearchChanged(q) => { self.sidebar_search = q; Task::none() }

            // ── Sort ──────────────────────────────────────────────────────────

            Message::SortBy(col) => {
                if self.sort_column == Some(col) {
                    self.sort_ascending = !self.sort_ascending;
                } else {
                    self.sort_column = Some(col);
                    self.sort_ascending = true;
                }
                self.update_filtered_tracks();
                Task::none()
            }

            Message::ToggleGroupByAlbum => {
                self.group_by_album = !self.group_by_album;
                Task::none()
            }

            // ── Like ──────────────────────────────────────────────────────────

            Message::ToggleLikeTrack(track) => {
                self.show_context_menu = None;
                let liked = crate::persist::toggle_favorite(track.path.clone());
                for t in self.all_tracks.iter_mut().filter(|t| t.path == track.path) { t.liked = liked; }
                for t in self.tracks.iter_mut().filter(|t| t.path == track.path) { t.liked = liked; }
                if let Some(ref mut ct) = self.current_track { if ct.path == track.path { ct.liked = liked; } }
                if let Some(ref mut st) = self.selected_track { if st.path == track.path { st.liked = liked; } }
                self.update_filtered_tracks();
                Task::none()
            }

            Message::KeyboardLike => {
                if let Some(ref track) = self.current_track.clone() {
                    let mut t = track.clone();
                    t.cover_data = None;
                    return Task::done(Message::ToggleLikeTrack(t));
                }
                Task::none()
            }

            // ── Playlists ─────────────────────────────────────────────────────

            Message::OpenPlaylistDialog(mode) => {
                let initial = match &mode {
                    PlaylistDialogMode::Create => "My Playlist".to_string(),
                    PlaylistDialogMode::AddTrack(_) => String::new(),
                    PlaylistDialogMode::Rename(old) => old.clone(),
                };
                let playlists = crate::persist::playlist_names();
                let first = playlists.first().cloned();
                self.playlist_dialog = Some(PlaylistDialogState {
                    mode,
                    name_input: initial,
                    selected_playlist: first,
                    add_album: false,
                });
                Task::none()
            }

            Message::ClosePlaylistDialog => { self.playlist_dialog = None; Task::none() }

            Message::PlaylistInputChanged(val) => {
                if let Some(ref mut d) = self.playlist_dialog { d.name_input = val; }
                Task::none()
            }

            Message::PlaylistDialogSelect(name) => {
                if let Some(ref mut d) = self.playlist_dialog { d.selected_playlist = Some(name); }
                Task::none()
            }

            Message::PlaylistDialogToggleAddAlbum(val) => {
                if let Some(ref mut d) = self.playlist_dialog { d.add_album = val; }
                Task::none()
            }

            Message::PlaylistDialogSubmit => {
                if let Some(dialog) = self.playlist_dialog.clone() {
                    match dialog.mode {
                        PlaylistDialogMode::Create => {
                            let name = dialog.name_input.trim().to_string();
                            if !name.is_empty() { crate::persist::create_playlist(name); }
                        }
                        PlaylistDialogMode::AddTrack(track) => {
                            if let Some(pl) = dialog.selected_playlist {
                                if dialog.add_album {
                                    let album_tracks: Vec<_> = self.all_tracks.iter()
                                        .filter(|t| t.album == track.album).cloned().collect();
                                    for t in album_tracks { crate::persist::add_to_playlist(pl.clone(), t.path); }
                                } else {
                                    crate::persist::add_to_playlist(pl, track.path);
                                }
                            }
                        }
                        PlaylistDialogMode::Rename(old) => {
                            let new = dialog.name_input.trim().to_string();
                            if !new.is_empty() && new != old {
                                crate::persist::rename_playlist(old.clone(), new.clone());
                                if self.selected_playlist.as_ref() == Some(&old) {
                                    self.selected_playlist = Some(new);
                                }
                            }
                        }
                    }
                    self.playlist_dialog = None;
                    self.update_filtered_tracks();
                }
                Task::none()
            }

            Message::DeletePlaylist(name) => {
                crate::persist::delete_playlist(name.clone());
                if self.selected_playlist.as_ref() == Some(&name) {
                    self.selected_playlist = None;
                }
                self.update_filtered_tracks();
                Task::none()
            }

            Message::AddTracksToPlaylist(pl, tracks) => {
                for t in tracks { crate::persist::add_to_playlist(pl.clone(), t.path); }
                self.show_context_menu = None;
                self.update_filtered_tracks();
                Task::none()
            }

            Message::CreatePlaylistFromContext(target, is_artist) => {
                crate::persist::create_playlist(target.clone());
                let matched: Vec<_> = self.all_tracks.iter()
                    .filter(|t| {
                        if is_artist { t.artist == target } else { t.album == target }
                    })
                    .cloned().collect();
                for t in matched { crate::persist::add_to_playlist(target.clone(), t.path); }
                self.show_context_menu = None;
                self.update_filtered_tracks();
                Task::none()
            }

            Message::CreatePlaylistWithTracks(name, tracks) => {
                crate::persist::create_playlist(name.clone());
                for t in tracks { crate::persist::add_to_playlist(name.clone(), t.path); }
                self.show_context_menu = None;
                self.update_filtered_tracks();
                Task::none()
            }

            // ── Tag editor ────────────────────────────────────────────────────

            Message::OpenTagEditor(tracks) => {
                self.show_context_menu = None;
                if tracks.is_empty() { return Task::none(); }
                let first = &tracks[0];
                let all_same = |f: fn(&Track) -> &str| tracks.iter().all(|t| f(t) == f(first));
                let all_same_opt = |f: fn(&Track) -> Option<u32>| tracks.iter().all(|t| f(t) == f(first));
                self.show_tag_editor = Some(TagEditorState {
                    title: if all_same(|t| &t.title) { first.title.clone() } else { String::new() },
                    artist: if all_same(|t| &t.artist) { first.artist.clone() } else { String::new() },
                    album: if all_same(|t| &t.album) { first.album.clone() } else { String::new() },
                    genre: if all_same(|t| &t.genre) { first.genre.clone() } else { String::new() },
                    track_number: if all_same_opt(|t| t.track_number) { first.track_number.map(|n| n.to_string()).unwrap_or_default() } else { String::new() },
                    disc_number: if all_same_opt(|t| t.disc_number) { first.disc_number.map(|n| n.to_string()).unwrap_or_default() } else { String::new() },
                    year: if all_same_opt(|t| t.year) { first.year.map(|n| n.to_string()).unwrap_or_default() } else { String::new() },
                    cover_path: None,
                    apply_to_album: false,
                    apply_title: false, apply_artist: false, apply_album: false,
                    apply_year: false, apply_genre: false, apply_track_num: false,
                    apply_disc_num: false, apply_cover: false,
                    tracks,
                });
                Task::none()
            }

            Message::CloseTagEditor => { self.show_tag_editor = None; Task::none() }

            Message::UpdateTagFieldTitle(v) => { if let Some(ref mut s) = self.show_tag_editor { s.title = v; s.apply_title = true; } Task::none() }
            Message::UpdateTagFieldArtist(v) => { if let Some(ref mut s) = self.show_tag_editor { s.artist = v; s.apply_artist = true; } Task::none() }
            Message::UpdateTagFieldAlbum(v) => { if let Some(ref mut s) = self.show_tag_editor { s.album = v; s.apply_album = true; } Task::none() }
            Message::UpdateTagFieldGenre(v) => { if let Some(ref mut s) = self.show_tag_editor { s.genre = v; s.apply_genre = true; } Task::none() }
            Message::UpdateTagFieldTrackNumber(v) => { if let Some(ref mut s) = self.show_tag_editor { s.track_number = v; s.apply_track_num = true; } Task::none() }
            Message::UpdateTagFieldDiscNumber(v) => { if let Some(ref mut s) = self.show_tag_editor { s.disc_number = v; s.apply_disc_num = true; } Task::none() }
            Message::UpdateTagFieldYear(v) => { if let Some(ref mut s) = self.show_tag_editor { s.year = v; s.apply_year = true; } Task::none() }
            Message::UpdateTagFieldCoverPath(v) => { if let Some(ref mut s) = self.show_tag_editor { s.cover_path = Some(v); s.apply_cover = true; } Task::none() }
            Message::UpdateTagFieldApplyToAlbum(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_to_album = v; } Task::none() }
            Message::ToggleTagFieldApplyTitle(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_title = v; } Task::none() }
            Message::ToggleTagFieldApplyArtist(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_artist = v; } Task::none() }
            Message::ToggleTagFieldApplyAlbum(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_album = v; } Task::none() }
            Message::ToggleTagFieldApplyYear(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_year = v; } Task::none() }
            Message::ToggleTagFieldApplyGenre(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_genre = v; } Task::none() }
            Message::ToggleTagFieldApplyTrackNum(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_track_num = v; } Task::none() }
            Message::ToggleTagFieldApplyDiscNum(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_disc_num = v; } Task::none() }
            Message::ToggleTagFieldApplyCover(v) => { if let Some(ref mut s) = self.show_tag_editor { s.apply_cover = v; } Task::none() }

            Message::SearchCoverOnline => {
                if let Some(ref s) = self.show_tag_editor {
                    let q = url_encode(&format!("{} {} album art", s.artist, s.album));
                    let url = format!("https://www.google.com/search?q={}&tbm=isch", q);
                    spawn_detached(std::process::Command::new("xdg-open").arg(url));
                }
                Task::none()
            }

            Message::SaveTags => {
                let Some(ref state) = self.show_tag_editor else { return Task::none(); };
                let track_num = state.track_number.trim().parse::<u32>().ok();
                let disc_num = state.disc_number.trim().parse::<u32>().ok();
                let year_num = state.year.trim().parse::<u32>().ok();

                let tracks_to_update: Vec<Track> = if state.apply_to_album {
                    let albums: Vec<_> = state.tracks.iter().map(|t| t.album.clone()).collect();
                    self.all_tracks.iter().filter(|t| albums.contains(&t.album)).cloned().collect()
                } else {
                    state.tracks.clone()
                };

                // Clone state fields needed
                let apply_title = state.apply_title;
                let apply_artist = state.apply_artist;
                let apply_album = state.apply_album;
                let apply_genre = state.apply_genre;
                let apply_track_num = state.apply_track_num;
                let apply_disc_num = state.apply_disc_num;
                let apply_year = state.apply_year;
                let apply_cover = state.apply_cover;
                let title_val = state.title.clone();
                let artist_val = state.artist.clone();
                let album_val = state.album.clone();
                let genre_val = state.genre.clone();
                let cover_path_val = state.cover_path.clone();

                for track in tracks_to_update {
                    let title = if apply_title { &title_val } else { &track.title };
                    let artist = if apply_artist { &artist_val } else { &track.artist };
                    let album = if apply_album { &album_val } else { &track.album };
                    let genre = if apply_genre { &genre_val } else { &track.genre };
                    let tn = if apply_track_num { track_num } else { track.track_number };
                    let dn = if apply_disc_num { disc_num } else { track.disc_number };
                    let yr = if apply_year { year_num } else { track.year };
                    let cp = if apply_cover { cover_path_val.as_deref() } else { None };

                    if let Err(e) = crate::library::write_tags(&track.path, title, artist, album, genre, tn, dn, cp, yr) {
                        eprintln!("Error saving tags for {}: {e}", track.path.display());
                    } else {
                        for t in self.all_tracks.iter_mut().filter(|t| t.path == track.path) {
                            t.title = title.clone();
                            t.artist = artist.clone();
                            t.album = album.clone();
                            t.genre = genre.clone();
                            t.track_number = tn;
                            t.disc_number = dn;
                            t.year = yr;
                        }
                        for t in self.tracks.iter_mut().filter(|t| t.path == track.path) {
                            t.title = title.clone();
                            t.artist = artist.clone();
                            t.album = album.clone();
                            t.genre = genre.clone();
                            t.track_number = tn;
                            t.disc_number = dn;
                            t.year = yr;
                        }
                        if let Some(ref mut ct) = self.current_track {
                            if ct.path == track.path {
                                ct.title = title.clone();
                                ct.artist = artist.clone();
                                ct.album = album.clone();
                            }
                        }
                    }
                }
                self.show_tag_editor = None;
                self.update_filtered_tracks();
                Task::none()
            }

            // ── Keyboard edit helper ──────────────────────────────────────────

            Message::KeyboardEdit => {
                let tracks = if !self.selected_tracks.is_empty() {
                    let mut t = self.selected_tracks.clone();
                    for tr in &mut t { tr.cover_data = None; }
                    t
                } else if let Some(ref track) = self.current_track.clone() {
                    let mut t = track.clone();
                    t.cover_data = None;
                    vec![t]
                } else {
                    Vec::new()
                };
                if !tracks.is_empty() { return Task::done(Message::OpenTagEditor(tracks)); }
                Task::none()
            }

            Message::KeyboardAdd => {
                if let Some(ref track) = self.current_track.clone() {
                    let mut t = track.clone();
                    t.cover_data = None;
                    return Task::done(Message::OpenPlaylistDialog(PlaylistDialogMode::AddTrack(t)));
                }
                Task::none()
            }

            // ── Context menu ──────────────────────────────────────────────────

            Message::ToggleContextMenu(val) => { self.show_context_menu = val; Task::none() }

            Message::HideAlbumOrArtist(name, is_artist) => {
                self.hidden_artists_albums.push((name.clone(), is_artist));
                crate::persist::write(|db| { db.hidden_artists_albums.push((name, is_artist)); });
                self.show_context_menu = None;
                self.selected_artist = None;
                self.selected_album = None;
                self.selected_genre = None;
                self.update_filtered_tracks();
                Task::none()
            }

            Message::RestoreHiddenItems => {
                self.hidden_artists_albums.clear();
                crate::persist::write(|db| { db.hidden_artists_albums.clear(); });
                self.update_filtered_tracks();
                Task::none()
            }

            // ── Hover ─────────────────────────────────────────────────────────

            Message::HoverTracklist(v) => { self.is_hovering_tracklist = v; Task::none() }
            Message::HoverSidebarList(v) => { self.is_hovering_sidebar_list = v; Task::none() }

            // ── Columns ───────────────────────────────────────────────────────

            Message::ToggleColumnVisibility(col) => {
                crate::persist::write(|db| {
                    if db.table_columns.contains(&col) {
                        if db.table_columns.len() > 1 { db.table_columns.retain(|&c| c != col); }
                    } else {
                        db.table_columns.push(col);
                    }
                });
                Task::none()
            }

            Message::MoveColumnLeft(col) => {
                crate::persist::write(|db| {
                    if let Some(pos) = db.table_columns.iter().position(|&c| c == col) {
                        if pos > 0 { db.table_columns.swap(pos, pos - 1); }
                    }
                });
                Task::none()
            }

            Message::MoveColumnRight(col) => {
                crate::persist::write(|db| {
                    if let Some(pos) = db.table_columns.iter().position(|&c| c == col) {
                        if pos + 1 < db.table_columns.len() { db.table_columns.swap(pos, pos + 1); }
                    }
                });
                Task::none()
            }

            // ── Shortcuts ─────────────────────────────────────────────────────

            Message::OpenShortcuts => { self.show_shortcuts = true; Task::none() }
            Message::CloseShortcuts => { self.show_shortcuts = false; Task::none() }

            // ── Layout / espectro (lavanda) ────────────────────────────────────

            Message::ToggleSpectrum => { self.show_spectrum = !self.show_spectrum; Task::none() }

            Message::ToggleLayout => {
                self.layout = match self.layout {
                    Layout::Standard => Layout::Focus,
                    Layout::Focus => Layout::Standard,
                };
                Task::none()
            }

            Message::Tray(command) => {
                use crate::tray::TrayCommand;
                match command {
                    TrayCommand::PlayPause => self.update(Message::PlayPause),
                    TrayCommand::Next => self.advance_track(1),
                    TrayCommand::Previous => self.advance_track(-1),
                    // Wayland apps can't raise their own window; ask Hyprland
                    // (the app id is "lavanda"). Elsewhere this does nothing.
                    TrayCommand::ShowWindow => {
                        spawn_detached(
                            std::process::Command::new("hyprctl").args(["dispatch", "focuswindow", "class:^(lavanda)$"]),
                        );
                        Task::none()
                    }
                    TrayCommand::Quit => iced::exit(),
                }
            }

            // ── Playlist files ────────────────────────────────────────────────

            Message::ExportPlaylist(name) => {
                let paths = crate::persist::get(|db| db.playlists.get(&name).cloned().unwrap_or_default());
                let tracks = paths.iter().filter_map(|p| self.all_tracks.iter().find(|t| &t.path == p));
                let csv = crate::playlist_io::to_csv(tracks);
                let file_name = format!("{name}.csv");
                Task::perform(
                    async move {
                        let picked = rfd::AsyncFileDialog::new()
                            .set_file_name(&file_name)
                            .add_filter("Playlist (CSV)", &["csv"])
                            .save_file()
                            .await;
                        (csv, picked.map(|h| h.path().to_path_buf()))
                    },
                    |(csv, path)| Message::ExportPlaylistTo(csv, path),
                )
            }

            Message::ExportPlaylistTo(csv, path) => {
                if let Some(path) = path {
                    let shown = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let note = match std::fs::write(&path, csv) {
                        Ok(()) => format!("Saved {shown}"),
                        Err(e) => format!("Couldn't save {shown}: {e}"),
                    };
                    self.set_status(note);
                }
                Task::none()
            }

            Message::ImportPlaylist => {
                if self.import_job.is_some() {
                    self.set_status("An import is already running".into());
                    return Task::none();
                }
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Playlist (CSV)", &["csv", "txt"])
                            .pick_file()
                            .await
                            .map(|h| h.path().to_path_buf())
                    },
                    Message::ImportPlaylistFrom,
                )
            }

            Message::ImportPlaylistFrom(None) => Task::none(),
            Message::ImportPlaylistFrom(Some(path)) => {
                let rows = match std::fs::read_to_string(&path) {
                    Ok(text) => crate::playlist_io::parse_csv(&text),
                    Err(e) => {
                        self.set_status(format!("Couldn't read that file: {e}"));
                        return Task::none();
                    }
                };
                if rows.is_empty() {
                    self.set_status("No songs in that file".into());
                    return Task::none();
                }
                // Named after the file, never replacing an existing playlist.
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Imported".into());
                self.begin_import(stem, rows, |r| format!("https://www.youtube.com/watch?v={}", r.id))
            }

            Message::ImportNext => {
                let Some(job) = self.import_job.as_mut() else { return Task::none() };
                match job.pending.pop_front() {
                    Some(link) => Task::perform(
                        async move { crate::youtube::download(&link).await },
                        Message::ImportDownloaded,
                    ),
                    // All fetched: rescan, then link the new files.
                    None => {
                        let dir = crate::config::get().music_path();
                        Task::perform(
                            async move { tokio::task::spawn_blocking(move || scan_folder(&dir)).await.unwrap_or_default() },
                            Message::ImportScanned,
                        )
                    }
                }
            }

            Message::ImportDownloaded(result) => {
                if let Some(job) = self.import_job.as_mut() {
                    match result {
                        Ok(_) => job.done += 1,
                        Err(_) => job.failed += 1,
                    }
                }
                Task::done(Message::ImportNext)
            }

            Message::ImportScanned(tracks) => {
                let rescan = self.update(Message::LibraryScanned(tracks));
                if let Some(job) = self.import_job.take() {
                    let paths = self.import_paths(&job.rows);
                    let count = paths.len();
                    crate::persist::write(|db| {
                        db.playlists.insert(job.name.clone(), paths);
                    });
                    if self.selected_playlist.as_deref() == Some(job.name.as_str()) {
                        self.update_filtered_tracks();
                    }
                    self.set_status(match job.failed {
                        0 => format!("Imported “{}” ({count} songs)", job.name),
                        n => format!("Imported “{}” ({count} songs; {n} couldn't be downloaded)", job.name),
                    });
                }
                rescan
            }

            // ── Lyrics ────────────────────────────────────────────────────────

            Message::ToggleLyrics => {
                self.show_lyrics = !self.show_lyrics;
                self.persist_state();
                Task::batch([self.ensure_lyrics(), self.ensure_lyrics_alt(), self.ensure_furigana()])
            }

            Message::LyricsLoaded(path, lyrics) => {
                self.lyrics_loading.remove(&path);
                self.lyrics.insert(path, lyrics);
                Task::batch([self.ensure_lyrics_alt(), self.ensure_furigana()])
            }

            Message::ToggleLyricsRomaji(on) => {
                self.lyrics_romaji = on;
                self.persist_state();
                Task::batch([self.ensure_lyrics_alt(), self.ensure_furigana()])
            }

            Message::LyricsAltLoaded(path, lyrics) => {
                self.lyrics_alt_loading.remove(&path);
                self.lyrics_alt.insert(path, lyrics);
                self.ensure_furigana()
            }

            Message::FuriganaLoaded(key, ruby) => {
                self.furigana_loading.remove(&key);
                self.furigana.insert(key, ruby);
                Task::none()
            }

            Message::LyricsDragStart => { self.dragging_lyrics = true; Task::none() }
            Message::LyricsDragMove(x) => {
                // The panel sits against the right edge of the window.
                self.lyrics_width = (self.window_width - x).clamp(LYRICS_MIN_W, LYRICS_MAX_W);
                Task::none()
            }
            Message::LyricsDragEnd => {
                self.dragging_lyrics = false;
                self.persist_state();
                Task::none()
            }

            Message::LyricsSyncLine(line_time) => {
                // Delay everything so that this line starts now. Fixes lyrics
                // timed for a cut without the music video's intro, and the like.
                let secs = self.position.as_secs_f64() - line_time.as_secs_f64();
                self.set_lyrics_offset(secs as f32)
            }
            Message::LyricsNudge(delta) => self.set_lyrics_offset(self.lyrics_offset + delta),
            Message::LyricsOffsetReset => self.set_lyrics_offset(0.0),

            Message::TracklistScrolled(offset, height) => {
                self.tracklist_offset = offset;
                self.tracklist_view_h = height;
                Task::none()
            }

            Message::SpectrumData(bins) => {
                // Bars jump up with a hit and fall back more slowly, so beats
                // read clearly without the display flickering.
                for (a, b) in self.spectrum.iter_mut().zip(bins.iter()) {
                    let k = if b > a { 0.7 } else { 0.25 };
                    *a += (b - *a) * k;
                }
                Task::none()
            }

            // ── Keyboard ─────────────────────────────────────────────────────

            Message::KeyboardArrowUp => {
                if self.is_hovering_tracklist && !self.tracks.is_empty() {
                    let cur_idx = self.selected_track.as_ref()
                        .and_then(|st| self.tracks.iter().position(|t| t.id == st.id));
                    let next_idx = match cur_idx {
                        Some(i) => if i == 0 { self.tracks.len() - 1 } else { i - 1 },
                        None => 0,
                    };
                    if let Some(track) = self.tracks.get(next_idx).cloned() {
                        let cover_data = load_cover(&track.path);
                        let track = Track { cover_data, ..track };
                        self.selected_track = Some(track.clone());
                        self.selected_tracks = vec![track.clone()];
                        self.last_clicked_track = Some(track.clone());
                        if let Some(y) = self.calculate_scroll_offset(track.id) {
                            return self.scroll_tracklist_to(y);
                        }
                    }
                }
                Task::none()
            }

            Message::KeyboardArrowDown => {
                if self.is_hovering_tracklist && !self.tracks.is_empty() {
                    let cur_idx = self.selected_track.as_ref()
                        .and_then(|st| self.tracks.iter().position(|t| t.id == st.id));
                    let next_idx = match cur_idx {
                        Some(i) => (i + 1) % self.tracks.len(),
                        None => 0,
                    };
                    if let Some(track) = self.tracks.get(next_idx).cloned() {
                        let cover_data = load_cover(&track.path);
                        let track = Track { cover_data, ..track };
                        self.selected_track = Some(track.clone());
                        self.selected_tracks = vec![track.clone()];
                        self.last_clicked_track = Some(track.clone());
                        if let Some(y) = self.calculate_scroll_offset(track.id) {
                            return self.scroll_tracklist_to(y);
                        }
                    }
                }
                Task::none()
            }

            Message::KeyPressed(key) => {
                use iced::keyboard::Key;
                use iced::keyboard::key::Named;
                let seek = crate::config::get().seek_step as i64;
                let vol = crate::config::get().volume_step;

                let has_tag_editor = self.show_tag_editor.is_some();
                let has_playlist_dialog = self.playlist_dialog.is_some();
                let has_yt_dialog = self.yt_dialog.is_some();
                let has_shortcuts = self.show_shortcuts;
                let has_settings = self.show_settings;
                let has_context_menu = self.show_context_menu.is_some();
                let ctrl = self.modifiers.control() || self.modifiers.command();

                match key {
                    Key::Named(Named::Escape) => {
                        if has_settings { return Task::done(Message::CloseSettings); }
                        if has_yt_dialog { return Task::done(Message::CloseYtSearch); }
                        if has_shortcuts { return Task::done(Message::CloseShortcuts); }
                        if has_playlist_dialog { return Task::done(Message::ClosePlaylistDialog); }
                        if has_tag_editor { return Task::done(Message::CloseTagEditor); }
                        if has_context_menu { return Task::done(Message::ToggleContextMenu(None)); }
                    }
                    Key::Named(Named::Enter) => {
                        if has_yt_dialog {
                            // Enter downloads the highlighted result if there
                            // is one, otherwise it runs the search.
                            let target = self.yt_dialog.as_ref().and_then(|d| d.selected);
                            return match target {
                                Some(i) => Task::done(Message::YtDownload(i)),
                                None => Task::done(Message::YtSubmit),
                            };
                        }
                        if has_tag_editor { return Task::done(Message::SaveTags); }
                        if has_playlist_dialog { return Task::done(Message::PlaylistDialogSubmit); }
                        if !has_shortcuts && !has_context_menu {
                            if let Some(ref track) = self.selected_track.clone() {
                                return Task::done(Message::DoubleClickTrack(track.clone()));
                            }
                        }
                    }
                    Key::Named(Named::Space) if !has_playlist_dialog && !has_tag_editor && !has_yt_dialog => {
                        return Task::done(Message::PlayPause);
                    }
                    Key::Named(Named::ArrowRight) => return Task::done(Message::SeekRelative(seek)),
                    Key::Named(Named::ArrowLeft) => return Task::done(Message::SeekRelative(-seek)),
                    Key::Named(Named::ArrowUp) | Key::Named(Named::ArrowDown) if has_yt_dialog => {
                        // Move the highlight through the result list instead of
                        // the (hidden behind the dialog) library cursor.
                        let dir = if matches!(key, Key::Named(Named::ArrowDown)) { 1i64 } else { -1i64 };
                        let (len, cur) = match self.yt_dialog.as_ref() {
                            Some(d) => (d.results.len() as i64, d.selected),
                            None => (0, None),
                        };
                        if len > 0 {
                            let next = match cur {
                                None => 0,
                                Some(c) => (c as i64 + dir).rem_euclid(len),
                            };
                            return Task::done(Message::YtSelect(next as usize));
                        }
                        return Task::none();
                    }
                    Key::Named(Named::ArrowUp) => return Task::done(Message::KeyboardArrowUp),
                    Key::Named(Named::ArrowDown) => return Task::done(Message::KeyboardArrowDown),
                    Key::Named(Named::F5) => return Task::done(Message::RescanLibrary),
                    Key::Character(ref c) if ctrl => {
                        if c.as_str() == "k" || c.as_str() == "K" {
                            return if has_shortcuts {
                                Task::done(Message::CloseShortcuts)
                            } else {
                                Task::done(Message::OpenShortcuts)
                            };
                        }
                        if c.as_str() == "," {
                            return Task::done(Message::OpenSettings);
                        }
                    }
                    // While a text-entry dialog is open, printable characters
                    // belong to its inputs, so library shortcuts must not fire.
                    // Returning from the whole arm would swallow these keys from
                    // the text input, so just fall through to no-op.
                    Key::Character(ref _c) if has_yt_dialog || has_settings => {}
                    Key::Character(ref c) if !has_playlist_dialog && !has_tag_editor => {
                        match c.as_str() {
                            "n" | "N" => return Task::done(Message::NextTrack),
                            "p" | "P" => return Task::done(Message::PreviousTrack),
                            "s" | "S" => return Task::done(Message::ToggleShuffle),
                            "r" | "R" => return Task::done(Message::ToggleRepeat),
                            "+" | "=" => return Task::done(Message::VolumeStep(vol)),
                            "-" => return Task::done(Message::VolumeStep(-vol)),
                            "/" => return Task::done(Message::SearchChanged(String::new())),
                            "l" | "L" | "f" | "F" => return Task::done(Message::KeyboardLike),
                            "e" | "E" | "m" | "M" => return Task::done(Message::KeyboardEdit),
                            "c" | "C" => return Task::done(Message::OpenPlaylistDialog(PlaylistDialogMode::Create)),
                            "a" | "A" => return Task::done(Message::KeyboardAdd),
                            "t" | "T" => return Task::done(Message::ToggleLayout),
                            "v" | "V" => return Task::done(Message::ToggleSpectrum),
                            "k" | "K" => return Task::done(Message::ToggleLyrics),
                            "[" => return Task::done(Message::LyricsNudge(-0.5)),
                            "]" => return Task::done(Message::LyricsNudge(0.5)),
                            "y" | "Y" if !has_yt_dialog => return Task::done(Message::OpenYtSearch),
                            "?" => return Task::done(Message::OpenShortcuts),
                            _ => {}
                        }
                    }
                    _ => {}
                }
                Task::none()
            }

            // ── Open local folder ─────────────────────────────────────────────

            Message::OpenLocalFolder(path) => {
                self.show_context_menu = None;
                if let Some(parent) = path.parent() {
                    let folder = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
                    for fm in &["nautilus", "thunar", "dolphin", "nemo", "pcmanfm-qt", "pcmanfm"] {
                        if spawn_detached(std::process::Command::new(fm).arg(&folder)) { break; }
                    }
                }
                Task::none()
            }

            Message::OpenSettings => {
                self.show_settings = true;
                Task::none()
            }
            Message::CloseSettings => {
                // Settings preview live, so closing keeps the current choices
                // (instead of silently reverting them).
                self.persist_config();
                self.show_settings = false;
                Task::none()
            }
            Message::SettingsThemeModeChanged(mode) => {
                self.theme_mode = mode;
                // Live preview while the settings dialog is open.
                crate::ui::theme::reload_with_mode(&self.theme_mode, &self.theme_preset);
                self.iced_theme = build_iced_theme();
                Task::none()
            }
            Message::SettingsThemePresetChanged(preset) => {
                self.theme_preset = preset;
                crate::ui::theme::reload_with_mode(&self.theme_mode, &self.theme_preset);
                self.iced_theme = build_iced_theme();
                Task::none()
            }
            Message::SettingsGlassmorphismToggled(v) => {
                self.use_glassmorphism = v;
                Task::none()
            }
            Message::SettingsCornerRadiusChanged(v) => {
                self.corner_radius = v.clamp(4.0, 20.0);
                crate::ui::theme::set_corner_radius(self.corner_radius);
                self.refresh_cover()
            }
            Message::SettingsBackgroundKindChanged(k) => {
                self.background_kind = k.to_lowercase();
                Task::none()
            }
            Message::SettingsBackgroundColorChanged(c) => {
                self.background_color = c;
                Task::none()
            }
            Message::SettingsBackgroundGradientFromChanged(c) => {
                self.background_gradient_from = c;
                Task::none()
            }
            Message::SettingsBackgroundGradientToChanged(c) => {
                self.background_gradient_to = c;
                Task::none()
            }
            Message::SettingsBackgroundGradientAngleChanged(a) => {
                self.background_gradient_angle = a;
                Task::none()
            }
            Message::SettingsBackgroundImageChanged(p) => {
                // Store as-is; may need to expand ~ later
                self.background_image = if p.trim().is_empty() { None } else { Some(p) };
                Task::none()
            }
            Message::SettingsSelectBackgroundImage => Task::perform(
                async {
                    let handle = rfd::AsyncFileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
                        .pick_file()
                        .await;
                    handle.map(|h| h.path().to_path_buf())
                },
                Message::SettingsBackgroundImageSelected,
            ),
            Message::SettingsBackgroundImageSelected(path) => {
                if let Some(p) = path {
                    self.background_image = Some(p.to_string_lossy().to_string());
                }
                Task::none()
            }
            Message::SettingsSpectrumStyleChanged(style) => {
                self.spectrum_style = style;
                Task::none()
            }
            Message::SettingsCoverSizeChanged(size) => {
                self.cover_size = size.clamp(COVER_MIN, COVER_MAX).round();
                self.refresh_cover()
            }
            Message::CoverBlurred(path, handle, rounded) => {
                // Ignore a result for a track that is no longer playing.
                if self.current_track.as_ref().is_some_and(|t| t.path == path) {
                    let (blurred, tone) = handle.map_or((None, None), |(h, t)| (Some(h), Some(t)));
                    self.blurred_cover = blurred;
                    self.cover_tone = tone;
                    self.cover_rounded = rounded;
                }
                Task::none()
            }
            Message::SettingsApply => {
                crate::ui::theme::reload_with_mode(&self.theme_mode, &self.theme_preset);
                self.iced_theme = build_iced_theme();
                self.loaded_theme_name = crate::ui::theme::read_current_theme_name();
                self.persist_config();
                self.show_settings = false;
                Task::none()
            }

            // ── Eventos de áudio ──────────────────────────────────────────────

            Message::Audio(event) => match event {
                AudioEvent::Progress { position, duration } => {
                    self.position = position;
                    // Zero when the file doesn't state its length: use the
                    // length read from the tags instead.
                    self.duration = if duration.is_zero() {
                        self.current_track.as_ref().map_or(Duration::ZERO, |t| t.duration)
                    } else {
                        duration
                    };
                    Task::none()
                }
                AudioEvent::Paused => { self.playback_state = PlaybackState::Paused; Task::none() }
                AudioEvent::Stopped => {
                    self.playback_state = PlaybackState::Stopped;
                    self.position = Duration::ZERO;
                    self.send_mpris(MprisUpdate::Status(PlaybackStatus::Stopped));
                    Task::none()
                }
                AudioEvent::TrackEnded => {
                    if self.repeat {
                        if let Some(t) = self.current_track.clone() {
                            self.audio.send(AudioCommand::Play(t.path));
                            self.notify_mpris_track(PlaybackStatus::Playing);
                        }
                        Task::none()
                    } else {
                        let current_idx = self.current_track.as_ref()
                            .and_then(|ct| self.queue.iter().position(|t| t.id == ct.id));
                        let is_last = match current_idx {
                            Some(i) => i + 1 >= self.queue.len(),
                            None => true,
                        };
                        if is_last && !self.shuffle {
                            self.audio.send(AudioCommand::Stop);
                            self.playback_state = PlaybackState::Stopped;
                            self.position = Duration::ZERO;
                            self.send_mpris(MprisUpdate::Status(PlaybackStatus::Stopped));
                            Task::none()
                        } else {
                            self.advance_track(1)
                        }
                    }
                }
                AudioEvent::Error(e) => { eprintln!("Audio error: {e}"); Task::none() }
                AudioEvent::Playing => { self.playback_state = PlaybackState::Playing; Task::none() }
            },

            Message::Mpris(cmd) => match cmd {
                MprisCommand::Play => {
                    if !matches!(self.playback_state, PlaybackState::Playing) {
                        self.audio.send(AudioCommand::Resume);
                        self.playback_state = PlaybackState::Playing;
                        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Playing));
                    }
                    Task::none()
                }
                MprisCommand::Pause => {
                    if matches!(self.playback_state, PlaybackState::Playing) {
                        self.audio.send(AudioCommand::Pause);
                        self.playback_state = PlaybackState::Paused;
                        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Paused));
                    }
                    Task::none()
                }
                MprisCommand::PlayPause => {
                    match self.playback_state {
                        PlaybackState::Playing => {
                            self.audio.send(AudioCommand::Pause);
                            self.playback_state = PlaybackState::Paused;
                            self.send_mpris(MprisUpdate::Status(PlaybackStatus::Paused));
                        }
                        _ => {
                            self.audio.send(AudioCommand::Resume);
                            self.playback_state = PlaybackState::Playing;
                            self.send_mpris(MprisUpdate::Status(PlaybackStatus::Playing));
                        }
                    }
                    Task::none()
                }
                MprisCommand::Next => self.advance_track(1),
                MprisCommand::Previous => self.advance_track(-1),
                MprisCommand::Stop => {
                    self.audio.send(AudioCommand::Stop);
                    self.playback_state = PlaybackState::Stopped;
                    self.position = Duration::ZERO;
                    self.send_mpris(MprisUpdate::Status(PlaybackStatus::Stopped));
                    Task::none()
                }
            },

            Message::CheckTheme => {
                // Only follow the live Omarchy/desktop theme when in "auto"
                // mode. Manual mode keeps the user's chosen preset.
                if self.theme_mode.eq_ignore_ascii_case("auto") {
                    let current = crate::ui::theme::read_current_theme_name();
                    if !current.is_empty() && current != self.loaded_theme_name {
                        crate::ui::theme::reload_system_theme();
                        self.iced_theme = build_iced_theme();
                        self.loaded_theme_name = current;
                    }
                }
                Task::none()
            }
        }
    }

    // ── View ──────────────────────────────────────────────────────────────────

    fn view(&self) -> Element<'_, Message> {
        let main: Element<Message> = match self.layout {
            Layout::Focus => column![self.header_view(), views::focus::view(self)]
                .spacing(0).width(Length::Fill).height(Length::Fill).into(),
            Layout::Standard => {
                let library: Element<Message> = if self.show_lyrics {
                    row![views::library::view(self), views::lyrics::side_panel(self)].into()
                } else {
                    views::library::view(self)
                };
                // Header, player, sidebar, track list and lyrics are separate
                // rounded cards with even gaps between them.
                let main_col = column![
                    self.header_view(),
                    views::player::view(self),
                    library,
                ]
                .spacing(theme::CARD_GAP)
                .padding(theme::CARD_GAP)
                .width(Length::Fill)
                .height(Length::Fill);

                let bg_style: Box<dyn Fn(&Theme) -> iced::widget::container::Style> = if self.glass_surfaces() {
                    Box::new(theme::base_style_glass)
                } else {
                    match self.background_kind.as_str() {
                        "color" => {
                            if let Some(c) = theme::parse_color_str(&self.background_color) {
                                Box::new(theme::base_style_solid(c))
                            } else {
                                Box::new(|_: &Theme| iced::widget::container::Style {
                                    background: Some(iced::Background::Color(theme::base())),
                                    ..Default::default()
                                })
                            }
                        }
                        "gradient" => {
                            let from = theme::parse_color_str(&self.background_gradient_from).unwrap_or(theme::base());
                            let to = theme::parse_color_str(&self.background_gradient_to).unwrap_or(theme::mantle());
                            Box::new(theme::base_style_gradient(from, to, self.background_gradient_angle))
                        }
                        _ => {
                            Box::new(|_: &Theme| iced::widget::container::Style {
                                background: Some(iced::Background::Color(theme::base())),
                                ..Default::default()
                            })
                        }
                    }
                };
                container(main_col)
                    .style(move |theme: &Theme| bg_style(theme))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        };

        let mut view_stack: iced::widget::Stack<'_, Message> = match self.background_kind.as_str() {
            "image" => {
                let path = self.background_image.as_deref().unwrap_or("");
                if !path.trim().is_empty() {
                    let expanded = crate::config::expand_tilde(path);
                    let handle = iced::widget::image::Handle::from_path(expanded);
                    let bg: Element<Message> = iced::widget::image(handle)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced::ContentFit::Cover)
                        .into();
                    stack![bg, main]
                } else {
                    stack![main]
                }
            }
            "album" => {
                // A blurred wash of the playing track's cover, like Apple
                // Music's player; no cover (or nothing playing) = plain theme.
                match &self.blurred_cover {
                    Some(handle) => {
                        let bg: Element<Message> = iced::widget::image(handle.clone())
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .content_fit(iced::ContentFit::Cover)
                            .opacity(self.cover_tone.map_or(0.75, |t| t.backdrop_opacity()))
                            .into();
                        stack![bg, main]
                    }
                    None => stack![main],
                }
            }
            _ => stack![main],
        };

        // Tag editor overlay
        if let Some(ref editor_state) = self.show_tag_editor {
            view_stack = view_stack.push(crate::ui::components::tag_editor::view(editor_state));
        }

        // Playlist dialog overlay
        if let Some(ref pd) = self.playlist_dialog {
            view_stack = view_stack.push(crate::ui::components::playlist_dialog::view(pd));
        }

        // YouTube search overlay — pushed last so it sits above everything else.
        if let Some(ref yd) = self.yt_dialog {
            view_stack = view_stack.push(crate::ui::components::yt_search::view(yd));
        }

        // Shortcuts overlay
        if self.show_shortcuts {
            view_stack = view_stack.push(self.shortcuts_modal_view());
        }

        // Context menu overlay
        if let Some(ref target) = self.show_context_menu {
            view_stack = view_stack.push(self.context_menu_view(target));
        }

        // Settings overlay
        if self.show_settings {
            view_stack = view_stack.push(crate::ui::components::settings::view(
                &self.theme_mode,
                &self.theme_preset,
                self.use_glassmorphism,
                &self.background_kind,
                &self.background_color,
                &self.background_gradient_from,
                &self.background_gradient_to,
                self.background_gradient_angle,
                self.background_image.as_deref().unwrap_or(""),
                self.corner_radius,
                &self.spectrum_style,
                self.cover_size,
            ));
        }

        view_stack.into()
    }

    fn header_view(&self) -> Element<'_, Message> {
        use iced::widget::button;
        let settings_btn = button(
            text(crate::ui::icons::ICON_SETTINGS)
                .font(crate::ui::icons::NERD_FONT_MONO)
                .color(theme::text())
                .size(24),
        )
        .on_press(Message::OpenSettings)
        .style(iced::widget::button::text)
        .padding([0, 4]);

        let lyrics_btn = button(
            text(crate::ui::icons::ICON_LYRICS)
                .font(crate::ui::icons::NERD_FONT_MONO)
                .color(if self.show_lyrics { theme::accent() } else { theme::text() })
                .size(24),
        )
        .on_press(Message::ToggleLyrics)
        .style(iced::widget::button::text)
        .padding([0, 4]);

        let header_style = if self.glass_surfaces() { theme::header_glass } else { theme::header };
        container(
            row![
                iced::widget::image(self.logo.clone()).width(28).height(28),
                Space::with_width(8),
                text(APP_NAME)
                    .color(theme::accent())
                    .size(16)
                    .font(crate::ui::icons::UI_FONT_BOLD),
                Space::with_width(Length::Fill),
                text(self.status_text().unwrap_or_default())
                    .size(12)
                    .color(theme::subtext())
                    .shaping(iced::widget::text::Shaping::Advanced),
                Space::with_width(12),
                lyrics_btn,
                settings_btn,
            ]
            .align_y(Alignment::Center),
        )
        .style(header_style)
        .width(Length::Fill)
        .padding([0, 16])
        .into()
    }

    fn shortcuts_modal_view(&self) -> Element<'_, Message> {
        use iced::widget::button;

        let row_item = |keys: &'static str, desc: &'static str| -> Element<'_, Message> {
            row![
                text(keys).width(Length::Fixed(120.0)).font(crate::ui::icons::UI_FONT_BOLD)
                    .color(theme::accent()).size(13),
                text(desc).color(theme::text()).size(13),
            ]
            .spacing(12).align_y(Alignment::Center).into()
        };

        let content = column![
            row![
                text("Keyboard Shortcuts").size(18).font(crate::ui::icons::UI_FONT_BOLD).color(theme::accent()),
                Space::with_width(Length::Fill),
                button(text(crate::ui::icons::ICON_CLOSE).font(crate::ui::icons::NERD_FONT_MONO).color(theme::red()).size(32))
                    .on_press(Message::CloseShortcuts).style(iced::widget::button::text),
            ].align_y(Alignment::Center),
            Space::with_height(16),
            row_item("Space", "Play / Pause"),
            row_item("N", "Next track"),
            row_item("P", "Previous track"),
            row_item("L / F", "Like / Unlike song"),
            row_item("E / M", "Edit metadata tags"),
            row_item("C", "Create playlist"),
            row_item("A", "Add song to playlist"),
            row_item("T", "Toggle focus mode"),
            row_item("V", "Toggle spectrum visualizer"),
            row_item("K", "Toggle lyrics"),
            row_item("Right-click", "Lyrics: sync that line to now"),
            row_item("[ / ]", "Lyrics 0.5 s earlier / later"),
            row_item("←/→", "Seek backward / forward"),
            row_item("↑/↓", "Navigate track list"),
            row_item("Y", "Search YouTube & download"),
            row_item("F5", "Rescan library"),
            row_item("+ / -", "Volume up / down"),
            row_item("Ctrl+,", "Open settings"),
            row_item("?", "This help"),
        ]
        .spacing(8).padding(24);

        let dialog = container(content)
            .width(440)
            .style(|_| iced::widget::container::Style {
                background: Some(iced::Background::Color(theme::base())),
                border: iced::Border { color: theme::accent(), width: 1.0, radius: crate::ui::theme::corner_radius().into() },
                shadow: iced::Shadow {
                    color: theme::mantle(),
                    offset: iced::Vector { x: 0.0, y: 4.0 },
                    blur_radius: 12.0,
                },
                ..Default::default()
            });

        container(dialog)
            .width(Length::Fill).height(Length::Fill)
            .center_x(Length::Fill).center_y(Length::Fill)
            .style(|_| iced::widget::container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.6))),
                ..Default::default()
            })
            .into()
    }

    fn context_menu_view(&self, target: &ContextMenuTarget) -> Element<'_, Message> {
        use iced::widget::button;

        let custom_playlists = crate::persist::playlist_names();

        let item_style = |_: &iced::Theme, status: iced::widget::button::Status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed);
            iced::widget::button::Style {
                background: if hovered { Some(iced::Background::Color(theme::with_alpha(theme::accent(), 0.2))) } else { None },
                text_color: if hovered { theme::accent() } else { theme::text() },
                border: iced::Border { radius: (crate::ui::theme::corner_radius() / 2.0).into(), ..Default::default() },
                ..Default::default()
            }
        };

        let accent_item_style = |_: &iced::Theme, status: iced::widget::button::Status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed);
            iced::widget::button::Style {
                background: if hovered { Some(iced::Background::Color(theme::with_alpha(theme::accent(), 0.2))) } else { None },
                text_color: theme::accent(),
                border: iced::Border { radius: (crate::ui::theme::corner_radius() / 2.0).into(), ..Default::default() },
                ..Default::default()
            }
        };

        let mut playlist_section = column![
            text("Add to Playlist:").size(11).color(theme::subtext()).font(crate::ui::icons::UI_FONT_BOLD)
        ].spacing(4);

        let (title, extra_btns, create_btn) = match target {
            ContextMenuTarget::Artist(name) => {
                let hide_btn: Element<Message> = button(text("Hide from UI").size(13))
                    .on_press(Message::HideAlbumOrArtist(name.clone(), true))
                    .style(item_style).padding([4, 8]).width(Length::Fill).into();
                for pl in &custom_playlists {
                    let artist_tracks: Vec<_> = self.all_tracks.iter()
                        .filter(|t| t.artist == *name).cloned().collect();
                    playlist_section = playlist_section.push(
                        button(text(format!("  + {pl}")).size(12).shaping(iced::widget::text::Shaping::Advanced))
                            .on_press(Message::AddTracksToPlaylist(pl.clone(), artist_tracks))
                            .style(item_style).padding([4, 8]).width(Length::Fill)
                    );
                }
                let create: Element<Message> = button(text("+ Create playlist").size(12))
                    .on_press(Message::CreatePlaylistFromContext(name.clone(), true))
                    .style(accent_item_style).padding([4, 8]).width(Length::Fill).into();
                (format!("Artist: {name}"), Some(hide_btn), create)
            }
            ContextMenuTarget::Album(name) => {
                let hide_btn: Element<Message> = button(text("Hide from UI").size(13))
                    .on_press(Message::HideAlbumOrArtist(name.clone(), false))
                    .style(item_style).padding([4, 8]).width(Length::Fill).into();
                for pl in &custom_playlists {
                    let album_tracks: Vec<_> = self.all_tracks.iter()
                        .filter(|t| t.album == *name).cloned().collect();
                    playlist_section = playlist_section.push(
                        button(text(format!("  + {pl}")).size(12).shaping(iced::widget::text::Shaping::Advanced))
                            .on_press(Message::AddTracksToPlaylist(pl.clone(), album_tracks))
                            .style(item_style).padding([4, 8]).width(Length::Fill)
                    );
                }
                let create: Element<Message> = button(text("+ Create playlist").size(12))
                    .on_press(Message::CreatePlaylistFromContext(name.clone(), false))
                    .style(accent_item_style).padding([4, 8]).width(Length::Fill).into();
                (format!("Album: {name}"), Some(hide_btn), create)
            }
            ContextMenuTarget::Track(track) => {
                let like_label = if track.liked { "Unlike this song" } else { "Like this song" };
                let extra: Element<Message> = column![
                    button(text(like_label).size(12))
                        .on_press(Message::ToggleLikeTrack(track.clone()))
                        .style(item_style).padding([4, 8]).width(Length::Fill),
                    button(text("Edit ID3 tag").size(12))
                        .on_press(Message::OpenTagEditor(vec![track.clone()]))
                        .style(item_style).padding([4, 8]).width(Length::Fill),
                    button(text("Open file folder").size(12))
                        .on_press(Message::OpenLocalFolder(track.path.clone()))
                        .style(item_style).padding([4, 8]).width(Length::Fill),
                ].spacing(4).into();
                for pl in &custom_playlists {
                    playlist_section = playlist_section.push(
                        button(text(format!("  + {pl}")).size(12).shaping(iced::widget::text::Shaping::Advanced))
                            .on_press(Message::AddTracksToPlaylist(pl.clone(), vec![track.clone()]))
                            .style(item_style).padding([4, 8]).width(Length::Fill)
                    );
                }
                let create: Element<Message> = button(text("+ Create playlist with song").size(12))
                    .on_press(Message::CreatePlaylistWithTracks(track.title.clone(), vec![track.clone()]))
                    .style(accent_item_style).padding([4, 8]).width(Length::Fill).into();
                (format!("Song: {}", track.title), Some(extra), create)
            }
            ContextMenuTarget::MultipleTracks(tracks) => {
                let extra: Element<Message> = button(text("Edit ID3 tags").size(12))
                    .on_press(Message::OpenTagEditor(tracks.clone()))
                    .style(item_style).padding([4, 8]).width(Length::Fill).into();
                for pl in &custom_playlists {
                    playlist_section = playlist_section.push(
                        button(text(format!("  + {pl}")).size(12).shaping(iced::widget::text::Shaping::Advanced))
                            .on_press(Message::AddTracksToPlaylist(pl.clone(), tracks.clone()))
                            .style(item_style).padding([4, 8]).width(Length::Fill)
                    );
                }
                let create: Element<Message> = button(text("+ Create playlist with selection").size(12))
                    .on_press(Message::CreatePlaylistWithTracks("Selected Tracks".to_string(), tracks.clone()))
                    .style(accent_item_style).padding([4, 8]).width(Length::Fill).into();
                (format!("{} Songs", tracks.len()), Some(extra), create)
            }
            ContextMenuTarget::Header(col) => {
                let active_cols = crate::persist::get(|db| db.table_columns.clone());
                let mut cols_col = column![
                    text("Show / Hide:").size(11).color(theme::subtext()).font(crate::ui::icons::UI_FONT_BOLD),
                    Space::with_height(4),
                ].spacing(4);
                for &c in crate::persist::TableColumn::all() {
                    let visible = active_cols.contains(&c);
                    cols_col = cols_col.push(
                        button(row![
                            text(if visible { "" } else { "" })
                                .font(crate::ui::icons::NERD_FONT_MONO)
                                .color(if visible { theme::accent() } else { theme::overlay0() })
                                .size(28),
                            text(c.label()).size(13).color(theme::text()),
                        ].spacing(8))
                        .on_press(Message::ToggleColumnVisibility(c))
                        .style(item_style).padding([4, 8]).width(Length::Fill)
                    );
                }
                let header_extra: Element<Message> = column![
                    text(format!("Column: {}", col.label())).size(11).color(theme::subtext()).font(crate::ui::icons::UI_FONT_BOLD),
                    Space::with_height(4),
                    button(text("<- Move Left").size(12)).on_press(Message::MoveColumnLeft(*col)).style(item_style).padding([4, 8]).width(Length::Fill),
                    button(text("-> Move Right").size(12)).on_press(Message::MoveColumnRight(*col)).style(item_style).padding([4, 8]).width(Length::Fill),
                    Space::with_height(8),
                    cols_col,
                ].spacing(4).into();
                let dummy: Element<Message> = Space::with_height(0).into();
                ("Table Columns".to_string(), Some(header_extra), dummy)
            }
        };

        playlist_section = playlist_section.push(Space::with_height(4)).push(create_btn);

        let mut menu_col = column![
            row![
                text(title).size(13).font(crate::ui::icons::UI_FONT_BOLD).color(theme::accent()).shaping(iced::widget::text::Shaping::Advanced),
                Space::with_width(Length::Fill),
                button(text(crate::ui::icons::ICON_CLOSE).font(crate::ui::icons::NERD_FONT_MONO).color(theme::red()).size(26))
                    .on_press(Message::ToggleContextMenu(None)).style(iced::widget::button::text),
            ].align_y(Alignment::Center),
            Space::with_height(8),
        ];

        if let Some(extra) = extra_btns {
            menu_col = menu_col.push(extra).push(Space::with_height(6));
        }

        let menu_card = container(
            menu_col.push(playlist_section).spacing(4).padding(16)
        )
        .width(260)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(theme::mantle())),
            border: iced::Border { color: theme::accent(), width: 1.0, radius: crate::ui::theme::corner_radius().into() },
            shadow: iced::Shadow {
                color: theme::base(),
                offset: iced::Vector { x: 0.0, y: 4.0 },
                blur_radius: 8.0,
            },
            ..Default::default()
        });

        container(menu_card)
            .width(Length::Fill).height(Length::Fill)
            .center_x(Length::Fill).center_y(Length::Fill)
            .style(|_| iced::widget::container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5))),
                ..Default::default()
            })
            .into()
    }

    // ── Subscription ──────────────────────────────────────────────────────────

    fn subscription(&self) -> Subscription<Message> {
        let base = Subscription::batch([
            Subscription::run_with_id("audio-events", channel_stream(self.audio_events.clone(), Message::Audio)),
            Subscription::run_with_id("mpris-cmds", channel_stream(self.mpris_cmds.clone(), Message::Mpris)),
            Subscription::run_with_id("tray-cmds", channel_stream(self.tray_cmds.clone(), Message::Tray)),
            Subscription::run_with_id("spectrum", channel_stream(self.spectrum_rx.clone(), Message::SpectrumData)),
            iced::time::every(Duration::from_secs(3)).map(|_| Message::CheckTheme),
            iced::keyboard::on_key_press(|key, _mods| Some(Message::KeyPressed(key))),
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(mods)) => {
                    Some(Message::ModifiersChanged(mods))
                }
                iced::Event::Window(iced::window::Event::Resized(size)) => {
                    Some(Message::WindowResized(size.width as f32, size.height as f32))
                }
                _ => None,
            }),
        ]);

        let mut subs = vec![base];

        if self.yt_dialog.is_some() {
            // The search box keeps Esc for itself (it only drops the focus), so
            // the key handler never saw the first Esc and the dialog stayed open.
            subs.push(iced::event::listen_with(|event, status, _| {
                use iced::keyboard::{key::Named, Event, Key};
                match (event, status) {
                    (
                        iced::Event::Keyboard(Event::KeyPressed { key: Key::Named(Named::Escape), .. }),
                        iced::event::Status::Captured,
                    ) => Some(Message::CloseYtSearch),
                    _ => None,
                }
            }));
        }

        if self.dragging_sidebar {
            subs.push(iced::event::listen_with(|event, _, _| {
                use iced::mouse;
                match event {
                    iced::Event::Mouse(mouse::Event::CursorMoved { position }) => Some(Message::SidebarDragMove(position.x)),
                    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(Message::SidebarDragEnd),
                    _ => None,
                }
            }));
        }

        if self.dragging_lyrics {
            subs.push(iced::event::listen_with(|event, _, _| {
                use iced::mouse;
                match event {
                    iced::Event::Mouse(mouse::Event::CursorMoved { position }) => Some(Message::LyricsDragMove(position.x)),
                    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(Message::LyricsDragEnd),
                    _ => None,
                }
            }));
        }

        if self.dragging_playlist_split {
            subs.push(iced::event::listen_with(|event, _, _| {
                use iced::mouse;
                match event {
                    iced::Event::Mouse(mouse::Event::CursorMoved { position }) => Some(Message::PlaylistDragMove(position.y)),
                    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(Message::PlaylistDragEnd),
                    _ => None,
                }
            }));
        }

        Subscription::batch(subs)
    }

    // ── Helpers de reprodução ─────────────────────────────────────────────────

    fn play_track_internal(&mut self, track: Track) -> Task<Message> {
        let cover_data = load_cover(&track.path);
        let track = Track { cover_data: cover_data.clone(), ..track };
        self.cover = cover_data.clone().map(iced::widget::image::Handle::from_bytes);
        self.cover_rounded = None;
        let blur = self.cover_task(&track.path, cover_data.clone());
        self.audio.send(AudioCommand::Play(track.path.clone()));
        self.audio.send(AudioCommand::SetVolume(self.volume));
        self.current_track = Some(track.clone());
        self.selected_track = Some(track.clone());
        self.lyrics_offset = crate::persist::lyrics_offset(&track.path);
        self.playback_state = PlaybackState::Playing;
        self.position = Duration::ZERO;
        self.duration = Duration::ZERO;
        self.notify_mpris_track(PlaybackStatus::Playing);
        self.persist_state();

        crate::persist::add_to_recently_played(track.path.clone());

        // Contabiliza a reprodução ao iniciar a faixa.
        let count = crate::persist::increment_play_count(track.path.clone());
        let path = track.path.clone();
        for t in self.all_tracks.iter_mut().filter(|t| t.path == path) { t.play_count = count; }
        for t in self.tracks.iter_mut().filter(|t| t.path == path) { t.play_count = count; }
        if let Some(ref mut ct) = self.current_track { ct.play_count = count; }

        if self.selected_playlist.as_deref() == Some("Recently Played") {
            self.update_filtered_tracks();
        }

        // Cache cover para MPRIS art_url
        if let Some(data) = cover_data {
            let cp = cache_cover_path();
            if let Some(dir) = cp.parent() { std::fs::create_dir_all(dir).ok(); }
            if std::fs::write(&cp, &data).is_ok() {
                let art_url = format!("file://{}", cp.display());
                let t = track.clone();
                self.send_mpris(MprisUpdate::Metadata {
                    title: t.title, artist: t.artist, album: t.album,
                    duration_us: t.duration.as_micros() as i64, art_url: Some(art_url),
                });
            }
        }

        send_track_notification(&track.title, &track.artist);
        let _ = self.tray_song_tx.send(format!("{} — {}", track.title, track.artist));

        let scroll = if let Some(y) = self.calculate_scroll_offset(track.id) {
            self.scroll_tracklist_to(y)
        } else {
            Task::none()
        };
        Task::batch([scroll, blur, self.ensure_lyrics(), self.ensure_lyrics_alt(), self.ensure_furigana()])
    }

    fn advance_track(&mut self, delta: i32) -> Task<Message> {
        if self.queue.is_empty() { return Task::none(); }
        let next_idx = if self.shuffle {
            use rand::Rng;
            let cur = self.current_track.as_ref().and_then(|ct| self.queue.iter().position(|t| t.id == ct.id));
            let len = self.queue.len();
            if len == 1 { 0 } else {
                let mut rng = rand::thread_rng();
                let mut idx = rng.gen_range(0..len);
                if let Some(c) = cur { while idx == c { idx = rng.gen_range(0..len); } }
                idx
            }
        } else {
            let cur = self.current_track.as_ref().and_then(|ct| self.queue.iter().position(|t| t.id == ct.id));
            match cur {
                Some(i) => { let n = i as i32 + delta; if n < 0 { self.queue.len() - 1 } else { n as usize % self.queue.len() } }
                None => 0,
            }
        };
        if let Some(track) = self.queue.get(next_idx).cloned() {
            self.play_track_internal(track)
        } else {
            Task::none()
        }
    }
}

// ── Helpers globais ───────────────────────────────────────────────────────────

fn channel_stream<T>(holder: Shared<T>, map: fn(T) -> Message) -> impl Stream<Item = Message>
where
    T: Send + 'static,
{
    iced::stream::channel(64, move |mut output| async move {
        let Some(mut rx) = holder.lock().unwrap().take() else { return; };
        while let Some(item) = rx.recv().await {
            if output.send(map(item)).await.is_err() { break; }
        }
    })
}

fn cache_cover_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".cache/lavanda/cover.jpg")
}

/// Identifies a set of lyric lines by their text.
fn furigana_key(lines: &[crate::lyrics::LyricLine]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for l in lines {
        l.text.hash(&mut h);
    }
    h.finish()
}

/// The cover as the player shows it: square, 512 px, with anti-aliased
/// rounded corners; `radius` is a fraction of the width.
fn rounded_cover(bytes: &[u8], radius: f32) -> Option<iced::widget::image::Handle> {
    const SIZE: u32 = 512;
    let img = image::load_from_memory(bytes).ok()?;
    let mut px = crate::cover::square_cover(&img, SIZE);
    round_corners(&mut px, radius * SIZE as f32);
    Some(iced::widget::image::Handle::from_rgba(SIZE, SIZE, px.into_raw()))
}

/// Percent-encodes `text` for a URL query (`&`, `#`, `/`, … in names
/// would otherwise cut the query short). Spaces become `+`.
fn url_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A YouTube thumbnail at 192×108 (shown at 96×54, sharp on HiDPI) with
/// rounded corners; `radius` in display pixels.
fn rounded_thumbnail(bytes: &[u8], radius: f32) -> Option<iced::widget::image::Handle> {
    let img = image::load_from_memory(bytes).ok()?;
    let mut px = img.resize_to_fill(192, 108, image::imageops::FilterType::Triangle).to_rgba8();
    round_corners(&mut px, radius * 2.0);
    Some(iced::widget::image::Handle::from_rgba(px.width(), px.height(), px.into_raw()))
}

/// A YouTube cover for the search preview: 640×360 (shown at 320×180) with
/// rounded corners.
fn preview_image(bytes: &[u8]) -> Option<iced::widget::image::Handle> {
    let img = image::load_from_memory(bytes).ok()?;
    let mut px = img.resize_to_fill(640, 360, image::imageops::FilterType::Triangle).to_rgba8();
    round_corners(&mut px, 28.0);
    Some(iced::widget::image::Handle::from_rgba(px.width(), px.height(), px.into_raw()))
}

/// Fades the corners out to a radius of `r` pixels, anti-aliased.
fn round_corners(px: &mut image::RgbaImage, r: f32) {
    let (w, h) = (px.width() as f32, px.height() as f32);
    let r = r.clamp(0.0, w.min(h) / 2.0);
    for (x, y, p) in px.enumerate_pixels_mut() {
        // Distance into a corner's rounding, measured from pixel centres.
        let dx = (r - (x as f32 + 0.5)).max((x as f32 + 0.5) - (w - r)).max(0.0);
        let dy = (r - (y as f32 + 0.5)).max((y as f32 + 0.5) - (h - r)).max(0.0);
        if dx > 0.0 && dy > 0.0 {
            let coverage = (r - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            p[3] = (p[3] as f32 * coverage) as u8;
        }
    }
}

/// A tiny blurred copy of a cover. Stretched over the window it becomes a
/// soft wash of the cover's colours — far cheaper than blurring at full size.
/// Also measures how bright the cover is and picks its main colour.
fn blurred_cover(bytes: &[u8]) -> Option<(iced::widget::image::Handle, CoverTone)> {
    let small = image::load_from_memory(bytes).ok()?.thumbnail(64, 64);
    let blurred = image::imageops::blur(&small, 3.0);
    let tone = CoverTone::measure(&blurred);
    Some((iced::widget::image::Handle::from_rgba(blurred.width(), blurred.height(), blurred.into_raw()), tone))
}

/// How bright a cover is, and its most vivid colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverTone {
    /// Average luminance, 0 (black) – 1 (white).
    pub luminance: f32,
    pub accent: iced::Color,
}

impl CoverTone {
    fn measure(px: &image::RgbaImage) -> Self {
        let colours: Vec<[f32; 3]> = px.pixels().map(|p| [p[0], p[1], p[2]].map(|c| c as f32 / 255.0)).collect();
        let luma = |c: &[f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let luminance = colours.iter().map(luma).sum::<f32>() / colours.len().max(1) as f32;
        // Accent: the average of the most vivid tenth (saturation × value).
        let vivid = |c: &[f32; 3]| {
            let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
            if max == 0.0 { 0.0 } else { (max - min) / max * max }
        };
        let mut ranked = colours.clone();
        ranked.sort_by(|a, b| vivid(b).total_cmp(&vivid(a)));
        if ranked.is_empty() {
            return CoverTone { luminance: 0.0, accent: iced::Color::WHITE };
        }
        let top = &ranked[..(ranked.len() / 10).max(1)];
        let sum = top.iter().fold([0.0; 3], |s, c| [s[0] + c[0], s[1] + c[1], s[2] + c[2]]);
        let n = top.len() as f32;
        CoverTone { luminance, accent: iced::Color::from_rgb(sum[0] / n, sum[1] / n, sum[2] / n) }
    }

    /// Covers bright enough that white text on them is hard to read.
    pub fn is_bright(self) -> bool {
        self.luminance > 0.55
    }

    /// Opacity for the blurred cover behind the (dark) theme: bright covers
    /// are dimmed so the backdrop ends up no brighter than ~35% luminance.
    pub fn backdrop_opacity(self) -> f32 {
        const THEME: f32 = 0.12; // the theme background shows through
        if self.luminance <= 0.35 {
            0.75
        } else {
            ((0.35 - THEME) / (self.luminance - THEME)).clamp(0.2, 0.75)
        }
    }

    /// The cover's colour, made light enough to read on the dimmed backdrop.
    pub fn lyrics_tint(self) -> iced::Color {
        let c = self.accent;
        let (max, min) = (c.r.max(c.g).max(c.b), c.r.min(c.g).min(c.b));
        let lightness = (max + min) / 2.0;
        // Blend towards white until light enough (≥ 0.8), keeping the hue.
        let toward_white = ((0.8 - lightness) / (1.0 - lightness).max(0.01)).clamp(0.0, 1.0);
        let lift = |v: f32| v + (1.0 - v) * toward_white;
        iced::Color::from_rgb(lift(c.r), lift(c.g), lift(c.b))
    }
}

fn send_track_notification(title: &str, artist: &str) {
    spawn_detached(std::process::Command::new("notify-send").args([
        APP_NAME,
        &format!("{title} — {artist}"),
        "--icon=tort-music-player",
        "--expire-time=3000",
        "--urgency=low",
    ]));
}

/// Starts a helper program without waiting for it, but reaps it once it
/// exits; a child nobody waits for lingers as a zombie process (one per
/// track change, for notify-send). Returns whether it started.
fn spawn_detached(cmd: &mut std::process::Command) -> bool {
    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
            true
        }
        Err(_) => false,
    }
}

fn sidebar_width_path() -> PathBuf {
    let xdg = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        format!("{}/.config", std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
    });
    PathBuf::from(xdg).join("lavanda/sidebar_width")
}

fn load_sidebar_width() -> f32 {
    std::fs::read_to_string(sidebar_width_path()).ok()
        .and_then(|s| s.trim().parse().ok()).unwrap_or(200.0)
}

fn save_sidebar_width(width: f32) {
    let path = sidebar_width_path();
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).ok(); }
    std::fs::write(path, width.to_string()).ok();
}

fn build_iced_theme() -> Theme {
    Theme::custom("Omarchy".into(), iced::theme::Palette {
        background: theme::base(), text: theme::text(),
        primary: theme::accent(), success: theme::green(), danger: theme::red(),
    })
}

pub fn run() -> iced::Result {
    iced::application(APP_NAME, AppState::update, AppState::view)
        .subscription(AppState::subscription)
        .default_font(iced::Font {
            family: iced::font::Family::Name(crate::ui::icons::TEXT_FONT_FAMILY),
            weight: iced::font::Weight::Normal,
            stretch: iced::font::Stretch::Normal,
            style: iced::font::Style::Normal,
        })
        .theme(|state: &AppState| state.iced_theme.clone())
        .window(iced::window::Settings {
            size: iced::Size::new(960.0, 640.0),
            min_size: Some(iced::Size::new(700.0, 480.0)),
            // Shown on X11; Wayland compositors take the icon from the
            // .desktop file matching the application id.
            icon: iced::window::icon::from_file_data(TUX_PNG, None).ok(),
            platform_specific: iced::window::settings::PlatformSpecific {
                application_id: "lavanda".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(AppState::new)
}

#[cfg(test)]
mod tests {
    use super::CoverTone;
    use iced::Color;

    #[test]
    fn url_encoding_keeps_the_whole_query() {
        assert_eq!(super::url_encode("AC/DC & Friends #1"), "AC%2FDC+%26+Friends+%231");
        assert_eq!(super::url_encode("夜"), "%E5%A4%9C");
    }

    #[test]
    fn bright_covers_are_dimmed_and_tinted() {
        let white = CoverTone { luminance: 0.95, accent: Color::from_rgb(0.9, 0.9, 0.9) };
        let pink = CoverTone { luminance: 0.8, accent: Color::from_rgb(0.9, 0.2, 0.5) };
        let dark = CoverTone { luminance: 0.2, accent: Color::from_rgb(0.1, 0.2, 0.6) };
        assert!(white.is_bright() && pink.is_bright() && !dark.is_bright());
        // Dark covers keep today's look; bright ones are dimmed, brighter more.
        assert_eq!(dark.backdrop_opacity(), 0.75);
        assert!(white.backdrop_opacity() < pink.backdrop_opacity());
        assert!(white.backdrop_opacity() <= 0.3);
        // The tint stays pink but becomes light enough to read.
        let t = pink.lyrics_tint();
        assert!((t.r.max(t.g).max(t.b) + t.r.min(t.g).min(t.b)) / 2.0 >= 0.79);
        assert!(t.r > t.g && t.b > t.g);
    }
}
