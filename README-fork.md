# lavanda (local fork)

Your copy of lavanda 1.0.1, with one added feature: YouTube search & download.

Upstream: https://github.com/sheep-farm/lavanda (MIT). All upstream code is
unmodified except where noted in the commit history.

Shown as **Tort's Music Player** (window title, header, notifications, MPRIS
identity). The binary, config directory and MPRIS bus name stay `lavanda`, so
`playerctl -p lavanda` and existing scripts keep working.

The icon is Tux by Larry Ewing (lewing@isc.tamu.edu), created with The GIMP —
`assets/tux.svg`, from Wikimedia Commons. Install it for the desktop entry:

```bash
install -Dm644 assets/tux.png ~/.local/share/icons/hicolor/256x256/apps/tort-music-player.png
install -Dm644 assets/tux.svg ~/.local/share/icons/hicolor/scalable/apps/tort-music-player.svg
install -Dm644 assets/lavanda.desktop ~/.local/share/applications/lavanda.desktop
```

## Branches

| Branch | What it is |
|---|---|
| `master` | Pristine upstream 1.0.1, exactly as released. Do not edit. |
| `feature/youtube-search` | Your fork. Upstream + the YouTube feature. |

Tag `yt-search-base` marks the pristine 1.0.1 commit for diffing.

```bash
git diff yt-search-base feature/youtube-search   # see exactly what was added
```

## Building and installing

```bash
cargo build --release
install -Dm755 target/release/lavanda ~/.local/bin/lavanda
```

The pristine upstream binary is kept at `~/.local/bin/lavanda-1.0.1-upstream`.
To roll back to stock lavanda:

```bash
install -m755 ~/.local/bin/lavanda-1.0.1-upstream ~/.local/bin/lavanda
```

## Requirements added by the fork

```bash
sudo pacman -S yt-dlp
```

Already satisfied: `rust`, `ffmpeg`, `noto-fonts-cjk`, JetBrainsMono Nerd Font
(in `~/.local/share/fonts/JetBrainsMonoNerdFont/`).

Text uses **UDEV Gothic NF** (JetBrains Mono + BIZ UD Gothic + Nerd Font
icons, SIL OFL), so Japanese matches the Latin text. Install it from
<https://github.com/yuru7/udev-gothic/releases> (the `UDEVGothic_NF_*.zip`):
copy `UDEVGothicNF-*.ttf` to `~/.local/share/fonts/UDEVGothicNF/` and run
`fc-cache -f`. Icons still use JetBrainsMono Nerd Font Mono.

Synced lyrics (`k`) fetch from lrclib.net with `curl`, which Arch ships in
`base`. `noto-fonts-cjk` is what renders Japanese/Korean/Chinese lyrics.
The lyrics panel's Romaji switch and the furigana use `kakasi`
(`sudo pacman -S kakasi`); without it the switch explains what to install.
The tray icon needs Waybar's `tray` module (or any StatusNotifierItem tray).

## Taking upstream updates

```bash
git fetch origin
git rebase origin/master feature/youtube-search
git build --release && install -Dm755 target/release/lavanda ~/.local/bin/lavanda
```

If the rebase conflicts in `src/app.rs` (the file the feature touches most),
resolve it, then rebuild. `src/youtube.rs` and `src/ui/components/yt_search.rs`
are new files and rarely conflict.
