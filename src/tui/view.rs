//! Drawing. Wide terminals get the library, the song list and the lyrics
//! side by side; narrow ones (a phone over SSH) one pane at a time with tabs.

use std::time::Duration;

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{fmt_time, App, Items, Modal, Output, Pane, TAG_FIELDS};
use crate::audio::PlaybackState;
use crate::lyrics::{LyricLine, Lyrics, Ruby};

/// Below this width, one pane at a time.
pub const NARROW: u16 = 100;

// ── Colours (the GUI's palette: your Omarchy theme or a preset) ──────────────

#[derive(Clone, Copy)]
struct Pal {
    text: Color,
    sub: Color,
    dim: Color,
    accent: Color,
    surface: Color,
    red: Color,
    green: Color,
}

fn rgb(c: iced::Color) -> Color {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::Rgb(b(c.r), b(c.g), b(c.b))
}

fn pal() -> Pal {
    use crate::theme;
    Pal {
        text: rgb(theme::text()),
        sub: rgb(theme::subtext()),
        dim: rgb(theme::overlay0()),
        accent: rgb(theme::accent()),
        surface: rgb(theme::surface0()),
        red: rgb(theme::red()),
        green: rgb(theme::green()),
    }
}

// ── Text helpers ─────────────────────────────────────────────────────────────

pub fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// `s` cut to `w` columns, with "…" when cut.
pub fn fit(s: &str, w: usize) -> String {
    if width(s) <= w {
        return s.to_string();
    }
    if w == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if used + cw > w.saturating_sub(1) {
            break;
        }
        out.push(c);
        used += cw;
    }
    out.push('…');
    out
}

/// `s` cut or padded to exactly `w` columns.
fn pad(s: &str, w: usize) -> String {
    let s = fit(s, w);
    let n = w.saturating_sub(width(&s));
    s + &" ".repeat(n)
}

/// Splits `s` into lines of at most `w` columns (by character; lyrics are
/// short and often have no spaces to break at).
pub fn wrap(s: &str, w: usize) -> Vec<String> {
    if w == 0 {
        return vec![String::new()];
    }
    let mut lines = vec![String::new()];
    let mut used = 0;
    for word in s.split_inclusive(' ') {
        let ww = width(word.trim_end());
        if used > 0 && used + ww > w {
            lines.push(String::new());
            used = 0;
        }
        for c in word.chars() {
            let cw = c.width().unwrap_or(0);
            if used + cw > w {
                lines.push(String::new());
                used = 0;
                if c == ' ' {
                    continue;
                }
            }
            lines.last_mut().unwrap().push(c);
            used += cw;
        }
    }
    lines.into_iter().map(|l| l.trim_end().to_string()).collect()
}

/// A lyric line with furigana as pairs of (readings row, text row), each at
/// most `w` columns. Each reading is centred over its kanji and may overhang
/// the kana beside them, as in print; where two readings would touch, the
/// text gets a space instead, so a reading never drifts over the wrong kanji.
pub fn ruby_rows(pieces: &[Ruby], w: usize) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = vec![(String::new(), String::new())];
    let mut col = 0; // columns used in the current text row
    let mut read_end = 0; // columns used in the current readings row
    for piece in pieces {
        // A plain piece may be long: split it into characters as needed.
        let chunks: Vec<String> = if piece.reading.is_some() { vec![piece.text.clone()] } else { piece.text.chars().map(String::from).collect() };
        for chunk in chunks {
            let bw = width(&chunk);
            let rw = piece.reading.as_deref().map_or(0, width);
            // Spaces before the kanji, and where the reading starts.
            let place = |col: usize, read_end: usize| -> (usize, usize) {
                if rw == 0 {
                    return (0, 0);
                }
                let centred = col as isize + bw as isize / 2 - rw as isize / 2;
                let min = if read_end > 0 { read_end as isize + 1 } else { 0 };
                let left = (min - centred).max(0) as usize;
                (left, (centred + left as isize) as usize)
            };
            let (mut left, mut start) = place(col, read_end);
            if col > 0 && (col + left + bw > w || start + rw > w) {
                rows.push((String::new(), String::new()));
                col = 0;
                read_end = 0;
                (left, start) = place(0, 0);
            }
            if col == 0 && chunk == " " {
                continue;
            }
            let (readings, text) = rows.last_mut().unwrap();
            if let Some(r) = &piece.reading {
                readings.push_str(&" ".repeat(start - read_end));
                readings.push_str(r);
                read_end = start + rw;
            }
            text.push_str(&" ".repeat(left));
            text.push_str(&chunk);
            col += left + bw;
        }
    }
    rows.into_iter().map(|(r, t)| (r.trim_end().to_string(), t.trim_end().to_string())).collect()
}

