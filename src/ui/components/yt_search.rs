use iced::widget::{button, column, container, image, row, text, text_input, scrollable, Space};
use iced::{Alignment, Element, Length};

use crate::app::{Message, YtSearchState};
use crate::ui::theme;

/// The search box, focused when the dialog opens.
pub const QUERY_ID: &str = "yt_query";

/// Rounding of the dialog and of everything in it, whatever the app-wide
/// corner radius is set to.
fn radius() -> f32 {
    crate::ui::theme::corner_radius().max(18.0)
}

/// A button with the dialog's rounding.
fn rounded(style: fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style) -> impl Fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |t, st| {
        let mut s = style(t, st);
        s.border.radius = (radius() * 0.6).into();
        s
    }
}

/// YouTube search & download dialog.
///
/// Opened with `Y`: a card centred over a dimmed backdrop, with the results
/// on the left and a large preview of the selected one on the right. A link
/// (a song, a playlist or an album) can be pasted instead of a search.
pub fn view(state: &YtSearchState) -> Element<'_, Message> {
    let busy = state.searching || state.downloading.is_some();

    let input = text_input("Search for a song, or paste a link (song, playlist, album)…", &state.query)
        .id(text_input::Id::new(QUERY_ID))
        .on_input(Message::YtQueryChanged)
        .on_submit(Message::YtSubmit)
        .style(|t, st| {
            let mut s = theme::dialog_input(t, st);
            s.border.radius = (radius() * 0.6).into();
            s
        })
        .size(14)
        .padding([10, 14])
        .width(Length::Fill);

    let is_link = crate::youtube::looks_like_url(&state.query);
    let search_button = if state.searching {
        button(text(if is_link { "Opening…" } else { "Searching…" }).size(14))
            .style(rounded(theme::secondary_button))
            .padding([10, 18])
    } else {
        button(text(if is_link { "Open link" } else { "Search" }).size(14))
            .on_press(Message::YtSubmit)
            .style(rounded(theme::primary_button))
            .padding([10, 18])
    };

    let header = row![
        text(crate::ui::icons::ICON_YOUTUBE)
            .font(crate::ui::icons::NERD_FONT_MONO)
            .size(24)
            .color(iced::Color::from_rgb8(0xff, 0x33, 0x33)),
        Space::with_width(8),
        text("Search YouTube")
            .size(17)
            .font(crate::ui::icons::UI_FONT_BOLD)
            .color(theme::accent()),
        Space::with_width(Length::Fill),
        button(
            text(crate::ui::icons::ICON_CLOSE)
                .font(crate::ui::icons::NERD_FONT_MONO)
                .color(theme::red())
                .size(26)
        )
        .on_press(Message::CloseYtSearch)
        .style(iced::widget::button::text),
    ]
    .align_y(Alignment::Center);

    // yt-dlp missing: show the install hint and nothing else.
    if let Some(err) = state.preflight_error.as_ref().filter(|_| !state.checking) {
        return shell(
            column![
                header,
                Space::with_height(16),
                text("yt-dlp is not available").size(13).color(theme::text()),
                Space::with_height(8),
                text(err.clone()).size(12).color(theme::red()),
                Space::with_height(16),
                row![
                    Space::with_width(Length::Fill),
                    button(text("Close").size(13))
                        .on_press(Message::CloseYtSearch)
                        .style(theme::secondary_button)
                        .padding([6, 14]),
                ],
            ]
            .spacing(4),
        );
    }

    let results_view: Element<Message> = if state.results.is_empty() {
        let hint = if state.searching {
            if is_link { "Opening the link…" } else { "Searching YouTube…" }
        } else {
            "Type a song or an artist and press Enter, or paste a YouTube link to a song, a playlist or an album."
        };
        container(text(hint).size(13).color(theme::overlay0()).center())
            .center_x(Length::Fill)
            .center_y(Length::Fixed(440.0))
            .into()
    } else {
        let mut items: Vec<Element<Message>> = Vec::with_capacity(state.results.len());

        for (i, r) in state.results.iter().enumerate() {
            let is_sel = state.selected == Some(i);
            let is_busy = state.downloading == Some(i);

            // Numbered prefix so the ↑/↓ position is obvious.
            let index_label = text(format!("{:>2}", i + 1))
                .size(12)
                .font(crate::ui::icons::UI_FONT_BOLD)
                .color(if is_sel { theme::accent() } else { theme::overlay0() })
                .width(Length::Fixed(20.0));

            let title_color = if is_sel { theme::accent() } else { theme::text() };
            let mut title_text = r.title.clone();
            // Keep long video titles from blowing out the row width.
            if title_text.chars().count() > 60 {
                title_text = title_text.chars().take(57).collect::<String>() + "…";
            }

            let duration_text = if r.duration.is_empty() {
                String::new()
            } else {
                r.duration.clone()
            };

            let mut meta: Vec<String> = Vec::new();
            if !r.uploader.is_empty() {
                meta.push(r.uploader.clone());
            }
            if !duration_text.is_empty() {
                meta.push(duration_text);
            }
            let meta_text = meta.join("  ·  ");

            let saved = state.saved.contains(&i);
            let action = if saved {
                button(text("✓ Saved").size(11).color(theme::green()))
                    .style(iced::widget::button::text)
                    .padding([4, 10])
            } else if is_busy {
                button(text("Downloading…").size(11))
                    .style(rounded(theme::secondary_button))
                    .padding([4, 10])
            } else {
                button(text("Download").size(11))
                    .on_press(Message::YtDownload(i))
                    .style(rounded(if is_sel { theme::primary_button } else { theme::secondary_button }))
                    .padding([4, 10])
            };

            // Thumbnail (or a placeholder while it loads), 16:9.
            let thumb: Element<Message> = match state.thumbs.get(&r.id) {
                Some(handle) => image(handle.clone()).width(96).height(54).into(),
                None => container(Space::new(96, 54)).style(theme::card).into(),
            };

            items.push(
                button(
                    row![
                        index_label,
                        thumb,
                        column![
                            text(title_text).size(13).color(title_color).shaping(iced::widget::text::Shaping::Advanced),
                            text(meta_text).size(11).color(theme::subtext()).shaping(iced::widget::text::Shaping::Advanced),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        action,
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .padding([6, 8]),
                )
                .on_press(Message::YtSelect(i))
                // The selected result is a lit card; the rest highlight on hover.
                .style(move |_, status| {
                    let hovered = matches!(status, iced::widget::button::Status::Hovered);
                    let tint = if is_sel { 0.14 } else if hovered { 0.06 } else { 0.0 };
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(theme::with_alpha(theme::accent(), tint))),
                        border: iced::Border {
                            color: if is_sel { theme::with_alpha(theme::accent(), 0.6) } else { iced::Color::TRANSPARENT },
                            width: 1.0,
                            radius: (radius() * 0.6).into(),
                        },
                        text_color: theme::text(),
                        ..Default::default()
                    }
                })
                .width(Length::Fill)
                .into(),
            );
        }

        scrollable(column(items).spacing(4))
            .height(Length::Fixed(440.0))
            .width(Length::Fill)
            .into()
    };

    let body_row: Element<Message> = if state.results.is_empty() {
        results_view
    } else {
        row![results_view, preview(state)].spacing(16).into()
    };

    // Progress / success line.
    let mut footer = column![].spacing(4);
    if let Some(err) = &state.error {
        footer = footer.push(
            text(err.clone())
                .size(11)
                .color(theme::red())
                .width(Length::Fill),
        );
    } else if let Some(st) = &state.status {
        let color = if st.starts_with("Saved") || st.starts_with("Download complete") {
            theme::green()
        } else {
            theme::subtext()
        };
        footer = footer.push(text(st.clone()).size(11).color(color).width(Length::Fill).shaping(iced::widget::text::Shaping::Advanced));
    }

    let body = column![
        header,
        Space::with_height(12),
        row![input, Space::with_width(8), search_button]
            .align_y(Alignment::Center),
        Space::with_height(12),
        body_row,
        Space::with_height(10),
        footer,
        Space::with_height(12),
        row![
            text(if busy {
                "Esc: close"
            } else if state.results.is_empty() {
                "Enter: search or open the link · Esc: close"
            } else {
                "↑/↓: pick · Enter: download it (or search, after changing the text) · Esc: close"
            })
            .size(11)
            .color(theme::overlay0()),
            Space::with_width(Length::Fill),
            button(text("Close").size(13))
                .on_press(Message::CloseYtSearch)
                .style(theme::secondary_button)
                .padding([6, 14]),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(4);

    shell(body)
}

/// Wraps dialog content in the centred card + dimmed backdrop.
fn shell<'a>(body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let body = body.into();

    let card = container(body)
        .width(Length::Fill)
        .max_width(920)
        .padding(24)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(theme::mantle())),
            border: iced::Border {
                color: theme::with_alpha(theme::accent(), 0.7),
                width: 1.0,
                radius: radius().into(),
            },
            shadow: iced::Shadow {
                color: theme::base(),
                offset: iced::Vector { x: 0.0, y: 4.0 },
                blur_radius: 16.0,
            },
            ..Default::default()
        });

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(20)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgba(
                0.0, 0.0, 0.0, 0.55,
            ))),
            ..Default::default()
        })
        .into()
}

