use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use iced::widget::{container, text_input};
use iced::{Border, Color};

// ── Raio de canto (configurável) ─────────────────────────────────────────────

static CORNER_RADIUS: AtomicU32 = AtomicU32::new(0);

/// Ajusta o raio de canto usado por botões, campos e cartões.
pub fn set_corner_radius(radius: f32) {
    CORNER_RADIUS.store(radius.clamp(0.0, 32.0).to_bits(), Ordering::Relaxed);
}

pub fn corner_radius() -> f32 {
    f32::from_bits(CORNER_RADIUS.load(Ordering::Relaxed))
}

// ── Paleta ───────────────────────────────────────────────────────────────────

static PALETTE: OnceLock<Mutex<Palette>> = OnceLock::new();

struct Palette {
    base: Color,
    mantle: Color,
    surface0: Color,
    overlay0: Color,
    text: Color,
    subtext: Color,
    accent: Color,
    green: Color,
    red: Color,
}

impl Palette {
    fn default_lavender() -> Self {
        Palette {
            base: hex(0x11, 0x11, 0x1b),
            mantle: hex(0x18, 0x18, 0x25),
            surface0: hex(0x31, 0x32, 0x44),
            overlay0: hex(0x6c, 0x70, 0x86),
            text: hex(0xcd, 0xd6, 0xf4),
            subtext: hex(0xa6, 0xad, 0xc8),
            accent: hex(0xcb, 0xa6, 0xf7),
            green: hex(0xa6, 0xe3, 0xa1),
            red: hex(0xf3, 0x8b, 0xa8),
        }
    }

    fn from_preset(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "vampire" | "dracula" => Palette {
                base: hex(0x14, 0x0b, 0x0b),
                mantle: hex(0x1a, 0x10, 0x10),
                surface0: hex(0x2d, 0x1a, 0x1a),
                overlay0: hex(0x8b, 0x5a, 0x5a),
                text: hex(0xf8, 0xe8, 0xe8),
                subtext: hex(0xd4, 0xc4, 0xc4),
                accent: hex(0xff, 0x37, 0x59),
                green: hex(0x50, 0xfa, 0x7b),
                red: hex(0xff, 0x55, 0x55),
            },
            "cotton candy" | "cottoncandy" => Palette {
                base: hex(0x12, 0x0f, 0x16),
                mantle: hex(0x18, 0x14, 0x1d),
                surface0: hex(0x2a, 0x24, 0x35),
                overlay0: hex(0x7c, 0x6b, 0x9a),
                text: hex(0xf7, 0xe6, 0xff),
                subtext: hex(0xe6, 0xd1, 0xff),
                accent: hex(0xff, 0x9c, 0xe8),
                green: hex(0xb4, 0xff, 0xa1),
                red: hex(0xff, 0x88, 0xa8),
            },
            "catppuccin mocha" | "mocha" => Palette {
                base: hex(0x1e, 0x1e, 0x2e),
                mantle: hex(0x18, 0x18, 0x25),
                surface0: hex(0x31, 0x32, 0x44),
                overlay0: hex(0x6c, 0x70, 0x86),
                text: hex(0xcd, 0xd6, 0xf4),
                subtext: hex(0xa6, 0xad, 0xc8),
                accent: hex(0xcb, 0xa6, 0xf7),
                green: hex(0xa6, 0xe3, 0xa1),
                red: hex(0xf3, 0x8b, 0xa8),
            },
            "tokyo night" | "tokyo" => Palette {
                base: hex(0x15, 0x16, 0x25),
                mantle: hex(0x1a, 0x1b, 0x26),
                surface0: hex(0x28, 0x29, 0x36),
                overlay0: hex(0x78, 0x7c, 0x99),
                text: hex(0xa9, 0xb1, 0xd6),
                subtext: hex(0x9a, 0xa0, 0xc2),
                accent: hex(0x7a, 0xa2, 0xf7),
                green: hex(0x9e, 0xce, 0x6a),
                red: hex(0xf7, 0x76, 0x8e),
            },
            "nord" => Palette {
                base: hex(0x2e, 0x34, 0x40),
                mantle: hex(0x29, 0x2e, 0x39),
                surface0: hex(0x3b, 0x42, 0x52),
                overlay0: hex(0x61, 0x6e, 0x88),
                text: hex(0xec, 0xef, 0xf4),
                subtext: hex(0xd8, 0xde, 0xe9),
                accent: hex(0x88, 0xc0, 0xd0),
                green: hex(0xa3, 0xbe, 0x8c),
                red: hex(0xbf, 0x61, 0x6a),
            },
            "gruvbox" => Palette {
                base: hex(0x28, 0x28, 0x28),
                mantle: hex(0x1d, 0x20, 0x21),
                surface0: hex(0x3c, 0x38, 0x36),
                overlay0: hex(0x92, 0x83, 0x74),
                text: hex(0xeb, 0xdb, 0xb2),
                subtext: hex(0xd5, 0xc4, 0xa1),
                accent: hex(0xfa, 0xbd, 0x2f),
                green: hex(0xb8, 0xbb, 0x26),
                red: hex(0xfb, 0x49, 0x34),
            },
            "everforest" => Palette {
                base: hex(0x2b, 0x33, 0x36),
                mantle: hex(0x23, 0x2a, 0x2e),
                surface0: hex(0x3d, 0x48, 0x4d),
                overlay0: hex(0x85, 0x92, 0x89),
                text: hex(0xd3, 0xc6, 0xaa),
                subtext: hex(0xb9, 0xac, 0x93),
                accent: hex(0xa7, 0xc0, 0x80),
                green: hex(0xa7, 0xc0, 0x80),
                red: hex(0xe6, 0x7e, 0x80),
            },
            "rose pine" | "rosepine" => Palette {
                base: hex(0x19, 0x17, 0x24),
                mantle: hex(0x1f, 0x1d, 0x2b),
                surface0: hex(0x26, 0x23, 0x3a),
                overlay0: hex(0x6e, 0x6a, 0x86),
                text: hex(0xe0, 0xde, 0xf4),
                subtext: hex(0x90, 0x8c, 0xaa),
                accent: hex(0xeb, 0xbc, 0xba),
                green: hex(0x9c, 0xcf, 0xd8),
                red: hex(0xeb, 0x6f, 0x92),
            },
            _ => Self::default_lavender(),
        }
    }
}

