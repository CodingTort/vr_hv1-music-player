//! Keyboard and mouse.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app::{App, Input, Items, Modal, Nav, Pane, PlaylistPrompt, Settings, TAG_FIELDS};
use crate::lyrics::Lyrics;

pub fn on_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q')) {
        app.should_quit = true;
        return;
    }
    if app.modal.is_some() {
        modal_key(app, key);
        return;
    }
    if app.searching {
        search_key(app, key);
        return;
    }
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let step = crate::config::get().seek_step as f64;
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::F(1) => app.modal = Some(Modal::Help),
        KeyCode::Char(' ') => app.toggle_play(),
        KeyCode::Char('n') => app.next(),
        KeyCode::Char('p') => app.previous(),
        KeyCode::Left => app.seek_by(if shift { -30.0 } else { -step }),
        KeyCode::Right => app.seek_by(if shift { 30.0 } else { step }),
        KeyCode::Char('+') | KeyCode::Char('=') => app.volume_by(crate::config::get().volume_step),
        KeyCode::Char('-') => app.volume_by(-crate::config::get().volume_step),
        KeyCode::Char('s') => app.toggle_shuffle(),
        KeyCode::Char('r') => app.toggle_repeat(),
        KeyCode::Char('l') | KeyCode::Char('f') => app.toggle_like(),
        KeyCode::Char('k') => app.toggle_lyrics(),
        KeyCode::Char('j') => app.toggle_romaji(),
        KeyCode::Char('g') => app.toggle_furigana(),
        KeyCode::Char('[') => app.set_lyrics_offset(app.lyrics_offset - 0.5),
        KeyCode::Char(']') => app.set_lyrics_offset(app.lyrics_offset + 0.5),
        KeyCode::Char('0') => app.set_lyrics_offset(0.0),
        KeyCode::Char('v') => app.show_spectrum = !app.show_spectrum,
        KeyCode::Char('t') => app.now_view = !app.now_view,
        KeyCode::Char('o') => app.toggle_output(),
        KeyCode::Char('O') => app.open_phone_dialog(),
        KeyCode::Char('y') => app.open_youtube(),
        KeyCode::Char('e') => app.open_tag_editor(),
        KeyCode::Char(',') => app.modal = Some(Modal::Settings(Settings { cursor: 0 })),
        KeyCode::F(5) => app.rescan(crate::app::Rescan::Manual),
        KeyCode::Char('/') => {
            app.searching = true;
            app.pane = Pane::List;
            app.now_view = false;
        }
        KeyCode::Char('c') => app.prompt("New playlist", "Name of the new playlist", "", PlaylistPrompt::New { then_add: None }),
        KeyCode::Char('a') => match app.selected_track().or(app.current) {
            Some(i) => {
                let path = app.tracks[i].path.clone();
                app.modal = Some(Modal::AddToPlaylist { path, cursor: 0 });
            }
            None => app.info("Select a song to add it to a playlist"),
        },
        KeyCode::Char('d') | KeyCode::Delete => {
            if app.current_playlist().is_some() {
                app.remove_from_playlist();
            }
        }
        KeyCode::Char('D') => match target_playlist(app) {
            Some(name) => app.modal = Some(Modal::ConfirmDelete(name)),
            None => app.info("Select a playlist first"),
        },
        KeyCode::Char('R') => match target_playlist(app) {
            Some(name) => app.prompt("Rename playlist", &format!("New name for “{name}”"), &name.clone(), PlaylistPrompt::Rename(name)),
            None => app.info("Select a playlist first"),
        },
        KeyCode::Char('x') => match target_playlist(app) {
            Some(name) => {
                let default = format!("{}/{}.csv", crate::config::get().music_dir.trim_end_matches('/'), name);
                app.prompt("Export playlist", &format!("Save “{name}” as a CSV of YouTube IDs to:"), &default, PlaylistPrompt::Export(name));
            }
            None => app.info("Select a playlist first"),
        },
        KeyCode::Char('i') => {
            let default = format!("{}/", crate::config::get().music_dir.trim_end_matches('/'));
            app.prompt("Import playlist", "Path of a playlist CSV (missing songs are downloaded):", &default, PlaylistPrompt::Import);
        }
        KeyCode::Tab => cycle_pane(app, 1),
        KeyCode::BackTab => cycle_pane(app, -1),
        KeyCode::Char('1') => focus(app, Pane::Nav),
        KeyCode::Char('2') => focus(app, Pane::List),
        KeyCode::Char('3') => {
            if app.show_lyrics {
                focus(app, Pane::Lyrics)
            }
        }
        KeyCode::Char('S') => sync_selected_line(app),
        KeyCode::Up => move_in_pane(app, -1),
        KeyCode::Down => move_in_pane(app, 1),
        KeyCode::PageUp => move_in_pane(app, -10),
        KeyCode::PageDown => move_in_pane(app, 10),
        KeyCode::Home => move_in_pane(app, -100_000),
        KeyCode::End => move_in_pane(app, 100_000),
        KeyCode::Enter => match app.pane {
            Pane::Nav => focus(app, Pane::List),
            Pane::List => app.activate(),
            Pane::Lyrics => seek_selected_line(app),
        },
        KeyCode::Esc | KeyCode::Backspace => {
            if app.now_view {
                app.now_view = false;
            } else if app.pane == Pane::Lyrics && app.lyric_cursor.is_some() {
                app.lyric_cursor = None;
            } else if !app.back() && app.narrow && app.pane == Pane::List {
                focus(app, Pane::Nav);
            }
        }
        _ => {}
    }
}

