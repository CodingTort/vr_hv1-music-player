use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Config {
    pub music_dir: String, // String para suportar "~" antes de expandir
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: bool,
    pub language: String,
    pub seek_step: u64,
    pub volume_step: f32,
    // YouTube search / download (via yt-dlp)
    pub yt_dlp_path: String,
    pub yt_dlp_format: String,
    pub yt_dlp_quality: String,
    pub yt_dlp_output: String,
    pub yt_search_results: usize,
    // Theme / UI
    pub theme_mode: String, // "auto" | "manual"
    pub theme_preset: String,
    pub use_glassmorphism: bool,
    pub corner_radius: f32,
    pub background_kind: String,
    pub background_color: String,
    pub background_gradient_from: String,
    pub background_gradient_to: String,
    pub background_gradient_angle: f32,
    pub background_image: String, // empty = none
}

impl Default for Config {
    fn default() -> Self {
        Config {
            music_dir: "~/Music".into(),
            volume: 0.8,
            shuffle: false,
            repeat: false,
            language: "auto".into(),
            seek_step: 5,
            volume_step: 0.05,
            yt_dlp_path: "yt-dlp".into(),
            yt_dlp_format: "mp3".into(),
            yt_dlp_quality: "128K".into(),
            yt_dlp_output: "%(title)s.%(ext)s".into(),
            yt_search_results: 15,
            theme_mode: "auto".into(),
            theme_preset: "lavender".into(),
            use_glassmorphism: true,
            corner_radius: 10.0,
            background_kind: "none".into(),
            background_color: "#11111b".into(),
            background_gradient_from: "#1e1b2e".into(),
            background_gradient_to: "#0f0e17".into(),
            background_gradient_angle: 135.0,
            background_image: String::new(),
        }
    }
}

impl Config {
    /// Retorna `music_dir` com `~` expandido para `$HOME`.
    pub fn music_path(&self) -> PathBuf {
        expand_tilde(&self.music_dir)
    }
}

// ── Inicialização ─────────────────────────────────────────────────────────────

pub fn load() {
    CONFIG.get_or_init(|| read_or_default());
}

pub fn get() -> &'static Config {
    CONFIG.get_or_init(|| read_or_default())
}

fn read_or_default() -> Config {
    let path = config_path();

    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        std::fs::write(&path, DEFAULT_CONFIG).ok();
        eprintln!("lavanda: configuração criada em {}", path.display());
        return Config::default();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("lavanda: erro ao ler config: {e}");
            return Config::default();
        }
    };

    match toml::from_str::<Config>(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lavanda: config inválida ({e}), usando padrões");
            Config::default()
        }
    }
}

pub fn config_path() -> PathBuf {
    expand_tilde("~/.config/lavanda/config.toml")
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(&path[2..])
    } else {
        PathBuf::from(path)
    }
}

// ── Config padrão gerada na primeira execução ─────────────────────────────────

const DEFAULT_CONFIG: &str = "# lavanda - configuration file\n# ~/.config/lavanda/config.toml\n\nmusic_dir = \"~/Music\"\nvolume = 0.8\nshuffle = false\nrepeat = false\nlanguage = \"auto\"\nseek_step = 5\nvolume_step = 0.05\nyt_dlp_path = \"yt-dlp\"\nyt_dlp_format = \"mp3\"\nyt_dlp_quality = \"128K\"\nyt_dlp_output = \"%(title)s.%(ext)s\"\nyt_search_results = 15\ntheme_mode = \"auto\"\ntheme_preset = \"lavender\"\nuse_glassmorphism = true\ncorner_radius = 10.0\nbackground_kind = \"none\"\nbackground_color = \"#11111b\"\nbackground_gradient_from = \"#1e1b2e\"\nbackground_gradient_to = \"#0f0e17\"\nbackground_gradient_angle = 135.0\nbackground_image = \"\"\n";