fn palette_mutex() -> &'static Mutex<Palette> {
    PALETTE.get_or_init(|| {
        let p = try_load_omarchy_theme().unwrap_or_else(|| {
            eprintln!("lavanda: tema Omarchy não encontrado, usando lavender padrão");
            Palette::default_lavender()
        });
        Mutex::new(p)
    })
}

pub fn set_manual_preset(name: &str) {
    let p = Palette::from_preset(name);
    *palette_mutex().lock().unwrap() = p;
}

pub const AVAILABLE_PRESETS: &[&str] = &[
    "lavender",
    "vampire",
    "cotton candy",
    "catppuccin mocha",
    "tokyo night",
    "nord",
    "gruvbox",
    "everforest",
    "rose pine",
];

pub fn get_available_presets() -> &'static [&'static str] {
    AVAILABLE_PRESETS
}

/// Inicializa a paleta na primeira execução.
#[allow(dead_code)]
pub fn load_system_theme() {
    let _ = palette_mutex();
}

/// Relê o tema do Omarchy em disco e atualiza a paleta sem reiniciar.
pub fn reload_system_theme() {
    if let Some(new) = try_load_omarchy_theme() {
        *palette_mutex().lock().unwrap() = new;
    }
}

pub fn reload_with_mode(mode: &str, preset: &str) {
    match mode.to_lowercase().as_str() {
        "manual" => {
            set_manual_preset(preset);
        }
        _ => {
            if let Some(new) = try_load_omarchy_theme() {
                *palette_mutex().lock().unwrap() = new;
            } else {
                set_manual_preset(preset);
            }
        }
    }
}