/// The playlist being shown, or highlighted in the library list.
fn target_playlist(app: &App) -> Option<String> {
    if let Some(p) = app.current_playlist() {
        return Some(p);
    }
    match app.nav.get(app.nav_cursor) {
        Some(Nav::Playlist(p)) if app.pane == Pane::Nav => Some(p.clone()),
        _ => None,
    }
}

fn focus(app: &mut App, pane: Pane) {
    app.pane = pane;
    app.now_view = false;
}

fn cycle_pane(app: &mut App, dir: i32) {
    let mut panes = vec![Pane::Nav, Pane::List];
    if app.show_lyrics {
        panes.push(Pane::Lyrics);
    }
    let i = panes.iter().position(|p| *p == app.pane).unwrap_or(1) as i32;
    focus(app, panes[(i + dir).rem_euclid(panes.len() as i32) as usize]);
}

fn move_in_pane(app: &mut App, delta: isize) {
    match app.pane {
        Pane::Nav => {
            let n = app.nav.len() as isize;
            let i = (app.nav_cursor as isize + delta).clamp(0, n - 1) as usize;
            if i != app.nav_cursor {
                app.select_nav(i);
            }
        }
        Pane::List => app.move_cursor(delta),
        Pane::Lyrics => {
            let Some(n) = shown_lines(app).map(|l| l.len()) else { return };
            if n == 0 {
                return;
            }
            let start = app.lyric_cursor.unwrap_or_else(|| {
                crate::view::active_line(&shown_lines(app).unwrap_or_default(), app.lyric_position()).unwrap_or(0)
            });
            app.lyric_cursor = Some((start as isize + delta).clamp(0, n as isize - 1) as usize);
            app.lyric_touched = Some(Instant::now());
        }
    }
}

fn shown_lines(app: &App) -> Option<Vec<crate::lyrics::LyricLine>> {
    let track = app.current_track()?;
    match app.shown_lyrics(track).0? {
        Lyrics::Synced(l) => Some(l.clone()),
        Lyrics::Plain(l) => Some(l.clone()),
        _ => None,
    }
}

fn synced_lines(app: &App) -> Option<Vec<crate::lyrics::LyricLine>> {
    let track = app.current_track()?;
    match app.shown_lyrics(track).0? {
        Lyrics::Synced(l) => Some(l.clone()),
        _ => None,
    }
}

fn seek_selected_line(app: &mut App) {
    if let (Some(lines), Some(i)) = (synced_lines(app), app.lyric_cursor) {
        if let Some(line) = lines.get(i) {
            app.seek_to_line(line.time);
            app.lyric_cursor = None;
        }
    }
}

fn sync_selected_line(app: &mut App) {
    let Some(lines) = synced_lines(app) else {
        app.info("This song has no synced lyrics");
        return;
    };
    let i = app.lyric_cursor.or_else(|| crate::view::active_line(&lines, app.lyric_position()));
    match i.and_then(|i| lines.get(i)) {
        Some(line) => {
            app.sync_lyric_line(line.time);
            app.lyric_cursor = None;
        }
        None => app.info("Select a lyric line first (tab to the lyrics, ↑↓)"),
    }
}

