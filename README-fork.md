# lavanda (local fork)

Your copy of lavanda 1.0.1, with one added feature: YouTube search & download.

Upstream: https://github.com/sheep-farm/lavanda (MIT). All upstream code is
unmodified except where noted in the commit history.

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

## Taking upstream updates

```bash
git fetch origin
git rebase origin/master feature/youtube-search
git build --release && install -Dm755 target/release/lavanda ~/.local/bin/lavanda
```

If the rebase conflicts in `src/app.rs` (the file the feature touches most),
resolve it, then rebuild. `src/youtube.rs` and `src/ui/components/yt_search.rs`
are new files and rarely conflict.
