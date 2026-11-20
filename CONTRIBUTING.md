# Contributing

Thanks for helping out! Bug reports, ideas and pull requests are all welcome.

## Reporting a bug

Open an issue and include:

- what you did, what you expected, and what happened instead
- your distribution, desktop or compositor, and the output of `lavanda --version`
- for lyrics problems: the song's title and artist as shown in the app, and whether a `.lrc` file is next to it

Please don't paste song lyrics into issues. The title and artist are enough to reproduce a lookup.

## Development setup

Install the [build requirements](README.md#to-build), then:

```bash
make            # release build of both programs
make test       # run the tests
make run        # build and start the desktop app
make run-tui    # build and start the terminal client (vrmusic-tui)
```

`cargo build` gives a faster debug build while you work. The terminal client (`src/tui/`) includes the
desktop app's modules with `#[path]`, so a change there is compiled into both programs: build both.

## Pull requests

- Keep each pull request to one change, and explain the *why* in the description.
- Run `make test` and make sure `cargo build` shows no warnings.
- Add a test for logic that can be tested without a window (parsing, matching, lyric timing, image
  processing). See the existing tests in `src/lyrics.rs` and `src/app.rs`.
- Test data must be invented: no real song lyrics in code, tests or commits.
- Text in the UI (artist names, titles, lyrics) can be in any script, so use `Shaping::Advanced` for it.
- Add a line to [CHANGELOG.md](CHANGELOG.md) under an "Unreleased" heading.

## Code layout

See [Project layout](README.md#project-layout) in the README. In short: `lyrics.rs` holds lyric lookup
and Japanese text handling, `ui/components/lyrics.rs` is the lyrics widget, `youtube.rs` drives yt-dlp,
and `app.rs` ties the application together. The terminal client lives in `src/tui/`; its phone player
is `phone.rs` (server) and `phone.html` (page).
