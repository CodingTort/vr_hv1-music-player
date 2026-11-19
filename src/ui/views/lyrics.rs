use iced::widget::text::Shaping;
use iced::widget::{button, column, container, mouse_area, row, text, toggler, Space};
use iced::{Alignment, Element, Length};

use crate::app::{AppState, Message};
use crate::lyrics::Lyrics;
use crate::ui::components::lyrics::LyricsView;
use crate::ui::theme;

/// Lyrics for the playing track.
pub fn view(state: &AppState, text_size: f32) -> Element<'_, Message> {
    let Some(track) = &state.current_track else {
        return note("Nothing playing", None);
    };
    let (lyrics, alt_note) = state.shown_lyrics(track);
    let offset = state.lyrics_offset;
    let (body, synced): (Element<'_, Message>, bool) = match lyrics {
        None => return note("Looking for lyrics…", None),
        // Japanese lyrics get furigana over their kanji (not in romaji mode,
        // where the lyrics shown have no kanji).
        Some(Lyrics::Synced(lines)) => (
            LyricsView::new(lines, true, state.position, offset, text_size)
                .furigana(state.furigana_for(lines).map(Vec::as_slice))
                .color(state.lyrics_color())
                .into(),
            true,
        ),
        Some(Lyrics::Plain(lines)) => (
            LyricsView::new(lines, false, state.position, 0.0, text_size)
                .furigana(state.furigana_for(lines).map(Vec::as_slice))
                .color(state.lyrics_color())
                .into(),
            false,
        ),
        Some(Lyrics::Instrumental) => return note("♪  Instrumental", None),
        Some(Lyrics::NotFound) => {
            return note(
                "No lyrics found",
                Some("To add your own, save a .lrc file next to the song, with the same name.".into()),
            )
        }
        Some(Lyrics::Error(e)) => return note("Couldn't fetch lyrics", Some(e.clone())),
    };

    // A bar above the lyrics, only when there is something to say or switch.
    let shifted = synced && offset != 0.0;
    let notes: Vec<String> = [
        alt_note,
        (!synced).then(|| "Not synced to the song".to_string()),
        // Timing adjusted for this song (right-click a line, or [ / ]).
        shifted.then(|| format!("Lyrics shifted {offset:+.1} s")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let japanese = state.is_japanese_song(track);
    if notes.is_empty() && !japanese {
        return body;
    }

    let notes = column(notes.into_iter().map(|n| {
        text(n).size(12).color(theme::overlay0()).shaping(Shaping::Advanced).into()
    }))
    .spacing(2)
    .width(Length::Fill);
    let mut bar = row![notes].spacing(10).align_y(Alignment::Center).padding([8, 14]);
    if shifted {
        bar = bar.push(
            button(text("Reset").size(12))
                .on_press(Message::LyricsOffsetReset)
                .style(iced::widget::button::text)
                .padding([2, 6]),
        );
    }
    if japanese {
        bar = bar.push(
            toggler(state.lyrics_romaji)
                .label("Romaji")
                .text_size(12)
                .size(16)
                .on_toggle(Message::ToggleLyricsRomaji),
        );
    }
    column![bar, body].into()
}

/// The lyrics column on the right of the library, resizable by dragging its
/// left edge like the sidebar.
pub fn side_panel(state: &AppState) -> Element<'_, Message> {
    // The gap between the cards is the (invisible) handle.
    let handle = mouse_area(Space::new(Length::Fixed(8.0), Length::Fill))
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .on_press(Message::LyricsDragStart);

    let style = if state.glass_surfaces() { theme::sidebar_glass } else { theme::sidebar };
    row![
        handle,
        container(view(state, 20.0))
            .style(style)
            .width(Length::Fixed(state.lyrics_width))
            .height(Length::Fill),
    ]
    .into()
}

/// A status message. Each piece is its own widget because iced centres a
/// multi-line text as a block, not line by line.
fn note<'a>(title: &'a str, detail: Option<String>) -> Element<'a, Message> {
    let mut content = column![text(title).size(15).color(theme::subtext()).shaping(Shaping::Advanced)]
        .spacing(10)
        .align_x(Alignment::Center);
    if let Some(detail) = detail {
        content = content.push(text(detail).size(13).color(theme::overlay0()).shaping(Shaping::Advanced));
    }
    container(content).padding(24).center(Length::Fill).into()
}