// ── Frame ────────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &mut App) {
    let p = pal();
    let area = f.area();
    app.narrow = area.width < NARROW;
    app.areas = Default::default();

    let player_h: u16 = if area.height >= 26 && area.width >= 60 { 7 } else { 4 };
    let [header, body, footer] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    draw_header(f, app, header, p);

    if app.now_view {
        draw_now_view(f, app, body, p);
    } else {
        let [main, player] = Layout::vertical([Constraint::Fill(1), Constraint::Length(player_h)]).areas(body);
        if app.narrow {
            draw_narrow(f, app, main, p);
        } else {
            draw_wide(f, app, main, p);
        }
        draw_player(f, app, player, p);
    }
    draw_footer(f, app, footer, p);
    if app.modal.is_some() {
        draw_modal(f, app, area, p);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect, p: Pal) {
    let title = Span::styled(" ♪ Tort's Music Player ", Style::default().fg(p.accent).add_modifier(Modifier::BOLD));
    let output = match app.output {
        Output::Laptop => Span::styled(" laptop ", Style::default().fg(p.sub)),
        Output::Phone => Span::styled(" ▶ phone ", Style::default().fg(Color::Black).bg(p.accent)),
    };
    let status = if !app.status.text.is_empty() {
        Span::styled(app.status.text.clone(), Style::default().fg(if app.status.error { p.red } else { p.green }))
    } else if let Some(prog) = app.import_progress() {
        Span::styled(prog, Style::default().fg(p.sub))
    } else {
        Span::raw("")
    };
    let used = width(&title.content) + width(&output.content) + 2;
    let room = (area.width as usize).saturating_sub(used);
    let status = Span::styled(fit(&status.content, room), status.style);
    let gap = room.saturating_sub(width(&status.content));
    let line = if app.narrow && !status.content.is_empty() {
        // On a phone, the message matters more than the name.
        let room = (area.width as usize).saturating_sub(width(&output.content) + 1);
        let s = Span::styled(fit(&app.status.text, room), status.style);
        let gap = room.saturating_sub(width(&s.content));
        Line::from(vec![Span::raw(" "), s, Span::raw(" ".repeat(gap)), output])
    } else {
        Line::from(vec![title, Span::raw(" "), status, Span::raw(" ".repeat(gap)), output, Span::raw(" ")])
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect, p: Pal) {
    let hints: Vec<(&str, &str)> = match &app.modal {
        Some(Modal::YouTube(d)) if d.typing => vec![("enter", "search"), ("↓", "results"), ("esc", "close")],
        Some(Modal::YouTube(_)) => vec![("enter", "download"), ("/", "new search"), ("esc", "close")],
        Some(Modal::Tags(_)) => vec![("↑↓", "field"), ("enter", "save"), ("esc", "cancel")],
        Some(Modal::Settings(_)) => vec![("↑↓", "setting"), ("←→", "change"), ("esc", "close")],
        Some(Modal::AddToPlaylist { .. }) => vec![("enter", "add"), ("esc", "cancel")],
        Some(_) => vec![("enter", "ok"), ("esc", "close")],
        None if app.searching => vec![("type", "to filter"), ("enter", "done"), ("esc", "clear")],
        None => {
            let mut h = vec![("space", "play"), ("n/p", "next/prev"), ("←→", "seek"), ("enter", "open")];
            if app.narrow {
                h.insert(0, ("tab", "pane"));
            }
            h.extend([("/", "search"), ("k", "lyrics"), ("o", "phone"), ("y", "YouTube"), ("?", "help"), ("q", "quit")]);
            h
        }
    };
    let mut spans = Vec::new();
    let mut used = 1;
    for (k, what) in hints {
        let n = width(k) + width(what) + 3;
        if used + n > area.width as usize {
            break;
        }
        spans.push(Span::styled(format!(" {k}"), Style::default().fg(p.accent)));
        spans.push(Span::styled(format!(" {what} "), Style::default().fg(p.dim)));
        used += n;
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn pane_block(title: Line<'static>, focused: bool, p: Pal) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(if focused { p.accent } else { p.surface }))
        .title(title)
}

fn draw_wide(f: &mut Frame, app: &mut App, area: Rect, p: Pal) {
    let lyrics_w = if app.show_lyrics { (area.width * 2 / 5).clamp(30, 60) } else { 0 };
    let [nav, list, lyrics] = Layout::horizontal([Constraint::Length(24), Constraint::Fill(1), Constraint::Length(lyrics_w)]).areas(area);
    draw_nav(f, app, nav, p);
    draw_list(f, app, list, p);
    if app.show_lyrics {
        draw_lyrics(f, app, lyrics, p, true);
    }
}

fn draw_narrow(f: &mut Frame, app: &mut App, area: Rect, p: Pal) {
    let [tabs, pane] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let mut panes = vec![(Pane::Nav, "Library"), (Pane::List, "Songs")];
    if app.show_lyrics {
        panes.push((Pane::Lyrics, "Lyrics"));
    }
    let mut x = tabs.x + 1;
    let mut spans = vec![Span::raw(" ")];
    for (pane, label) in panes {
        let text = format!(" {label} ");
        let w = width(&text) as u16;
        let style = if app.pane == pane { Style::default().fg(Color::Black).bg(p.accent) } else { Style::default().fg(p.sub) };
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" "));
        app.areas.tabs.push((Rect::new(x, tabs.y, w, 1), pane));
        x += w + 1;
    }
    f.render_widget(Paragraph::new(Line::from(spans)), tabs);
    match app.pane {
        Pane::Nav => draw_nav(f, app, pane, p),
        Pane::List => draw_list(f, app, pane, p),
        Pane::Lyrics => draw_lyrics(f, app, pane, p, true),
    }
}

fn draw_nav(f: &mut Frame, app: &mut App, area: Rect, p: Pal) {
    let focused = app.pane == Pane::Nav;
    let block = pane_block(Line::from(" Library "), focused, p);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.areas.nav = inner;

    // Rows: the fixed entries, a "Playlists" heading, then the playlists.
    let first_pl = app.first_playlist_nav();
    let mut rows: Vec<(Line<'static>, Option<usize>)> = Vec::new();
    for (i, nav) in app.nav.iter().enumerate() {
        if i == first_pl {
            rows.push((Line::from(Span::styled(" PLAYLISTS", Style::default().fg(p.dim))), None));
        }
        let selected = i == app.nav_cursor;
        let label = fit(&nav.label(), inner.width.saturating_sub(3) as usize);
        let style = if selected && focused {
            Style::default().fg(Color::Black).bg(p.accent)
        } else if selected {
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.text)
        };
        rows.push((Line::from(Span::styled(format!(" {}", pad(&label, inner.width.saturating_sub(1) as usize)), style)), Some(i)));
    }
    if first_pl == app.nav.len() {
        rows.push((Line::from(Span::styled(" PLAYLISTS", Style::default().fg(p.dim))), None));
        rows.push((Line::from(Span::styled(" press c to create one", Style::default().fg(p.dim))), None));
    }
    let h = inner.height as usize;
    let cursor_row = rows.iter().position(|(_, i)| *i == Some(app.nav_cursor)).unwrap_or(0);
    let offset = scroll(app.areas.nav_offset, cursor_row, h, rows.len());
    app.areas.nav_offset = offset;
    let shown: Vec<Line> = rows.iter().skip(offset).take(h).map(|(l, _)| l.clone()).collect();
    for (row, (_, idx)) in rows.iter().skip(offset).take(h).enumerate() {
        if let Some(i) = idx {
            app.areas.nav_rows.push((inner.y + row as u16, *i));
        }
    }
    f.render_widget(Paragraph::new(shown), inner);
}

/// The first visible row so that `cursor` stays on screen.
fn scroll(offset: usize, cursor: usize, height: usize, len: usize) -> usize {
    if height == 0 {
        return 0;
    }
    let mut offset = offset;
    if cursor < offset {
        offset = cursor;
    } else if cursor >= offset + height {
        offset = cursor + 1 - height;
    }
    offset.min(len.saturating_sub(height))
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect, p: Pal) {
    let focused = app.pane == Pane::List;
    let count = app.items.len();
    let mut title = vec![Span::styled(format!(" {} ", app.list_title()), Style::default().fg(p.text).add_modifier(Modifier::BOLD))];
    title.push(Span::styled(format!("{count} "), Style::default().fg(p.dim)));
    let block = pane_block(Line::from(title), focused, p);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.areas.list = inner;

    let mut y = inner.y;
    // Search line.
    if app.searching || !app.query.text.is_empty() {
        let line = Line::from(vec![
            Span::styled(" / ", Style::default().fg(p.accent)),
            Span::styled(app.query.text.clone(), Style::default().fg(p.text)),
        ]);
        f.render_widget(Paragraph::new(line), Rect::new(inner.x, y, inner.width, 1));
        if app.searching && app.modal.is_none() {
            let before: String = app.query.text.chars().take(app.query.cursor).collect();
            f.set_cursor_position(Position::new(inner.x + 3 + width(&before) as u16, y));
        }
        y += 1;
    }
    let w = inner.width as usize;
    let wide = w >= 70;
    let plays_w = if w >= 90 { 7 } else { 0 };
    let (title_w, artist_w, album_w) = if wide {
        let rest = w.saturating_sub(4 + 7 + 2 + plays_w);
        (rest * 2 / 5, rest * 3 / 10, rest - rest * 2 / 5 - rest * 3 / 10)
    } else {
        (w.saturating_sub(8), 0, 0)
    };
    if wide && matches!(app.items, Items::Tracks(_)) && y < inner.bottom() {
        let plays = if plays_w > 0 { format!("{:>7}", "Plays") } else { String::new() };
        let head = format!("    {}{}{}  {:>5}{plays}", pad("Title", title_w), pad("Artist", artist_w), pad("Album", album_w), "Time");
        f.render_widget(Paragraph::new(Span::styled(fit(&head, w), Style::default().fg(p.dim))), Rect::new(inner.x, y, inner.width, 1));
        y += 1;
    }
    let rows_area = Rect::new(inner.x, y, inner.width, inner.bottom().saturating_sub(y));
    app.areas.list_rows = rows_area;
    let h = rows_area.height as usize;
    let offset = scroll(app.areas.list_offset, app.cursor, h, count);
    app.areas.list_offset = offset;

    let mut lines: Vec<Line> = Vec::new();
    match &app.items {
        Items::Tracks(list) if list.is_empty() => {
            let msg = if app.tracks.is_empty() {
                format!("No music found in {}", crate::config::get().music_dir)
            } else if !app.query.text.is_empty() {
                "No songs match your search".to_string()
            } else {
                "Nothing here yet".to_string()
            };
            lines.push(Line::from(Span::styled(format!(" {msg}"), Style::default().fg(p.dim))));
        }
        Items::Tracks(list) => {
            for (row, &ti) in list.iter().enumerate().skip(offset).take(h) {
                let t = &app.tracks[ti];
                let playing = app.current == Some(ti);
                let selected = row == app.cursor;
                let marker = if playing {
                    if app.state == PlaybackState::Playing { "▶" } else { "‖" }
                } else if app.is_liked(ti) {
                    "♥"
                } else {
                    " "
                };
                let base = if selected && focused {
                    Style::default().fg(Color::Black).bg(p.accent)
                } else if selected {
                    Style::default().bg(p.surface)
                } else {
                    Style::default()
                };
                let fg = |c: Color| if selected && focused { base } else { base.fg(c) };
                let title_style = if playing { fg(p.accent).add_modifier(Modifier::BOLD) } else { fg(p.text) };
                let time = fmt_time(t.duration);
                if wide {
                    lines.push(Line::from(vec![
                        Span::styled(format!(" {marker}  "), fg(p.accent)),
                        Span::styled(pad(&t.title, title_w), title_style),
                        Span::styled(pad(&t.artist, artist_w), fg(p.sub)),
                        Span::styled(pad(&t.album, album_w), fg(p.dim)),
                        Span::styled(format!("  {time:>5}"), fg(p.dim)),
                        Span::styled(if plays_w > 0 { format!("{:>7}", app.play_count(ti)) } else { String::new() }, fg(p.dim)),
                    ]));
                } else {
                    let label = if t.artist.is_empty() { t.title.clone() } else { format!("{} · {}", t.title, t.artist) };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{marker} "), fg(p.accent)),
                        Span::styled(pad(&label, title_w), title_style),
                        Span::styled(format!(" {time:>5}"), fg(p.dim)),
                    ]));
                }
            }
        }
        Items::Groups(_, groups) => {
            for (row, (name, n)) in groups.iter().enumerate().skip(offset).take(h) {
                let selected = row == app.cursor;
                let style = if selected && focused {
                    Style::default().fg(Color::Black).bg(p.accent)
                } else if selected {
                    Style::default().bg(p.surface).fg(p.text)
                } else {
                    Style::default().fg(p.text)
                };
                let name = if name.is_empty() { "(none)" } else { name };
                let count = format!("{n} ");
                let label = pad(&format!(" {name}"), w.saturating_sub(width(&count)));
                lines.push(Line::from(vec![Span::styled(label, style), Span::styled(count, if selected && focused { style } else { Style::default().fg(p.dim) })]));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), rows_area);
}

