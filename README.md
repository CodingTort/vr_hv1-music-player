<div align="center">

<img src="assets/tux.png" width="96" alt="">

# vr_hv1's music client

**A fast, native Linux music player with synced lyrics, furigana, romaji and YouTube downloads.**
It shows up on your desktop as *Tort's Music Player*.

[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE)
[![Rust 1.89+](https://img.shields.io/badge/rust-1.89%2B-orange.svg)](https://www.rust-lang.org)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux-blue.svg)](#requirements)
[![Wayland & X11](https://img.shields.io/badge/Wayland%20%26%20X11-native-cyan.svg)](#requirements)

![Main window: library, player with visualiser, and the synced lyrics panel with furigana](docs/screenshots/main.png)

</div>

An enhanced fork of [lavanda](https://github.com/sheep-farm/lavanda), built for people who want more
control over how their player looks, who keep their music as local files, and who listen to a lot of
Japanese music.

## Contents

- [Highlights](#highlights)
- [Quick start (Arch Linux)](#quick-start-arch-linux)
- [Requirements](#requirements)
- [Installation](#installation)
- [Using it](#using-it)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Configuration](#configuration)
- [Desktop integration](#desktop-integration)
- [Privacy](#privacy)
- [Project layout](#project-layout)
- [Contributing](#contributing)
- [Credits](#credits)
- [License](#license)

## Highlights

**Lyrics**
- **Synced lyrics panel**: the line being sung lights up and the text glides along with the song.
  Click a line to jump there, scroll to look around, drag the panel edge to resize it.
- Lyrics come from a `.lrc` file next to the song, the file's embedded lyrics, or [LRCLIB](https://lrclib.net).
  Lookups understand YouTube-style titles (`【Artist】Song (Official Video)`) and covers (`歌ってみた`, `Cover`).
- **Per-song timing fix**: if lyrics run early or late, right-click the line being sung, or nudge with `[` / `]`.
  The adjustment is remembered.
- Uploads with a translation woven in are skipped when a clean version exists.

**Japanese**
- **Furigana** above every kanji, using MeCab's grammar-aware readings (kakasi as a fallback).
- **Japanese ⇄ Romaji switch** in the lyrics panel. Both versions share the same timing, so switching
  never moves the highlighted line.

**Look and feel**
- Rounded, evenly spaced cards with an adjustable corner radius, plus optional glass (translucent) panels.
- **Blurred album-cover background** that dims bright covers and tints the lyrics to stay readable.
- Large framed cover art. YouTube thumbnails are shown whole instead of cropped and zoomed in.
- A sensitive, mirrored spectrum visualiser that follows the music's loudness.
- Nine built-in themes, or follow your [Omarchy](https://omarchy.org) theme live.
- Focus layout (`t`): big cover, lyrics alongside, library hidden.

![Focus layout with the cover, controls and lyrics side by side](docs/screenshots/focus.png)

**Library and downloads**
- **YouTube search and download** (`y`) through yt-dlp, with thumbnails and automatic retries.
- **Playlist files**: export a playlist as a tiny CSV of YouTube IDs and import it on another machine.
  Songs you already have are linked, and missing ones are downloaded.
- Plays MP3, FLAC, OGG, Opus, WAV, AAC, M4A, AIFF and more. Browse by artist, album or genre.
- Liked songs, play counts, Recently Played and Most Played lists, multi-select and a tag editor.

**Desktop**
- **System tray icon** with play/pause, next, previous and a song tooltip.
- **MPRIS2**: works with `playerctl`, Waybar, media keys and desktop widgets.
- Desktop notifications when the track changes.

## Quick start (Arch Linux)

```bash
# Build tools, runtime and fonts
sudo pacman -S --needed base-devel git rust alsa-lib yt-dlp ffmpeg curl kakasi \
    noto-fonts-cjk ttf-jetbrains-mono-nerd libnotify xdg-desktop-portal-gtk

# Better Japanese readings (optional, from the AUR)
yay -S mecab mecab-ipadic

# Build and install for your user (to ~/.local)
git clone https://github.com/CodingTort/vr_hv1-s-music-client.git
cd vr_hv1-s-music-client
make install
```

Then install the [UDEV Gothic NF](#fonts) font, make sure `~/.local/bin` is on your `PATH`, and launch
**Tort's Music Player** from your app launcher or run `lavanda`. Your music is read from `~/Music`.

## Requirements

### To build

| Dependency | Why | Arch | Debian / Ubuntu | Fedora |
|---|---|---|---|---|
| Rust **1.89 or newer** | compiler | `rust` (or `rustup`) | use [rustup](https://rustup.rs): distro packages are too old | `rust cargo` |
| C toolchain and `make` | linking, `make install` | `base-devel` | `build-essential` | `gcc make` |
| `pkg-config` | finds ALSA | `pkgconf` (in `base-devel`) | `pkg-config` | `pkgconf-pkg-config` |
| ALSA headers | audio output | `alsa-lib` | `libasound2-dev` | `alsa-lib-devel` |

### To run

These are already present on any normal Linux desktop:

- A **Wayland** compositor (tested on Hyprland) or an **X11** session
- Working **GPU drivers** (Vulkan or OpenGL, e.g. Mesa)
- **PipeWire**, PulseAudio or plain ALSA for sound
- A **D-Bus** session bus (for MPRIS and the tray icon)

### Optional features

Everything below is optional. When a tool is missing, the app keeps working and that one feature tells
you what to install.

| Feature | Needs | Arch | Debian / Ubuntu | Fedora |
|---|---|---|---|---|
| YouTube search and download | `yt-dlp`, `ffmpeg` | `yt-dlp ffmpeg` | `yt-dlp ffmpeg` (or yt-dlp's [official release](https://github.com/yt-dlp/yt-dlp#installation), which stays current) | `yt-dlp ffmpeg` (RPM Fusion) |
| Online lyrics, YouTube thumbnails | `curl` | `curl` | `curl` | `curl` |
| Romaji and furigana | `kakasi` | `kakasi` | `kakasi` | `kakasi` |
| Accurate Japanese readings (recommended alongside kakasi) | `mecab` with the IPAdic dictionary | AUR: `mecab mecab-ipadic` | `mecab mecab-ipadic-utf8` | `mecab mecab-ipadic` |
| Track-change notifications | `notify-send` | `libnotify` | `libnotify-bin` | `libnotify` |
| File dialogs (background image, playlist import/export) | an XDG desktop portal | `xdg-desktop-portal-gtk` (or `-hyprland`, `-kde`) | `xdg-desktop-portal-gtk` | `xdg-desktop-portal-gtk` |
| Tray icon | a StatusNotifierItem tray (Waybar `tray`, KDE, GNOME with AppIndicator) | — | — | — |
| "Show window" from the tray | Hyprland (`hyprctl`) | — | — | — |
| Live theme following | [Omarchy](https://omarchy.org) | — | — | — |
| Command-line control | `playerctl` | `playerctl` | `playerctl` | `playerctl` |

### Fonts

| Font | Used for | Install |
|---|---|---|
| **UDEV Gothic NF** | all text: Latin and Japanese in one matching style | Download `UDEVGothic_NF_*.zip` from the [UDEV Gothic releases](https://github.com/yuru7/udev-gothic/releases), copy the `UDEVGothicNF-*.ttf` files to `~/.local/share/fonts/`, then run `fc-cache -f` |
| **JetBrainsMono Nerd Font** | icons | Arch: `ttf-jetbrains-mono-nerd`, others: [nerdfonts.com](https://www.nerdfonts.com/font-downloads) |
| **Noto Sans CJK** | fallback for Chinese/Korean/Japanese text | Arch: `noto-fonts-cjk`, Debian: `fonts-noto-cjk`, Fedora: `google-noto-sans-cjk-fonts` |

## Installation

```bash
git clone https://github.com/CodingTort/vr_hv1-s-music-client.git
cd vr_hv1-s-music-client
make install          # builds in release mode, installs to ~/.local
```

`make install` puts these files in place:

| File | Location |
|---|---|
| program | `~/.local/bin/lavanda` |
| app launcher entry | `~/.local/share/applications/lavanda.desktop` |
| icon | `~/.local/share/icons/hicolor/{scalable,256x256}/apps/tort-music-player.*` |

To install system-wide instead, build as yourself, then copy the files as root:
`make && sudo make install-files PREFIX=/usr/local`. To remove it again, run `make uninstall` with the
same `PREFIX` (and `sudo` for a system-wide install).

Other useful targets: `make` (build only), `make test` (run the test suite), `make run`.

> The program is called `lavanda` because its settings and library data live in `~/.config/lavanda`,
> shared with the original project. If you are coming from lavanda, your library, playlists and settings
> carry over.

## Using it

**Your library.** Put music in `~/Music` (or set `music_dir`). It is scanned at start-up; press `F5` after
adding files. Browse by Artists, Albums or Genres in the sidebar. Covers come from the file's embedded art,
or from `cover.jpg` / `folder.jpg` next to it.

**Lyrics.** Press `k` or click the microphone button. To use your own lyrics, save a `.lrc` file with
the same name as the song next to it. If the lyrics are early or late, right-click the line that is being
sung right now; the offset is saved for that song.

**Japanese songs.** Use the **Romaji** switch at the top of the lyrics panel. Japanese mode shows furigana
over the kanji. Install MeCab for the most accurate readings: without it, kakasi is used, which
sometimes misreads conjugated verbs and set phrases.

**YouTube.** Press `y`, search, and press Enter or click a result to download it into your library.
Files are tagged automatically, including the video ID, which playlist files rely on.

**Playlist files.** Select a playlist and click its export icon to save a `.csv`. Use the import icon next
to `+` to load one: matching songs are linked, and missing ones are downloaded from YouTube one by one.
The format is `youtube_id,title,artist`, one song per line.

**Settings.** Click the gear icon or press `Ctrl+,` to change the theme, glass panels, corner radius,
background (colour, gradient, image or blurred album cover), cover size and visualiser style.

## Keyboard shortcuts

| Key | Action | | Key | Action |
|---|---|---|---|---|
| `Space` | play / pause | | `k` | lyrics panel |
| `n` / `p` | next / previous | | `[` / `]` | lyrics 0.5 s earlier / later |
| `→` / `←` | seek | | `t` | focus layout |
| `+` / `-` | volume | | `v` | visualiser |
| `s` / `r` | shuffle / repeat | | `y` | YouTube search |
| `↑` / `↓`, `Enter` | move, play | | `c` / `a` | new playlist / add to playlist |
| `/` | search | | `l` | like |
| `F5` | rescan library | | `e` | edit tags |
| `Ctrl+,` | settings | | `?` | all shortcuts |

Mouse: double-click a track to play it, right-click for context menus, `Ctrl+Click` / `Shift+Click` to
select several.

## Configuration

Settings are written to `~/.config/lavanda/config.toml` on first launch. Most of them can be changed in
the Settings dialog; every option is listed here:

```toml
music_dir = "~/Music"            # your library, scanned at start-up
volume = 0.8
shuffle = false
repeat = false
language = "auto"                # "auto", "en", "pt_BR", "es"
seek_step = 5                    # seconds per ← / →
volume_step = 0.05

# Appearance
theme_mode = "auto"              # "auto" follows Omarchy, "manual" uses theme_preset
theme_preset = "lavender"        # lavender, vampire, cotton candy, catppuccin mocha,
                                 # tokyo night, nord, gruvbox, everforest, rose pine
use_glassmorphism = true         # translucent panels
corner_radius = 10.0
background_kind = "none"         # "none", "color", "gradient", "image", "album" (blurred cover)
background_color = "#11111b"
background_gradient_from = "#1e1b2e"
background_gradient_to = "#0f0e17"
background_gradient_angle = 135.0
background_image = ""
cover_size = 240.0               # 140–400 px
spectrum_style = "mirrored"      # "mirrored" or "bottom"

# Lyrics
lyrics_online = true             # look lyrics up on lrclib.net; false = local files only

# YouTube (yt-dlp)
yt_dlp_path = "yt-dlp"
yt_dlp_format = "mp3"            # any format yt-dlp's --audio-format accepts
yt_dlp_quality = "128K"
yt_dlp_output = "%(title)s.%(ext)s"
yt_search_results = 15
```

| Data | Where |
|---|---|
| settings | `~/.config/lavanda/config.toml` |
| volume, lyrics panel (shown, width, romaji) | `~/.config/lavanda/state.toml` |
| playlists, likes, play counts, lyric offsets | `~/.config/lavanda/db.json` |
| downloaded lyrics | `~/.cache/lavanda/lyrics/` |

## Desktop integration

**Waybar.** The tray icon appears in Waybar's `tray` module. To show the current song, add an `mpris` module:

```jsonc
"mpris": {
    "format": "{player_icon}  {title} — {artist}",
    "player-icons": { "lavanda": "󰝚", "default": "󰝚" },
    "on-click": "playerctl -p lavanda play-pause",
    "on-scroll-up": "playerctl -p lavanda next",
    "on-scroll-down": "playerctl -p lavanda previous"
}
```

**Global shortcuts (Hyprland).**

```ini
bind = , XF86AudioPlay, exec, playerctl -p lavanda play-pause
bind = , XF86AudioNext, exec, playerctl -p lavanda next
bind = , XF86AudioPrev, exec, playerctl -p lavanda previous
```

**Tray icon.** Left click shows the window, middle click plays or pauses, scrolling skips tracks, and
right click opens the menu.

## Privacy

The player works fully offline. It only goes online for these:

| What | Sends | To |
|---|---|---|
| Lyrics lookup (turn off with `lyrics_online = false`) | the playing song's title, artist and length | lrclib.net |
| YouTube search and download | your search text, or the video you picked | YouTube, via yt-dlp |
| Search-result thumbnails | the video ID | i.ytimg.com |

Nothing else leaves your computer.

## Project layout

```
src/
├── main.rs, app.rs         application state, messages, window
├── lyrics.rs               lyrics lookup, LRC parsing, romaji, furigana (MeCab / kakasi)
├── youtube.rs              yt-dlp search and download
├── playlist_io.rs          playlist CSV export / import
├── tray.rs                 system tray icon (StatusNotifierItem + dbusmenu)
├── config.rs, state.rs, persist.rs, locale.rs
├── audio/                  decoding and output, spectrum analysis, MPRIS
├── library/                library scanner, track model
└── ui/
    ├── theme.rs, icons.rs
    ├── views/              library, player, focus layout, lyrics panel
    └── components/         lyrics view, visualiser, dialogs, controls
assets/                     icon (Tux) and desktop entry
```

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md). Changes are listed in
the [changelog](CHANGELOG.md).

## Credits

- Based on [**lavanda**](https://github.com/sheep-farm/lavanda) by Flavio de Vasconcellos (MIT).
- Icon: [**Tux**](https://commons.wikimedia.org/wiki/File:Tux.svg) by Larry Ewing (lewing@isc.tamu.edu), created with The GIMP.
- Lyrics from [**LRCLIB**](https://lrclib.net), a free and open lyrics database.
- Japanese readings by [**MeCab**](https://taku910.github.io/mecab/) and [**KAKASI**](http://kakasi.namazu.org/).
- Downloads by [**yt-dlp**](https://github.com/yt-dlp/yt-dlp). Font: [**UDEV Gothic**](https://github.com/yuru7/udev-gothic) by Yuko Otawara.
- Built with [iced](https://iced.rs), [Symphonia](https://github.com/pdeljanov/Symphonia), [cpal](https://github.com/RustAudio/cpal), [lofty](https://github.com/Serial-ATA/lofty-rs) and [zbus](https://github.com/dbus2/zbus).

Please only download music you have the right to download.

## License

[MIT](LICENSE). Copyright © 2026 CodingTort, and © 2026 Flavio de Vasconcellos for the original lavanda code.