fn edit_input(input: &mut Input, key: KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('u') if ctrl => input.clear(),
        KeyCode::Char('w') if ctrl => input.delete_word(),
        KeyCode::Char('a') if ctrl => input.home(),
        KeyCode::Char('e') if ctrl => input.end(),
        KeyCode::Char(c) if !ctrl => input.insert(c),
        KeyCode::Backspace => input.backspace(),
        KeyCode::Delete => input.delete(),
        KeyCode::Left => input.left(),
        KeyCode::Right => input.right(),
        KeyCode::Home => input.home(),
        KeyCode::End => input.end(),
        _ => return false,
    }
    true
}

fn search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.searching = false;
            app.query.clear();
            app.refresh_items();
        }
        KeyCode::Enter => app.searching = false,
        KeyCode::Up => app.move_cursor(-1),
        KeyCode::Down => app.move_cursor(1),
        _ => {
            if edit_input(&mut app.query, key) {
                app.cursor = 0;
                app.refresh_items();
            }
        }
    }
}

fn modal_key(app: &mut App, key: KeyEvent) {
    let Some(modal) = &mut app.modal else { return };
    match modal {
        Modal::Help | Modal::Phone => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?')) {
                app.modal = None;
            } else if key.code == KeyCode::Char('o') && matches!(app.modal, Some(Modal::Phone)) {
                app.modal = None;
                app.toggle_output();
            }
        }
        Modal::Prompt(p) => match key.code {
            KeyCode::Esc => app.modal = None,
            KeyCode::Enter => app.submit_prompt(),
            _ => {
                edit_input(&mut p.input, key);
            }
        },
        Modal::ConfirmDelete(name) => {
            let name = name.clone();
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    app.modal = None;
                    app.delete_playlist(&name);
                }
                _ => app.modal = None,
            }
        }
        Modal::AddToPlaylist { path, cursor } => {
            let n = app_playlists_len(&app.nav) + 1;
            match key.code {
                KeyCode::Esc => app.modal = None,
                KeyCode::Up => *cursor = cursor.saturating_sub(1),
                KeyCode::Down => *cursor = (*cursor + 1).min(n - 1),
                KeyCode::Enter => {
                    let (path, cursor) = (path.clone(), *cursor);
                    choose_playlist(app, path, cursor);
                }
                _ => {}
            }
        }
        Modal::YouTube(d) => {
            if d.typing {
                match key.code {
                    KeyCode::Esc => app.modal = None,
                    KeyCode::Enter => app.youtube_search(),
                    KeyCode::Down | KeyCode::Tab if !d.results.is_empty() => d.typing = false,
                    _ => {
                        edit_input(&mut d.input, key);
                    }
                }
            } else {
                match key.code {
                    KeyCode::Esc => app.modal = None,
                    KeyCode::Up if d.cursor == 0 => d.typing = true,
                    KeyCode::Up => {
                        d.cursor -= 1;
                        app.youtube_preview();
                    }
                    KeyCode::Down => {
                        d.cursor = (d.cursor + 1).min(d.results.len().saturating_sub(1));
                        app.youtube_preview();
                    }
                    KeyCode::Char('/') | KeyCode::Tab => d.typing = true,
                    KeyCode::Enter => app.youtube_download(),
                    KeyCode::Char('A') => app.youtube_download_all(),
                    _ => {}
                }
            }
        }
        Modal::Tags(ed) => match key.code {
            KeyCode::Esc => app.modal = None,
            KeyCode::Enter => app.save_tags(),
            KeyCode::Up | KeyCode::BackTab => ed.cursor = ed.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => ed.cursor = (ed.cursor + 1).min(TAG_FIELDS.len() - 1),
            _ => {
                let i = ed.cursor;
                edit_input(&mut ed.fields[i], key);
            }
        },
        Modal::Settings(st) => {
            let row = st.cursor;
            match key.code {
                KeyCode::Esc | KeyCode::Char(',') | KeyCode::Char('q') => app.modal = None,
                KeyCode::Up => st.cursor = st.cursor.saturating_sub(1),
                KeyCode::Down => st.cursor = (st.cursor + 1).min(App::SETTINGS.len() - 1),
                KeyCode::Left => app.change_setting(row, -1),
                KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => app.change_setting(row, 1),
                _ => {}
            }
        }
    }
}

fn choose_playlist(app: &mut App, path: std::path::PathBuf, cursor: usize) {
    let playlists = app.playlists();
    app.modal = None;
    match playlists.get(cursor) {
        Some(name) => app.add_to_playlist(name, path),
        None => app.prompt("New playlist", "Name of the new playlist (the song is added to it)", "", PlaylistPrompt::New { then_add: Some(path) }),
    }
}

