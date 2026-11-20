# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [1.2.0] - 2026-11-20

### Added

- **Terminal client** (`vrmusic-tui`), rebuilt on the desktop app's code: library by songs, artists,
  albums, genres, liked, recently and most played, and playlists; search; synced lyrics with timing
  fixes, the Japanese ⇄ romaji switch and furigana; YouTube search and download; playlists (create, add,
  remove, rename, delete, CSV export and import); tag editing; settings and themes (including the live
  Omarchy theme); album art drawn in the terminal; visualiser; media keys and notifications; mouse and
  touch; a layout that fits a phone screen over SSH. `make install` installs it next to the desktop app.
- **Listen on your phone**: `o` in the terminal client moves playback to a page in the phone's browser,
  with album art, synced lyrics and lock-screen controls, reached over Tailscale or an SSH tunnel and
  protected by a secret key. The phone connects with a short address and a 6-digit code instead of a
  long link (which phone terminals cut off when they wrap it). Formats Safari can't play are converted
  with ffmpeg.
- `vrmusic`: starts the terminal client inside tmux (or returns to it), so it keeps playing when an SSH
  connection drops; without tmux it just runs the client.
- `phone_port` setting.

### Changed

- Shown as **vr_hv1's Music Player** (window, header, notifications, media controls, phone page).
  The desktop program stays `lavanda`, as do its config directory and MPRIS name.

- Album-cover squaring moved to a shared module (`src/cover.rs`), used by both programs.

## [1.1.1] - 2026-11-20

### Fixed

- Typing a non-ASCII character (such as `é`) into a colour field in Settings crashed the app.
- `lavanda --about` and `--help` described the original lavanda project and linked to its repository.
- "Search cover online" in the tag editor cut the search short at `&`, `#` or `/` in artist or album names.

### Documentation

- README: the MeCab AUR package is `mecab-git`; added a note for rustup users, a "Known limitations"
  section and the cover search in the privacy table; corrected what `/` and `language` do.

## [1.1.0] - 2026-11-20

The first release of this fork, based on [lavanda](https://github.com/sheep-farm/lavanda) 1.0.1.

### Added

- **Synced lyrics panel**: highlights the line being sung, glides with the song, click to seek, drag to
  resize. Also available beside the cover in the focus layout.
- Lyrics from a `.lrc` file next to the song, embedded lyrics, or LRCLIB, with lookups that understand
  YouTube-style titles and covers.
- Per-song lyrics timing: right-click the current line, or nudge with `[` / `]`.
- **Japanese ⇄ Romaji switch**, with romaji from a matching LRCLIB upload when there is one.
- **Furigana** above kanji, using MeCab when installed and kakasi otherwise.
- **YouTube search and download** through yt-dlp, with thumbnails.
- **Playlist export and import** as a CSV of YouTube IDs.
- **System tray icon** with menu and song tooltip.
- Settings dialog: nine theme presets, glass panels, corner radius, and colour, gradient, image or
  blurred-album-cover backgrounds.
- Adjustable cover size and a mirrored visualiser style.
- `make install` / `make uninstall`.

### Changed

- Shown as **Tort's Music Player** with a Tux icon. The program name and config directory stay `lavanda`.
- Rounded card layout with even spacing; covers are rounded and framed.
- UDEV Gothic NF for text, so Latin and Japanese match. Japanese, Chinese and Korean text no longer
  shows as boxes.
- YouTube thumbnails used as covers are shown whole instead of centre-cropped.
- Bright covers dim the blurred background, and lyrics take a tint that stays readable.
- The spectrum visualiser uses a decibel scale and follows the music's loudness instead of the volume slider.

### Fixed

- The last second of every song was cut off.
- The playback position ran about a second ahead of the audio.
- Files with an unknown length showed no progress.
- yt-dlp downloads failed on YouTube's intermittent HTTP 403 errors; searches took up to a minute.
- Switching between Japanese and romaji moved the highlighted line.
- Lyrics uploads with a translation woven in highlighted the translation.
- Leftover `notify-send` processes piled up.

[1.2.0]: https://github.com/CodingTort/vr_hv1-s-music-client/releases/tag/v1.2.0
[1.1.1]: https://github.com/CodingTort/vr_hv1-s-music-client/releases/tag/v1.1.1
[1.1.0]: https://github.com/CodingTort/vr_hv1-s-music-client/releases/tag/v1.1.0
