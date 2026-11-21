# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [1.3.1] - 2026-11-21

### Fixed

- Desktop app: with the cursor in the YouTube dialog's search box, Esc only took the cursor out of the
  box, and a second Esc was needed to close the dialog. In between, typing went nowhere and Enter
  downloaded the selected result. One Esc closes the dialog now.

## [1.3.0] - 2026-11-21

### Added

- **Links in the YouTube dialog** (`y`, desktop app and terminal client): paste a link to a song, a
  playlist or an album (YouTube, YouTube Music, or any site yt-dlp supports) instead of a search. A
  playlist or album lists all its songs: download one, or all of them into a new playlist named after it
  (**Download all into a playlist**, or `A` in the terminal client). Songs you already have are linked
  instead of downloaded again.
- The YouTube dialog shows a large preview of the selected result's cover, with its title, channel and
  length.

### Changed

- The YouTube dialog is bigger, with rounded corners, results and search box. The search box has the
  keyboard focus when the dialog opens.
- **Faster to build**: the release build is optimised for speed instead of size, without whole-program
  optimisation. On a 20-core machine a clean build takes 2 minutes instead of 3, and a rebuild after
  changing the app's code 25 seconds instead of 2 minutes. The program is just as fast (the binaries are
  larger: 25 MB instead of 15).
- **Smoother with big libraries**: the song list only builds the rows on screen. With 1,500 songs listed,
  building each frame takes a fifth of the time it did.
- **An idle player stays idle**: the visualiser stops redrawing the window a second after the music
  stops. Before, it redrew 30 times a second, using about 30 % of a CPU core with nothing playing.
- Playlists list their songs in the playlist's own order (an imported album plays in album order), and
  the playlist names are sorted alphabetically (they came in a different order on every start).
- Long titles in the song list stay on one line instead of wrapping into two clipped half-lines.

### Fixed

- Opening the YouTube dialog and every search briefly froze the window while it checked that yt-dlp
  works. The check now runs in the background, once.

## [1.2.2] - 2026-11-20

### Changed

- Lyrics stay where you scroll them for 10 seconds after you stop, then follow the song again: on the
  phone player (which used to snap back at once), in the terminal client (which used to stay put until
  you pressed Esc) and in the desktop app (which waited 3 seconds).
- The phone player keeps the line being sung in the centre of the lyrics box, also for the first and last
  lines of a song.

### Fixed

- `--about` in 1.2.1 still named the repository's old address.

## [1.2.1] - 2026-11-20

### Fixed

- Through `vrmusic`, tmux showed the terminal client in 256 colours (posterised album art and theme)
  when the terminal didn't announce 24-bit colour, as over SSH. `vrmusic` now tells tmux it can.
- The phone player's lyrics box scrolled past the line being sung, leaving it out of view.
- The phone player showed a broken image for songs without a cover; it shows the app icon now.
- Settings and window state were saved with float noise (`volume = 0.800000011920929`); they're saved
  as written now (`volume = 0.8`).

### Changed

- The phone player's pages use the player's colours (your Omarchy theme or the chosen preset).
- Screenshots in the Tokyo Night theme, and an accurate picture of the iPhone setup.

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

[1.3.1]: https://github.com/CodingTort/vr_hv1-music-player/releases/tag/v1.3.1
[1.3.0]: https://github.com/CodingTort/vr_hv1-music-player/releases/tag/v1.3.0
[1.2.2]: https://github.com/CodingTort/vr_hv1-music-player/releases/tag/v1.2.2
[1.2.1]: https://github.com/CodingTort/vr_hv1-music-player/releases/tag/v1.2.1
[1.2.0]: https://github.com/CodingTort/vr_hv1-music-player/releases/tag/v1.2.0
[1.1.1]: https://github.com/CodingTort/vr_hv1-music-player/releases/tag/v1.1.1
[1.1.0]: https://github.com/CodingTort/vr_hv1-music-player/releases/tag/v1.1.0