// ── Lyrics ───────────────────────────────────────────────────────────────────

fn draw_lyrics(f: &mut Frame, app: &mut App, area: Rect, p: Pal, bordered: bool) {
    let focused = app.pane == Pane::Lyrics;
    let mut title = vec![Span::styled(" Lyrics ", Style::default().fg(p.text).add_modifier(Modifier::BOLD))];
    if let Some(track) = app.current_track() {
        if app.is_japanese_song(track) {
            title.push(Span::styled(if app.romaji { "romaji " } else { "日本語 " }, Style::default().fg(p.accent)));
        }
    }
    if app.lyrics_offset != 0.0 {
        title.push(Span::styled(format!("{:+.1}s ", app.lyrics_offset), Style::default().fg(p.dim)));
    }
    let inner = if bordered {
        let block = pane_block(Line::from(title), focused, p);
        let inner = block.inner(area);
        f.render_widget(block, area);
        inner
    } else {
        area
    };
    app.areas.lyrics = inner;
    let Some(track) = app.current_track().cloned() else {
        note(f, inner, "Nothing playing", "", p);
        return;
    };
    let (shown, alt_note) = app.shown_lyrics(&track);
    let (lines, synced) = match shown {
        Some(Lyrics::Synced(l)) => (l.clone(), true),
        Some(Lyrics::Plain(l)) => (l.clone(), false),
        Some(Lyrics::Instrumental) => return note(f, inner, "♪ Instrumental", "", p),
        Some(Lyrics::NotFound) => {
            return note(f, inner, "No lyrics found", "To add your own, save a .lrc file next to the song, with the same name.", p)
        }
        Some(Lyrics::Error(e)) => return note(f, inner, "Couldn't load lyrics", &e.clone(), p),
        None => return note(f, inner, "Looking for lyrics…", "", p),
    };
    let ruby = app.furigana_for(&lines).cloned();
    let active = if synced { active_line(&lines, app.lyric_position()) } else { None };
    let target = app.lyric_cursor.or(active).unwrap_or(0);

    // Every screen row, with the lyric line it belongs to.
    let w = inner.width.saturating_sub(2) as usize;
    let mut rows: Vec<(Line<'static>, usize)> = Vec::new();
    if let Some(n) = &alt_note {
        rows.push((Line::from(Span::styled(fit(n, w), Style::default().fg(p.dim))), usize::MAX));
    }
    for (i, line) in lines.iter().enumerate() {
        let text = if line.text.trim().is_empty() { if synced { "♪".to_string() } else { String::new() } } else { line.text.clone() };
        let style = if Some(i) == active {
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
        } else if synced && active.is_some_and(|a| i < a) {
            Style::default().fg(p.dim)
        } else if synced {
            Style::default().fg(p.sub)
        } else {
            Style::default().fg(p.text)
        };
        let style = if app.lyric_cursor == Some(i) && focused { style.bg(p.surface) } else { style };
        let reading_style = Style::default().fg(if Some(i) == active { p.sub } else { p.dim });
        match ruby.as_ref().and_then(|r| r.get(i)).filter(|pieces| pieces.iter().any(|p| p.reading.is_some())) {
            Some(pieces) => {
                for (readings, base) in ruby_rows(pieces, w) {
                    rows.push((Line::from(Span::styled(format!(" {readings}"), reading_style)), i));
                    rows.push((Line::from(Span::styled(format!(" {base}"), style)), i));
                }
            }
            None => {
                for part in wrap(&text, w) {
                    rows.push((Line::from(Span::styled(format!(" {part}"), style)), i));
                }
            }
        }
        if ruby.is_some() && i + 1 < lines.len() {
            rows.push((Line::from(""), i));
        }
    }
    // Keep the line being sung (or the selected one) a third of the way down.
    let h = inner.height as usize;
    let target_row = rows.iter().position(|(_, i)| *i == target).unwrap_or(0);
    let offset = if synced || app.lyric_cursor.is_some() {
        target_row.saturating_sub(h / 3).min(rows.len().saturating_sub(h))
    } else {
        0
    };
    for (row, (_, i)) in rows.iter().skip(offset).take(h).enumerate() {
        if *i != usize::MAX {
            app.areas.lyric_rows.push((inner.y + row as u16, *i));
        }
    }
    let shown: Vec<Line> = rows.into_iter().skip(offset).take(h).map(|(l, _)| l).collect();
    f.render_widget(Paragraph::new(shown), inner);
}

/// The last line that has started at `at`.
pub fn active_line(lines: &[LyricLine], at: Duration) -> Option<usize> {
    lines.partition_point(|l| l.time <= at).checked_sub(1)
}

fn note(f: &mut Frame, area: Rect, title: &str, detail: &str, p: Pal) {
    let w = area.width.saturating_sub(2) as usize;
    let mut lines = vec![Line::from(""), Line::from(Span::styled(fit(title, w), Style::default().fg(p.text).add_modifier(Modifier::BOLD))).centered()];
    for part in wrap(detail, w) {
        lines.push(Line::from(Span::styled(part, Style::default().fg(p.dim))).centered());
    }
    let top = area.height.saturating_sub(lines.len() as u16) / 3;
    let area = Rect::new(area.x, area.y + top, area.width, area.height.saturating_sub(top));
    f.render_widget(Paragraph::new(lines), area);
}

// ── Player ───────────────────────────────────────────────────────────────────

fn draw_player(f: &mut Frame, app: &mut App, area: Rect, p: Pal) {
    let block = Block::default().borders(Borders::TOP).border_style(Style::default().fg(p.surface));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let big = inner.height >= 5;
    let cover_w = if big && app.cover.is_some() { inner.height * 2 } else { 0 };
    let viz_w = if app.show_spectrum && inner.width >= 90 { inner.width / 3 } else { 0 };
    let [cover, info, viz] =
        Layout::horizontal([Constraint::Length(cover_w + u16::from(cover_w > 0)), Constraint::Fill(1), Constraint::Length(viz_w)]).areas(inner);
    if let Some((_, img)) = &app.cover {
        if cover_w > 0 {
            let lines = crate::art::half_blocks(img, cover_w, inner.height);
            f.render_widget(Paragraph::new(lines), Rect::new(cover.x, cover.y, cover_w, inner.height));
        }
    }
    draw_now_info(f, app, info, p, big);
    if viz_w > 0 {
        draw_spectrum(f, app, viz, p);
    }
}

/// Title, artist, progress and switches.
fn draw_now_info(f: &mut Frame, app: &mut App, area: Rect, p: Pal, big: bool) {
    let area = Rect::new(area.x + 1, area.y, area.width.saturating_sub(1), area.height);
    let w = area.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    match (app.current_track(), big) {
        (Some(t), true) => {
            lines.push(Line::from(Span::styled(fit(&t.title, w), Style::default().fg(p.text).add_modifier(Modifier::BOLD))));
            let who = [t.artist.as_str(), t.album.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" — ");
            lines.push(Line::from(Span::styled(fit(&who, w), Style::default().fg(p.sub))));
            lines.push(Line::from(""));
        }
        (Some(t), false) => {
            let label = if t.artist.is_empty() { t.title.clone() } else { format!("{} · {}", t.title, t.artist) };
            lines.push(Line::from(Span::styled(fit(&label, w), Style::default().fg(p.text).add_modifier(Modifier::BOLD))));
        }
        (None, _) => {
            lines.push(Line::from(Span::styled("Nothing playing", Style::default().fg(p.sub))));
            if big {
                lines.push(Line::from(Span::styled(fit("Pick a song and press Enter", w), Style::default().fg(p.dim))));
                lines.push(Line::from(""));
            }
        }
    }

    // Progress bar (clickable).
    let icon = match app.state {
        PlaybackState::Playing => "▶",
        PlaybackState::Paused => "‖",
        PlaybackState::Stopped => "■",
    };
    let pos = fmt_time(app.position);
    let dur = if app.duration > Duration::ZERO { fmt_time(app.duration) } else { "--:--".into() };
    let lead = format!("{icon} {pos} ");
    let bar_w = w.saturating_sub(width(&lead) + width(&dur) + 1);
    let frac = if app.duration > Duration::ZERO { (app.position.as_secs_f64() / app.duration.as_secs_f64()).clamp(0.0, 1.0) } else { 0.0 };
    let filled = ((frac * bar_w as f64).round() as usize).min(bar_w);
    app.areas.progress = Rect::new(area.x + width(&lead) as u16, area.y + lines.len() as u16, bar_w as u16, 1);
    lines.push(Line::from(vec![
        Span::styled(lead, Style::default().fg(p.accent)),
        Span::styled("━".repeat(filled), Style::default().fg(p.accent)),
        Span::styled("─".repeat(bar_w - filled), Style::default().fg(p.surface)),
        Span::styled(format!(" {dur}"), Style::default().fg(p.sub)),
    ]));

    // Switches.
    let on = |b: bool| Style::default().fg(if b { p.accent } else { p.dim });
    let mut flags = vec![Span::styled("shuffle", on(app.shuffle)), Span::raw("  "), Span::styled("repeat", on(app.repeat)), Span::raw("  ")];
    match app.output {
        Output::Laptop => flags.push(Span::styled(format!("vol {}%", (app.volume * 100.0).round()), Style::default().fg(p.sub))),
        Output::Phone => flags.push(Span::styled("on phone", Style::default().fg(p.accent))),
    }
    if app.current.is_some_and(|i| app.is_liked(i)) {
        flags.push(Span::styled("  ♥", Style::default().fg(p.red)));
    }
    lines.push(Line::from(flags));
    f.render_widget(Paragraph::new(lines), area);
}

const EIGHTHS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

fn draw_spectrum(f: &mut Frame, app: &App, area: Rect, p: Pal) {
    if app.output == Output::Phone {
        note(f, area, "Playing on your phone", "", p);
        return;
    }
    let (cols, h) = (area.width as usize, area.height as usize);
    if cols == 0 || h == 0 {
        return;
    }
    let bins = &app.spectrum;
    // One bar per two columns, sampled across the bins.
    let bars = cols.div_ceil(2);
    let heights: Vec<usize> = (0..bars).map(|b| (bins[b * bins.len() / bars].clamp(0.0, 1.0) * (h * 8) as f32).round() as usize).collect();
    let mut lines = Vec::with_capacity(h);
    for row in 0..h {
        let mut s = String::with_capacity(cols * 3);
        for (b, &eighths) in heights.iter().enumerate() {
            let cell = if app.spectrum_mirrored {
                // Grows from the middle out, both ways, in whole cells.
                let cells = ((eighths as f32 / 8.0).round() as usize).min(h);
                let top = (h - cells) / 2;
                if row >= top && row < top + cells { "█" } else { " " }
            } else {
                let from_bottom = h - 1 - row;
                let level = eighths.saturating_sub(from_bottom * 8).min(8);
                EIGHTHS[level]
            };
            s.push_str(cell);
            if b * 2 + 1 < cols {
                s.push(' ');
            }
        }
        lines.push(Line::from(Span::styled(s, Style::default().fg(p.accent))));
    }
    f.render_widget(Paragraph::new(lines), area);
}

// ── Now playing view ─────────────────────────────────────────────────────────

fn draw_now_view(f: &mut Frame, app: &mut App, area: Rect, p: Pal) {
    if app.narrow {
        // Cover on top (if there's room), then the song, then lyrics.
        let cover_h = if area.height >= 30 { (area.width / 2).min(area.height / 3) } else { 0 };
        let [cover, info, lyrics] = Layout::vertical([Constraint::Length(cover_h), Constraint::Length(5), Constraint::Fill(1)]).areas(area);
        if let (Some((_, img)), true) = (&app.cover, cover_h > 0) {
            let cw = cover_h * 2;
            let x = cover.x + (cover.width.saturating_sub(cw)) / 2;
            f.render_widget(Paragraph::new(crate::art::half_blocks(img, cw, cover_h)), Rect::new(x, cover.y, cw.min(cover.width), cover_h));
        }
        draw_now_info(f, app, info, p, true);
        if app.show_lyrics {
            draw_lyrics(f, app, lyrics, p, true);
        }
        return;
    }
    let left_w = (area.width * 2 / 5).max(30);
    let [left, right] = Layout::horizontal([Constraint::Length(left_w), Constraint::Fill(1)]).areas(area);
    let info_h = 6;
    let cover_h = left.height.saturating_sub(info_h + 1).min(left.width.saturating_sub(4) / 2);
    let [cover, info, viz] = Layout::vertical([Constraint::Length(cover_h + 1), Constraint::Length(info_h), Constraint::Fill(1)]).areas(left);
    if let Some((_, img)) = &app.cover {
        let cw = cover_h * 2;
        let x = cover.x + cover.width.saturating_sub(cw) / 2;
        f.render_widget(Paragraph::new(crate::art::half_blocks(img, cw, cover_h)), Rect::new(x, cover.y + 1, cw, cover_h));
    }
    draw_now_info(f, app, Rect::new(info.x + 2, info.y, info.width.saturating_sub(4), info.height), p, true);
    if app.show_spectrum && viz.height > 0 {
        draw_spectrum(f, app, Rect::new(viz.x + 2, viz.y, viz.width.saturating_sub(4), viz.height), p);
    }
    draw_lyrics(f, app, right, p, true);
}

// ── Dialogs ──────────────────────────────────────────────────────────────────

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2)).max(10.min(area.width));
    let h = h.min(area.height.saturating_sub(2)).max(3.min(area.height));
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn dialog(f: &mut Frame, area: Rect, title: &str, p: Pal) -> Rect {
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(p.accent))
        .title(Span::styled(format!(" {title} "), Style::default().fg(p.text).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), inner.height)
}

