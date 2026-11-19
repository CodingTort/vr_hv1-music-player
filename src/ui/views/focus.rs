use iced::widget::{button, column, container, image, row, text, Space};
// Track tags can be in any script; Basic shaping has no font fallback.
use iced::widget::text::Shaping;
use iced::{Alignment, Element, Length};

use crate::app::{AppState, Message};
use crate::ui::components::{controls, progress, spectrum};
use crate::ui::{icons, theme};

pub fn view(state: &AppState) -> Element<'_, Message> {
    // The focus layout is about the cover, so it gets more room — but no more
    // than the window has left after the title, controls and spectrum (~360 px).
    let size = (state.cover_size * 1.4).min(state.window_height - 360.0).max(120.0);
    let cover: Element<Message> = if let Some(handle) = state.cover_rounded.clone().or_else(|| state.cover.clone()) {
        // The rounded copy arrives a moment after the track starts; the
        // frame's corners match it (the radius is scaled with the cover).
        let radius = theme::corner_radius() * size / state.cover_size;
        container(image(handle).width(size).height(size).content_fit(iced::ContentFit::Cover))
            .padding(2)
            .style(theme::cover_frame(radius + 2.0))
            .into()
    } else {
        container(
            text(icons::ICON_MUSIC)
                .font(icons::NERD_FONT_MONO)
                .color(theme::overlay0())
                .size(96),
        )
        .width(size)
        .height(size)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(theme::card)
        .into()
    };

    let track_info: Element<Message> = if let Some(track) = &state.current_track {
        let title_row = row![
            text(&track.title)
                .shaping(Shaping::Advanced)
                .color(theme::text())
                .size(22)
                .font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..icons::UI_FONT
                }),
            button(
                text(icons::ICON_HEART)
                    .font(icons::NERD_FONT_MONO)
                    .size(30)
                    .color(if track.liked { theme::red() } else { theme::overlay0() }),
            )
            .on_press(Message::KeyboardLike)
            .style(iced::widget::button::text)
            .padding([0, 6]),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        column![
            text(&track.artist).color(theme::subtext()).size(13).shaping(Shaping::Advanced),
            title_row,
            // "Album · 3", or just "Album" when there is no track number.
            text(match track.track_number {
                Some(n) => format!("{} · {n}", track.album),
                None => track.album.clone(),
            })
            .color(theme::subtext())
            .size(13)
            .shaping(Shaping::Advanced),
        ]
        .spacing(4)
        .align_x(Alignment::Center)
        .into()
    } else {
        text(state.strings.no_track)
            .color(theme::overlay0())
            .size(16)
            .into()
    };

    let mut layout = column![
        Space::with_height(Length::Fill),
        container(cover).center_x(Length::Fill),
        Space::with_height(24),
        container(track_info).center_x(Length::Fill),
        Space::with_height(20),
        row![
            Space::with_width(Length::FillPortion(1)),
            container(progress::progress_bar(state.position, state.duration))
                .width(Length::FillPortion(1)),
            Space::with_width(Length::FillPortion(1)),
        ]
        .width(Length::Fill),
        Space::with_height(12),
        container(controls::playback_controls(
            &state.playback_state,
            state.volume,
            state.shuffle,
            state.repeat,
        ))
        .center_x(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    if state.show_spectrum {
        layout = layout
            .push(Space::with_height(16))
            .push(
                container(spectrum::spectrum_view(
                    &state.spectrum,
                    state.volume,
                    state.spectrum_style == "mirrored",
                    Length::Fixed(320.0),
                    Length::Fixed(80.0),
                ))
                .center_x(Length::Fill),
            );
    }

    layout = layout.push(Space::with_height(Length::Fill));

    // Lyrics beside the cover, as in Apple Music's full-screen player.
    let content: Element<Message> = if state.show_lyrics {
        row![
            container(layout).width(Length::FillPortion(3)),
            container(super::lyrics::view(state, 24.0))
                .width(Length::FillPortion(2))
                .height(Length::Fill),
        ]
        .into()
    } else {
        layout.into()
    };

    let bg_style = if state.glass_surfaces() {
        theme::base_style_glass
    } else {
        |_: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(theme::base())),
            ..Default::default()
        }
    };
    container(content)
        .style(bg_style)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