// ── Mouse ────────────────────────────────────────────────────────────────────

fn inside(r: Rect, x: u16, y: u16) -> bool {
    r.contains(Position::new(x, y))
}

pub fn on_mouse(app: &mut App, m: MouseEvent) {
    let (x, y) = (m.column, m.row);
    match m.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let d: isize = if m.kind == MouseEventKind::ScrollUp { -3 } else { 3 };
            if let Some(modal) = &mut app.modal {
                match modal {
                    Modal::YouTube(dlg) if !dlg.results.is_empty() => {
                        dlg.typing = false;
                        dlg.cursor = (dlg.cursor as isize + d.signum()).clamp(0, dlg.results.len() as isize - 1) as usize;
                        app.youtube_preview();
                    }
                    Modal::AddToPlaylist { cursor, .. } => {
                        let n = app_playlists_len(&app.nav) + 1;
                        *cursor = (*cursor as isize + d.signum()).clamp(0, n as isize - 1) as usize;
                    }
                    _ => {}
                }
                return;
            }
            let pane = if inside(app.areas.nav, x, y) {
                Pane::Nav
            } else if inside(app.areas.lyrics, x, y) {
                Pane::Lyrics
            } else {
                Pane::List
            };
            let saved = app.pane;
            app.pane = pane;
            move_in_pane(app, if pane == Pane::Nav { d.signum() } else { d });
            if pane != Pane::Lyrics {
                app.pane = saved;
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let double = app.last_click.is_some_and(|(t, cx, cy)| t.elapsed() < Duration::from_millis(400) && cx == x && cy == y);
            app.last_click = Some((Instant::now(), x, y));
            if app.modal.is_some() {
                modal_click(app, x, y, double);
                return;
            }
            if let Some(&(_, pane)) = app.areas.tabs.iter().find(|(r, _)| inside(*r, x, y)) {
                focus(app, pane);
                return;
            }
            if inside(app.areas.progress, x, y) && app.areas.progress.width > 0 {
                let frac = (x - app.areas.progress.x) as f64 / app.areas.progress.width.max(1) as f64;
                let at = app.duration.as_secs_f64() * frac;
                app.seek_to(Duration::from_secs_f64(at));
                return;
            }
            if let Some(&(_, i)) = app.areas.nav_rows.iter().find(|(row, _)| *row == y).filter(|_| inside(app.areas.nav, x, y)) {
                app.pane = Pane::Nav;
                app.select_nav(i);
                if app.narrow {
                    focus(app, Pane::List);
                }
                return;
            }
            if inside(app.areas.list_rows, x, y) {
                let row = app.areas.list_offset + (y - app.areas.list_rows.y) as usize;
                if row < app.items.len() {
                    app.pane = Pane::List;
                    app.cursor = row;
                    let is_group = matches!(app.items, Items::Groups(..));
                    if double || is_group {
                        app.activate();
                    }
                }
                return;
            }
            if inside(app.areas.lyrics, x, y) {
                if let Some(&(_, i)) = app.areas.lyric_rows.iter().find(|(row, _)| *row == y) {
                    if let Some(line) = synced_lines(app).and_then(|l| l.get(i).cloned()) {
                        app.seek_to_line(line.time);
                        app.lyric_cursor = None;
                    }
                }
            }
        }
        _ => {}
    }
}

fn app_playlists_len(nav: &[Nav]) -> usize {
    nav.iter().filter(|n| matches!(n, Nav::Playlist(_))).count()
}

fn modal_click(app: &mut App, x: u16, y: u16, double: bool) {
    let rows = app.areas.modal_rows;
    if !inside(rows, x, y) {
        return;
    }
    let row = app.areas.modal_offset + (y - rows.y) as usize;
    match &mut app.modal {
        Some(Modal::YouTube(d)) if row < d.results.len() => {
            d.typing = false;
            d.cursor = row;
            if double {
                app.youtube_download();
            } else {
                app.youtube_preview();
            }
        }
        Some(Modal::AddToPlaylist { path, cursor }) => {
            let n = app_playlists_len(&app.nav) + 1;
            if row < n {
                *cursor = row;
                let path = path.clone();
                choose_playlist(app, path, row);
            }
        }
        _ => {}
    }
}