fn input_line(f: &mut Frame, area: Rect, input: &crate::app::Input, p: Pal, cursor: bool) {
    let w = area.width as usize;
    // Scroll long text so the cursor stays visible.
    let before: String = input.text.chars().take(input.cursor).collect();
    let skip = width(&before).saturating_sub(w.saturating_sub(2));
    let mut shown = String::new();
    let mut col = 0;
    for c in input.text.chars() {
        let cw = c.width().unwrap_or(0);
        if col >= skip && width(&shown) + cw <= w.saturating_sub(1) {
            shown.push(c);
        }
        col += cw;
    }
    f.render_widget(Paragraph::new(Span::styled(pad(&shown, w), Style::default().fg(p.text).bg(p.surface))), Rect::new(area.x, area.y, area.width, 1));
    if cursor {
        let x = area.x + (width(&before) - skip) as u16;
        f.set_cursor_position(Position::new(x.min(area.right().saturating_sub(1)), area.y));
    }
}

fn draw_modal(f: &mut Frame, app: &mut App, area: Rect, p: Pal) {
    let Some(modal) = &app.modal else { return };
    let sel = Style::default().fg(Color::Black).bg(p.accent);
    match modal {
        Modal::Help => {
            let keys: &[(&str, &str)] = &[
                ("space", "play / pause"),
                ("n  p", "next / previous song"),
                ("← →", "seek (shift: 30 s)"),
                ("+ -", "volume"),
                ("s  r", "shuffle / repeat this song"),
                ("enter", "play song / open artist, album…"),
                ("esc ⌫", "back / clear search"),
                ("/", "search"),
                ("tab", "next pane (1 2 3 jump)"),
                ("t", "now playing view"),
                ("l", "like / unlike"),
                ("e", "edit tags"),
                ("k", "lyrics panel"),
                ("j", "Japanese ⇄ romaji"),
                ("g", "furigana on / off"),
                ("[  ]", "lyrics 0.5 s earlier / later"),
                ("0", "reset lyrics timing"),
                ("S", "sync selected lyric line to now"),
                ("c", "new playlist"),
                ("a", "add song to playlist"),
                ("d", "remove song from playlist"),
                ("R  D", "rename / delete playlist"),
                ("x  i", "export / import playlist (CSV)"),
                ("y", "search & download from YouTube"),
                ("o  O", "play on phone ⇄ here / phone code"),
                ("v", "visualiser"),
                (",", "settings"),
                ("F5", "rescan library"),
                ("q", "quit"),
            ];
            let r = centered(area, 50, keys.len() as u16 + 3);
            let inner = dialog(f, r, "Keys", p);
            let lines: Vec<Line> = keys
                .iter()
                .map(|(k, what)| Line::from(vec![Span::styled(format!("{k:<7}"), Style::default().fg(p.accent)), Span::styled(*what, Style::default().fg(p.text))]))
                .collect();
            let mut lines = lines;
            lines.push(Line::from(Span::styled("Mouse: click, double-click, scroll", Style::default().fg(p.dim))));
            f.render_widget(Paragraph::new(lines), inner);
        }
        Modal::Prompt(pr) => {
            let r = centered(area, 64, 6);
            let inner = dialog(f, r, &pr.title, p);
            f.render_widget(Paragraph::new(Span::styled(fit(&pr.hint, inner.width as usize), Style::default().fg(p.sub))), Rect::new(inner.x, inner.y, inner.width, 1));
            input_line(f, Rect::new(inner.x, inner.y + 2, inner.width, 1), &pr.input, p, true);
        }
        Modal::ConfirmDelete(name) => {
            let r = centered(area, 50, 5);
            let inner = dialog(f, r, "Delete playlist", p);
            let lines = vec![
                Line::from(Span::styled(fit(&format!("Delete “{name}”?"), inner.width as usize), Style::default().fg(p.text))),
                Line::from(Span::styled("The songs stay in your library.  y / n", Style::default().fg(p.dim))),
            ];
            f.render_widget(Paragraph::new(lines), inner);
        }
        Modal::AddToPlaylist { cursor, .. } => {
            let mut options = app.playlists();
            options.push("+ New playlist…".into());
            let r = centered(area, 44, options.len() as u16 + 3);
            let inner = dialog(f, r, "Add to playlist", p);
            let h = inner.height as usize;
            let offset = scroll(app.areas.modal_offset, *cursor, h, options.len());
            app.areas.modal_offset = offset;
            app.areas.modal_rows = inner;
            let lines: Vec<Line> = options
                .iter()
                .enumerate()
                .skip(offset)
                .take(h)
                .map(|(i, o)| Line::from(Span::styled(pad(o, inner.width as usize), if i == *cursor { sel } else { Style::default().fg(p.text) })))
                .collect();
            f.render_widget(Paragraph::new(lines), inner);
        }
        Modal::YouTube(d) => {
            let h = (d.results.len() as u16 + 6).clamp(9, area.height.saturating_sub(4).max(9));
            let r = centered(area, 90, h);
            let inner = dialog(f, r, "Search YouTube", p);
            input_line(f, Rect::new(inner.x, inner.y, inner.width, 1), &d.input, p, d.typing);
            let mut y = inner.y + 2;
            let msg = if d.searching {
                Some("Searching…".to_string())
            } else if d.message.is_none() && d.results.is_empty() {
                Some("Type a song or an artist and press Enter. Songs you pick download into your library.".to_string())
            } else {
                d.message.clone()
            };
            if let Some(m) = msg {
                f.render_widget(Paragraph::new(Span::styled(fit(&m, inner.width as usize), Style::default().fg(p.sub))), Rect::new(inner.x, y, inner.width, 1));
                y += 1;
            }
            let rows = Rect::new(inner.x, y, inner.width, inner.bottom().saturating_sub(y));
            app.areas.modal_rows = rows;
            let h = rows.height as usize;
            let offset = scroll(app.areas.modal_offset, d.cursor, h, d.results.len());
            app.areas.modal_offset = offset;
            let w = rows.width as usize;
            let lines: Vec<Line> = d
                .results
                .iter()
                .enumerate()
                .skip(offset)
                .take(h)
                .map(|(i, r)| {
                    let state = if d.saved.contains(&r.url) {
                        "✓ saved"
                    } else if d.downloading.contains(&r.url) {
                        "downloading"
                    } else {
                        ""
                    };
                    let right = format!(" {:>6} {:>11}", r.duration, state);
                    let left_w = w.saturating_sub(width(&right));
                    let who = if r.uploader.is_empty() { String::new() } else { format!("  {}", r.uploader) };
                    let style = if i == d.cursor && !d.typing { sel } else { Style::default().fg(p.text) };
                    let who_style = if i == d.cursor && !d.typing { sel } else { Style::default().fg(p.dim) };
                    let title = fit(&r.title, left_w);
                    let who = fit(&who, left_w.saturating_sub(width(&title)));
                    let gap = left_w.saturating_sub(width(&title) + width(&who));
                    Line::from(vec![
                        Span::styled(title, style),
                        Span::styled(who, who_style),
                        Span::styled(" ".repeat(gap), style),
                        Span::styled(right, if state == "✓ saved" { Style::default().fg(p.green) } else { who_style }),
                    ])
                })
                .collect();
            f.render_widget(Paragraph::new(lines), rows);
        }
        Modal::Tags(ed) => {
            let r = centered(area, 70, TAG_FIELDS.len() as u16 + 5);
            let inner = dialog(f, r, "Edit tags", p);
            for (i, label) in TAG_FIELDS.iter().enumerate() {
                let y = inner.y + i as u16;
                let focused = i == ed.cursor;
                f.render_widget(
                    Paragraph::new(Span::styled(format!("{label:<8}"), Style::default().fg(if focused { p.accent } else { p.sub }))),
                    Rect::new(inner.x, y, 8, 1),
                );
                input_line(f, Rect::new(inner.x + 8, y, inner.width.saturating_sub(8), 1), &ed.fields[i], p, focused);
            }
            let y = inner.y + TAG_FIELDS.len() as u16 + 1;
            let (msg, style) = match &ed.error {
                Some(e) => (e.clone(), Style::default().fg(p.red)),
                None => ("Enter saves the tags into the file".to_string(), Style::default().fg(p.dim)),
            };
            f.render_widget(Paragraph::new(Span::styled(fit(&msg, inner.width as usize), style)), Rect::new(inner.x, y, inner.width, 1));
        }
        Modal::Settings(st) => {
            let r = centered(area, 56, App::SETTINGS.len() as u16 + 4);
            let inner = dialog(f, r, "Settings", p);
            let mut lines: Vec<Line> = App::SETTINGS
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let style = if i == st.cursor { sel } else { Style::default().fg(p.text) };
                    let value = format!("‹ {} ›", app.settings_value(i));
                    let gap = (inner.width as usize).saturating_sub(width(label) + width(&value));
                    Line::from(vec![Span::styled(format!("{label}{}", " ".repeat(gap)), style), Span::styled(value, style)])
                })
                .collect();
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Saved to ~/.config/lavanda/config.toml", Style::default().fg(p.dim))));
            f.render_widget(Paragraph::new(lines), inner);
        }
        Modal::Phone => {
            let w = (area.width.min(96) as usize).saturating_sub(4);
            let (tailnet, local): (Vec<&String>, Vec<&String>) = app.phone_addresses.iter().partition(|u| !u.starts_with("http://localhost"));
            let code = app.phone_code.as_deref().map(|c| format!("{} {}", &c[..3], &c[3..])).unwrap_or_default();
            let mut lines = vec![Line::from(Span::styled("On your phone, open in Safari (or any browser):", Style::default().fg(p.text)))];
            let addr = |lines: &mut Vec<Line<'static>>, u: &str| {
                for part in wrap(u, w) {
                    lines.push(Line::from(Span::styled(part, Style::default().fg(p.accent).add_modifier(Modifier::BOLD))));
                }
            };
            match tailnet.first() {
                Some(u) => {
                    addr(&mut lines, u);
                    if let Some(ip) = tailnet.get(1) {
                        lines.push(Line::from(Span::styled(fit(&format!("(or {ip})"), w), Style::default().fg(p.dim))));
                    }
                }
                None => {
                    if let Some(u) = local.first() {
                        addr(&mut lines, u);
                    }
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("and enter the code  ", Style::default().fg(p.text)),
                Span::styled(format!(" {code} "), Style::default().fg(Color::Black).bg(p.accent).add_modifier(Modifier::BOLD)),
            ]));
            for part in wrap("It works for 10 minutes. Then add the page to your Home Screen.", w) {
                lines.push(Line::from(Span::styled(part, Style::default().fg(p.dim))));
            }
            lines.push(Line::from(""));
            if !tailnet.is_empty() {
                if let Some(u) = local.first() {
                    let port = crate::config::get().phone_port;
                    for part in wrap(&format!("Without Tailscale: SSH with -L {port}:127.0.0.1:{port}, then {u}"), w) {
                        lines.push(Line::from(Span::styled(part, Style::default().fg(p.dim))));
                    }
                }
            } else {
                for part in wrap("That's through an SSH tunnel; with Tailscale it works without one.", w) {
                    lines.push(Line::from(Span::styled(part, Style::default().fg(p.dim))));
                }
            }
            let seen = match app.phone_seen {
                Some(t) if t.elapsed() < Duration::from_secs(10) => Span::styled("Phone connected", Style::default().fg(p.green)),
                Some(_) => Span::styled("Phone not seen recently (its screen may be locked)", Style::default().fg(p.sub)),
                None => Span::styled("Waiting for the phone…", Style::default().fg(p.sub)),
            };
            lines.push(Line::from(seen));
            lines.push(Line::from(Span::styled(fit("o moves the music back to this computer.", w), Style::default().fg(p.dim))));
            let r = centered(area, 96, lines.len() as u16 + 2);
            let inner = dialog(f, r, "Play on your phone", p);
            f.render_widget(Paragraph::new(lines), inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ruby(text: &str, reading: Option<&str>) -> Ruby {
        Ruby { text: text.into(), reading: reading.map(Into::into) }
    }

    #[test]
    fn fitting_and_wrapping_count_wide_characters_as_two() {
        assert_eq!(width("夜空"), 4);
        assert_eq!(fit("夜空に星", 5), "夜空…");
        assert_eq!(fit("abc", 5), "abc");
        assert_eq!(pad("ab", 4), "ab  ");
        assert_eq!(wrap("夜空に星が光る", 6), vec!["夜空に", "星が光", "る"]);
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
    }

    #[test]
    fn furigana_sit_over_their_kanji() {
        // Columns of a reading's centre and of its kanji's centre agree.
        fn centres(row: &(String, String), reading: &str, kanji: &str) -> (usize, usize) {
            let r = width(&row.0[..row.0.find(reading).unwrap()]) + width(reading) / 2;
            let k = width(&row.1[..row.1.find(kanji).unwrap()]) + width(kanji) / 2;
            (r, k)
        }
        // 光(ひかり)の海(うみ)を越(こ)えて: ひかり overhangs の; うみ would
        // touch it, so 海 moves one column right; こ stays over 越.
        let line = vec![ruby("光", Some("ひかり")), ruby("の", None), ruby("海", Some("うみ")), ruby("を", None), ruby("越", Some("こ")), ruby("えて", None)];
        let rows = ruby_rows(&line, 40);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.trim_start().replace(' ', ""), "光の海を越えて");
        for (reading, kanji) in [("ひかり", "光"), ("うみ", "海"), ("こ", "越")] {
            let (r, k) = centres(&rows[0], reading, kanji);
            assert!(r.abs_diff(k) <= 1, "{reading} over {kanji}: {rows:?}");
        }
        // Readings never touch.
        assert!(!rows[0].0.contains("りう") && !rows[0].0.contains("みこ"));
        // Nothing to make room for: no spaces added.
        let rows = ruby_rows(&[ruby("越", Some("こ")), ruby("えて", None)], 40);
        assert_eq!(rows, vec![("こ".to_string(), "越えて".to_string())]);
        // Narrow: wraps between pieces, readings go with their kanji.
        let pieces = vec![ruby("夜空", Some("よぞら")), ruby("に", None), ruby("星", Some("ほし"))];
        let rows = ruby_rows(&pieces, 6);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].1.contains("夜空") && rows[1].1.contains('星') && rows[1].0.contains("ほし"));
    }

    #[test]
    fn scrolling_keeps_the_cursor_visible() {
        assert_eq!(scroll(0, 0, 10, 100), 0);
        assert_eq!(scroll(0, 15, 10, 100), 6);
        assert_eq!(scroll(20, 5, 10, 100), 5);
        assert_eq!(scroll(95, 99, 10, 100), 90);
        assert_eq!(scroll(0, 3, 10, 5), 0);
    }
}