/// The selected result, large: its cover, title, channel and length, and
/// what can be done with it. For a pasted playlist or album, also a button
/// to download all of it into a playlist.
fn preview(state: &YtSearchState) -> Element<'static, Message> {
    const W: f32 = 320.0;
    let Some((i, r)) = state.selected.and_then(|i| state.results.get(i).map(|r| (i, r))) else {
        return Space::with_width(W).into();
    };
    let cover: Element<Message> = match &state.preview {
        Some((id, handle)) if *id == r.id => image(handle.clone()).width(W).height(W * 9.0 / 16.0).into(),
        _ => match state.thumbs.get(&r.id) {
            // The small thumbnail, scaled up, until the large one arrives.
            Some(handle) => image(handle.clone()).width(W).height(W * 9.0 / 16.0).into(),
            None => container(Space::new(W, W * 9.0 / 16.0))
                .style(|_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(theme::surface0())),
                    border: iced::Border { radius: (radius() * 0.6).into(), ..Default::default() },
                    ..Default::default()
                })
                .into(),
        },
    };
    let meta = [r.uploader.as_str(), r.duration.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("  ·  ");
    let action = if state.saved.contains(&i) {
        button(text("✓ Saved to your library").size(13).color(theme::green())).style(iced::widget::button::text).padding([8, 0])
    } else if state.downloading == Some(i) {
        button(text("Downloading…").size(13)).style(rounded(theme::secondary_button)).padding([8, 16])
    } else {
        button(text("Download").size(13))
            .on_press_maybe(state.downloading.is_none().then_some(Message::YtDownload(i)))
            .style(rounded(theme::primary_button))
            .padding([8, 16])
    };
    let mut col = column![
        cover,
        Space::with_height(10),
        text(r.title.clone()).size(15).font(crate::ui::icons::UI_FONT_BOLD).color(theme::text()).shaping(iced::widget::text::Shaping::Advanced),
        text(meta).size(12).color(theme::subtext()).shaping(iced::widget::text::Shaping::Advanced),
        Space::with_height(10),
        action,
    ]
    .spacing(2)
    .width(W);
    if let Some(list) = &state.list_title {
        col = col.push(Space::with_height(14)).push(
            text(format!("From “{list}”: {} songs", state.results.len()))
                .size(12)
                .color(theme::subtext())
                .shaping(iced::widget::text::Shaping::Advanced),
        );
        col = col.push(
            button(text("Download all into a playlist").size(13))
                .on_press_maybe(state.downloading.is_none().then_some(Message::YtDownloadAll))
                .style(rounded(theme::primary_button))
                .padding([8, 16]),
        );
    }
    col.into()
}
