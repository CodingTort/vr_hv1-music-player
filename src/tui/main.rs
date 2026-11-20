//! lavanda-tui: the terminal client of Tort's Music Player.
//!
//! Shares the GUI's code (audio engine, library, lyrics, YouTube, playlists,
//! themes) and its settings and library data in `~/.config/lavanda`. Over SSH
//! it can send the music to a phone (`o`, see `phone.rs`).

use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use ratatui::crossterm::execute;

// The GUI's modules, shared as they are. Not every function in them is used
// here, which is expected.
#[allow(dead_code, unused_imports)]
#[path = "../audio/mod.rs"]
mod audio;
#[allow(dead_code)]
#[path = "../config.rs"]
mod config;
#[allow(dead_code)]
#[path = "../cover.rs"]
mod cover;
#[allow(dead_code, unused_imports)]
#[path = "../library/mod.rs"]
mod library;
#[allow(dead_code)]
#[path = "../locale.rs"]
mod locale;
#[allow(dead_code)]
#[path = "../lyrics.rs"]
mod lyrics;
#[allow(dead_code)]
#[path = "../persist.rs"]
mod persist;
#[allow(dead_code)]
#[path = "../playlist_io.rs"]
mod playlist_io;
#[allow(dead_code)]
#[path = "../state.rs"]
mod state;
#[allow(dead_code)]
#[path = "../ui/theme.rs"]
mod theme;
#[allow(dead_code)]
#[path = "../youtube.rs"]
mod youtube;

mod app;
mod art;
mod input;
mod phone;
mod view;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> io::Result<()> {
    let mut start_on_phone = false;
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("lavanda-tui {VERSION}");
                return Ok(());
            }
            "--phone" => start_on_phone = true,
            "--help" | "-h" => {
                println!("{} (terminal) {VERSION}", audio::mpris::APP_NAME);
                println!("Music player for the terminal, with synced lyrics and phone playback.");
                println!();
                println!("USAGE:");
                println!("    lavanda-tui [--phone]");
                println!();
                println!("OPTIONS:");
                println!("        --phone      Start with the music playing on your phone");
                println!("    -h, --help       Print this help");
                println!("    -V, --version    Print the version");
                println!();
                println!("Press ? inside for all keys. Settings and library are shared with the");
                println!("desktop app (~/.config/lavanda). Run one of the two at a time.");
                return Ok(());
            }
            other => {
                eprintln!("lavanda-tui: unknown option '{other}'");
                eprintln!("Try 'lavanda-tui --help'.");
                std::process::exit(1);
            }
        }
    }

    config::load();
    locale::load();
    persist::init();
    let cfg = config::get();
    theme::reload_with_mode(&cfg.theme_mode, &cfg.theme_preset);

    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;

    let mut player = audio::AudioPlayer::spawn();
    let mut audio_events = player.take_events();
    let (spectrum_tx, mut spectrum_rx) = tokio::sync::mpsc::unbounded_channel();
    audio::spectrum::launch(player.viz_buf.clone(), spectrum_tx);
    let (mpris_cmd_tx, mut mpris_cmds) = tokio::sync::mpsc::unbounded_channel();
    let (mpris_tx, mpris_rx) = tokio::sync::mpsc::unbounded_channel();
    audio::mpris::launch(mpris_cmd_tx, mpris_rx);
    let (jobs_tx, mut jobs) = tokio::sync::mpsc::unbounded_channel();

    let mut app = app::App::new(rt.handle().clone(), jobs_tx, player, mpris_tx);
    if start_on_phone {
        app.toggle_output();
    }

    // Messages the shared code prints (theme changes, audio errors) would
    // land on top of the screen: send them to a log while the UI runs, and
    // put stderr back before a crash report or on the way out.
    let stderr = quiet_stderr();
    let mut terminal = ratatui::init();
    let ratatui_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_stderr(stderr);
        ratatui_hook(info);
    }));
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = (|| -> io::Result<()> {
        while !app.should_quit {
            // The GUI's smoothing: bars jump up with a hit, fall back slower.
            while let Ok(bins) = spectrum_rx.try_recv() {
                if app.output == app::Output::Laptop {
                    for (a, b) in app.spectrum.iter_mut().zip(bins.iter()) {
                        let k = if *b > *a { 0.7 } else { 0.25 };
                        *a += (b - *a) * k;
                    }
                }
            }
            while let Ok(ev) = audio_events.try_recv() {
                app.on_audio(ev);
            }
            while let Ok(cmd) = mpris_cmds.try_recv() {
                app.on_mpris(cmd);
            }
            while let Ok(job) = jobs.try_recv() {
                app.on_job(job);
            }
            app.sync_phone();
            app.tick();

            terminal.draw(|f| view::draw(f, &mut app))?;

            // ~30 frames a second while there's input to wait for.
            if event::poll(Duration::from_millis(33))? {
                match event::read()? {
                    Event::Key(key) if key.kind != KeyEventKind::Release => input::on_key(&mut app, key),
                    Event::Mouse(m) => input::on_mouse(&mut app, m),
                    _ => {}
                }
            }
        }
        Ok(())
    })();

    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    restore_stderr(stderr);
    app.player.send(audio::AudioCommand::Stop);
    result
}

/// Points stderr at `~/.cache/lavanda/tui.log`; returns the real stderr.
fn quiet_stderr() -> Option<i32> {
    use std::os::fd::AsRawFd;
    let home = std::env::var("HOME").ok()?;
    let dir = std::path::PathBuf::from(home).join(".cache/lavanda");
    std::fs::create_dir_all(&dir).ok()?;
    let log = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("tui.log")).ok()?;
    // SAFETY: plain file-descriptor calls; `log` is open for the duration.
    unsafe {
        let saved = libc::dup(2);
        if saved < 0 {
            return None;
        }
        if libc::dup2(log.as_raw_fd(), 2) < 0 {
            libc::close(saved);
            return None;
        }
        Some(saved)
    }
}

fn restore_stderr(saved: Option<i32>) {
    if let Some(fd) = saved {
        // SAFETY: `fd` is the duplicate made by `quiet_stderr`.
        unsafe {
            libc::dup2(fd, 2);
        }
    }
}
