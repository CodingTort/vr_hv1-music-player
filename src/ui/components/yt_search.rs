use iced::widget::{button, column, container, image, row, text, text_input, scrollable, Space};
use iced::{Alignment, Element, Length};

use crate::app::{Message, YtSearchState};
use crate::ui::theme;

/// YouTube search & download dialog.
///
/// Opened with `Y`. Mirrors the layout of the playlist dialog: a card centred
/// over a dimmed backdrop.
pub fn view(state: &YtSearchState) -> Element<'_, Message> {
    let busy = state.searching || state.downloading.is_some();

    let input = text_input("Artist — song title…", &state.query)
        .on_input(Message::YtQueryChanged)
        .on_submit(Message::YtSubmit)
        .style(theme::dialog_input)
        .size(13)
        .padding([6, 8])
        .width(Length::Fill);

    let search_button = if state.searching {
        button(text("Searching…").size(13))
            .style(theme::secondary_button)
            .padding([6, 14])
    } else {
        button(text("Search").size(13))
            .on_press(Message::YtSubmit)
            .style(theme::primary_button)
            .padding([6, 14])
    };

    let header = row![
        text(crate::ui::icons::ICON_YOUTUBE)
            .font(crate::ui::icons::NERD_FONT_MONO)
            .size(24)
            .color(iced::Color::from_rgb8(0xff, 0x33, 0x33)),
        Space::with_width(8),
        text("YouTube")
            .size(16)
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
    if let Some(err) = &state.preflight_error {
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
            "Searching YouTube…".to_string()
        } else {
            "Type a song and press Enter to search.".to_string()
        };
        text(hint)
            .size(12)
            .color(theme::overlay0())
            .width(Length::Fill)
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
                    .style(theme::secondary_button)
                    .padding([4, 10])
            } else {
                button(text("Download").size(11))
                    .on_press(Message::YtDownload(i))
                    .style(if is_sel { theme::primary_button } else { theme::secondary_button })
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
                            radius: theme::corner_radius().into(),
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
            .height(Length::Fixed(380.0))
            .width(Length::Fill)
            .into()
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
        results_view,
        Space::with_height(10),
        footer,
        Space::with_height(12),
        row![
            text(if busy {
                "Enter: download selected · Esc: close"
            } else {
                "Enter: search · ↑/↓: select · Enter: download · Esc: close"
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
fn shell(body: impl Into<Element<'static, Message>>) -> Element<'static, Message> {
    let body = body.into();

    let card = container(body)
        .width(640)
        .padding(20)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(theme::mantle())),
            border: iced::Border {
                color: theme::accent(),
                width: 1.0,
                radius: crate::ui::theme::corner_radius().into(),
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