/// Retorna o nome do tema atualmente configurado no Omarchy (para detecção de mudanças).
pub fn read_current_theme_name() -> String {
    let home = match home_dir() {
        Some(h) => h,
        None => return String::new(),
    };
    std::fs::read_to_string(home.join(".config/omarchy/current/theme.name"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn try_load_omarchy_theme() -> Option<Palette> {
    let home = home_dir()?;

    let theme_name = std::fs::read_to_string(home.join(".config/omarchy/current/theme.name"))
        .ok()?
        .trim()
        .to_string();

    let user_path = home.join(format!(".config/omarchy/themes/{}/colors.toml", theme_name));
    let system_path = home.join(format!(
        ".local/share/omarchy/themes/{}/colors.toml",
        theme_name
    ));

    let content = std::fs::read_to_string(&user_path)
        .or_else(|_| std::fs::read_to_string(&system_path))
        .ok()?;

    eprintln!("lavanda: carregando tema \"{}\"", theme_name);
    parse_colors_toml(&content)
}

fn parse_colors_toml(content: &str) -> Option<Palette> {
    let mut map: HashMap<String, Color> = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        let val = val.trim();

        let hex6 = if let Some(pos) = val.find('#') {
            let after = &val[pos + 1..];
            let end = after
                .find(|c: char| !c.is_ascii_hexdigit())
                .unwrap_or(after.len());
            after[..end.min(6)].to_string()
        } else {
            val.trim_matches('"').chars().take(6).collect()
        };

        if hex6.len() == 6 {
            if let Some(c) = parse_hex_str(&hex6) {
                map.insert(key, c);
            }
        }
    }

    let bg = *map.get("background")?;
    let fg = *map.get("foreground")?;
    let accent = *map.get("accent")?;

    let c8 = map
        .get("color8")
        .copied()
        .unwrap_or_else(|| lerp_color(bg, fg, 0.3));

    let is_dark = luminance(bg) < 0.5;
    let (mantle, surface0) = if is_dark {
        (lerp_color(bg, c8, 0.10), lerp_color(bg, c8, 0.40))
    } else {
        (lerp_color(bg, fg, 0.05), lerp_color(bg, fg, 0.18))
    };

    Some(Palette {
        base: bg,
        mantle,
        surface0,
        overlay0: c8,
        text: fg,
        subtext: map
            .get("color15")
            .copied()
            .unwrap_or_else(|| lerp_color(fg, c8, 0.3)),
        accent,
        red: map
            .get("color1")
            .copied()
            .unwrap_or_else(|| hex(0xf3, 0x8b, 0xa8)),
        green: map
            .get("color2")
            .copied()
            .unwrap_or_else(|| hex(0xa6, 0xe3, 0xa1)),
    })
}

fn parse_hex_str(s: &str) -> Option<Color> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let hex_stripped = s.strip_prefix('#').unwrap_or(s);
    let bytes = hex_stripped.as_bytes();
    match bytes.len() {
        6 => {
            let r = u8::from_str_radix(&hex_stripped[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex_stripped[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex_stripped[4..6], 16).ok()?;
            Some(hex(r, g, b))
        }
        3 => {
            let r = u8::from_str_radix(&hex_stripped[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex_stripped[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex_stripped[2..3].repeat(2), 16).ok()?;
            Some(hex(r, g, b))
        }
        _ => None,
    }
}

pub fn parse_color_str(s: &str) -> Option<Color> {
    parse_hex_str(s)
}

fn luminance(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

// ── Acessores de cor ─────────────────────────────────────────────────────────

macro_rules! color_fn {
    ($name:ident, $field:ident) => {
        pub fn $name() -> Color {
            palette_mutex().lock().unwrap().$field
        }
    };
}

color_fn!(base, base);
color_fn!(mantle, mantle);
color_fn!(surface0, surface0);
color_fn!(overlay0, overlay0);
color_fn!(text, text);
color_fn!(subtext, subtext);
color_fn!(accent, accent);
color_fn!(green, green);
color_fn!(red, red);

// ── Utilitários ──────────────────────────────────────────────────────────────

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

// ── Estilos de Container ──────────────────────────────────────────────────────

pub fn card(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(mantle())),
        border: Border {
            color: surface0(),
            width: 1.0,
            radius: corner_radius().into(),
        },
        ..Default::default()
    }
}

pub fn header(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(mantle())),
        ..Default::default()
    }
}

pub fn header_glass(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(with_alpha(base(), 0.6))),
        border: Border { radius: corner_radius().into(), width: 0.0, color: Color::TRANSPARENT },
        ..Default::default()
    }
}

