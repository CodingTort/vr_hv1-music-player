use iced::widget::{button, column, container, image, row, text, Space};
// Track tags can be in any script; Basic shaping has no font fallback.
use iced::widget::text::Shaping;
use iced::{Alignment, Element, Length};

use crate::app::{AppState, Message};
use crate::ui::components::{controls, progress, spectrum};
use crate::ui::{icons, theme};

pub fn view(state: &AppState) -> Element<'_, Message> {
    let track_info: Element<Message> = if let Some(track) = &state.current_track {
        let like_btn = button(
            text(icons::ICON_HEART)
                .font(icons::NERD_FONT_MONO)
                .size(30)
                .color(if track.liked { theme::red() } else { theme::overlay0() }),
        )
        .on_press(Message::KeyboardLike)
        .style(iced::widget::button::text)
        .padding([0, 4]);

        let artist_btn = button(
            text(&track.artist)
                .color(theme::subtext())
                .size(13)
                .shaping(Shaping::Advanced),
        )
        .on_press(Message::FocusArtistName)
        .style(iced::widget::button::text)
        .padding(0);

        let title_btn = button(
            text(&track.title)
                .color(theme::text())
                .size(20)
                .font(icons::UI_FONT_BOLD)
                .shaping(Shaping::Advanced),
        )
        .on_press(Message::FocusSongName)
        .style(iced::widget::button::text)
        .padding(0);

        // "Album (3)", or just "Album" when there is no track number.
        let album_label = match track.track_number {
            Some(n) => format!("{} ({n})", track.album),
            None => track.album.clone(),
        };
        let album_btn = button(
            text(album_label).color(theme::subtext()).size(13).shaping(Shaping::Advanced),
        )
        .on_press(Message::FocusAlbumName)
        .style(iced::widget::button::text)
        .padding(0);

        row![
            column![artist_btn, title_btn, album_btn,]
                .spacing(4)
                .width(Length::Fill),
            like_btn,
        ]
        .spacing(8)
        .align_y(Alignment::Start)
        .into()
    } else {
        column![text(state.strings.no_track)
            .color(theme::overlay0())
            .size(16)]
        .into()
    };

    // Capa do álbum
    let size = state.cover_size;
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

    let info_col = column![
        track_info,
        Space::with_height(12),
        progress::progress_bar(state.position, state.duration),
        Space::with_height(8),
        controls::playback_controls(
            &state.playback_state,
            state.volume,
            state.shuffle,
            state.repeat,
        ),
    ]
    .spacing(0);

    let mut player_row = row![cover, Space::with_width(16)];

    if state.show_spectrum {
        player_row = player_row
            .push(info_col.width(Length::FillPortion(5)))
            .push(Space::with_width(16))
            .push(spectrum::spectrum_view(
                &state.spectrum,
                state.volume,
                state.spectrum_style == "mirrored",
                Length::FillPortion(3),
                Length::Fixed(size),
            ));
    } else {
        player_row = player_row.push(info_col.width(Length::Fill));
    }

    let player_row = player_row
        .spacing(0)
        .align_y(Alignment::Center)
        .padding(16);

    let panel_style = if state.glass_surfaces() { theme::player_panel_glass } else { theme::player_panel };
    container(player_row)
        .style(panel_style)
        .width(Length::Fill)
        .into()
}
