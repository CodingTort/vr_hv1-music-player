use iced::{font::Weight, Font};

/// The text font (see README-fork.md for installing it).
pub const TEXT_FONT_FAMILY: &str = "UDEV Gothic NF";

/// Fonte Nerd Font carregada do sistema.
/// Qualquer Nerd Font instalada (JetBrainsMono, FiraCode, Hack, etc.) funciona.
pub const NERD_FONT: Font = Font {
    family: iced::font::Family::Name("JetBrainsMono Nerd Font"),
    weight: Weight::Normal,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

pub const NERD_FONT_MONO: Font = Font {
    family: iced::font::Family::Name("JetBrainsMono Nerd Font Mono"),
    ..NERD_FONT
};

/// Fonte base da UI. UDEV Gothic NF is JetBrains Mono with matching
/// Japanese (BIZ UD Gothic) in one family, so mixed text no longer falls back
/// to a different-looking font for kana and kanji.
pub const UI_FONT: Font = Font {
    family: iced::font::Family::Name(TEXT_FONT_FAMILY),
    weight: Weight::Normal,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

pub const UI_FONT_BOLD: Font = Font {
    weight: Weight::Bold,
    ..UI_FONT
};

// Codepoints Font Awesome (Nerd Fonts tier 1 — universais em qualquer Nerd Font)
pub const ICON_PLAY: &str = "\u{f04b}"; //
pub const ICON_PAUSE: &str = "\u{f04c}"; //
pub const ICON_PREV: &str = "\u{f048}"; //
pub const ICON_NEXT: &str = "\u{f051}"; //
pub const ICON_SHUFFLE: &str = "\u{f074}"; //
pub const ICON_REPEAT: &str = "\u{f021}"; //
pub const ICON_VOL_UP: &str = "\u{f028}"; //
pub const ICON_MUSIC: &str = "\u{f001}"; //
pub const ICON_SEARCH: &str = "\u{f002}"; //
pub const ICON_HEART: &str = "\u{f004}"; //
pub const ICON_PLUS: &str = "\u{f067}"; //
pub const ICON_TRASH: &str = "\u{f1f8}"; //
pub const ICON_EDIT: &str = "\u{f044}"; //
pub const ICON_PODIUM: &str = "\u{f0d25}"; //
pub const ICON_CLOCK: &str = "\u{f017}"; //
pub const ICON_LIST: &str = "\u{f0ca}"; //
pub const ICON_CLOSE: &str = "\u{f00d}"; //
pub const ICON_SETTINGS: &str = "\u{f013}"; //
pub const ICON_LYRICS: &str = "\u{f130}"; //  (microphone)
pub const ICON_EXPORT: &str = "\u{f093}"; //  (upload)
pub const ICON_IMPORT: &str = "\u{f019}"; //  (download)
pub const ICON_YOUTUBE: &str = "\u{f05c3}"; // 󰗃 (YouTube, Material Design)