pub fn sidebar(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(mantle())),
        border: Border {
            color: surface0(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn sidebar_glass(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(with_alpha(mantle(), 0.6))),
        border: Border {
            color: with_alpha(surface0(), 0.7),
            width: 1.0,
            radius: corner_radius().into(),
        },
        ..Default::default()
    }
}

pub fn selected_row(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(with_alpha(accent(), 0.15))),
        border: Border {
            color: with_alpha(accent(), 0.4),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn player_panel(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(mantle())),
        border: Border {
            color: surface0(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn player_panel_glass(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(with_alpha(mantle(), 0.6))),
        border: Border {
            color: with_alpha(surface0(), 0.7),
            width: 1.0,
            radius: corner_radius().into(),
        },
        ..Default::default()
    }
}

pub fn album_header(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(with_alpha(surface0(), 0.5))),
        border: Border {
            color: with_alpha(accent(), 0.2),
            width: 0.0,
            radius: corner_radius().into(),
        },
        ..Default::default()
    }
}

pub fn primary_button(_: &iced::Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let is_hovered = matches!(
        status,
        iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
    );
    iced::widget::button::Style {
        background: Some(iced::Background::Color(if is_hovered {
            lerp_color(accent(), text(), 0.15)
        } else {
            accent()
        })),
        text_color: base(),
        border: Border { radius: corner_radius().into(), ..Default::default() },
        ..Default::default()
    }
}

pub fn secondary_button(_: &iced::Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let is_hovered = matches!(
        status,
        iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
    );
    iced::widget::button::Style {
        background: Some(iced::Background::Color(if is_hovered { surface0() } else { mantle() })),
        text_color: text(),
        border: Border { radius: corner_radius().into(), width: 1.0, color: surface0() },
        ..Default::default()
    }
}

pub fn pick_list(_: &iced::Theme, _: iced::widget::pick_list::Status) -> iced::widget::pick_list::Style {
    iced::widget::pick_list::Style {
        background: iced::Background::Color(mantle()),
        border: Border { radius: corner_radius().into(), width: 1.0, color: surface0() },
        text_color: text(),
        placeholder_color: overlay0(),
        handle_color: text(),
    }
}

pub fn dialog_input(_: &iced::Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused => accent(),
        _ => surface0(),
    };
    text_input::Style {
        background: iced::Background::Color(with_alpha(surface0(), 0.5)),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: corner_radius().into(),
        },
        icon: overlay0(),
        placeholder: overlay0(),
        value: text(),
        selection: with_alpha(accent(), 0.3),
    }
}

pub fn base_style_glass(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(with_alpha(base(), 0.4))),
        ..Default::default()
    }
}

pub fn base_style_solid(c: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(c)),
        ..Default::default()
    }
}

pub fn base_style_gradient(from: Color, to: Color, angle_deg: f32) -> impl Fn(&iced::Theme) -> container::Style {
    use iced::gradient::{Gradient, Linear};
    let angle = iced::Radians(angle_deg.to_radians());
    let gradient = Linear::new(angle)
        .add_stop(0.0, from)
        .add_stop(1.0, to);
    move |_: &iced::Theme| container::Style {
        background: Some(iced::Background::Gradient(Gradient::Linear(gradient))),
        ..Default::default()
    }
}

/// Frame around the album cover: a hairline border just outside the image
/// and a soft shadow underneath, rounded like the (pre-rounded) cover.
pub fn cover_frame(radius: f32) -> impl Fn(&iced::Theme) -> container::Style {
    move |_: &iced::Theme| container::Style {
        border: Border { color: with_alpha(text(), 0.18), width: 1.5, radius: radius.into() },
        shadow: iced::Shadow {
            color: Color { a: 0.35, ..Color::BLACK },
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 16.0,
        },
        ..Default::default()
    }
}
