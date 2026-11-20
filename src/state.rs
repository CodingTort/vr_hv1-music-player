use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct SavedState {
    #[serde(serialize_with = "crate::config::tidy_opt_f32", skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    pub show_lyrics: Option<bool>,
    #[serde(serialize_with = "crate::config::tidy_opt_f32", skip_serializing_if = "Option::is_none")]
    pub lyrics_width: Option<f32>,
    pub lyrics_romaji: Option<bool>,
}

fn state_path() -> PathBuf {
    let xdg = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        format!(
            "{}/.config",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
        )
    });
    PathBuf::from(xdg).join("lavanda").join("state.toml")
}

pub fn load() -> SavedState {
    let Ok(content) = std::fs::read_to_string(state_path()) else {
        return SavedState::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

pub fn save(s: &SavedState) {
    let path = state_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    if let Ok(content) = toml::to_string(s) {
        std::fs::write(path, content).ok();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn saved_state_has_no_float_noise() {
        let s = super::SavedState { volume: Some(0.01), show_lyrics: Some(true), lyrics_width: None, lyrics_romaji: None };
        let text = toml::to_string(&s).unwrap();
        assert_eq!(text, "volume = 0.01\nshow_lyrics = true\n");
    }
}
