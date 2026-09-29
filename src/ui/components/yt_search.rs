use iced::widget::{button, column, container, row, text, text_input, scrollable, Space};
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

            let action = if is_busy {
                button(text("Downloading…").size(11))
                    .style(theme::secondary_button)
                    .padding([4, 10])
            } else {
                button(text("Download").size(11))
                    .on_press(Message::YtDownload(i))
                    .style(if is_sel { theme::primary_button } else { theme::secondary_button })
                    .padding([4, 10])
            };

            items.push(
                button(
                    row![
                        index_label,
                        column![
                            text(title_text).size(13).color(title_color),
                            text(meta_text).size(11).color(theme::subtext()),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        action,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .padding([4, 6]),
                )
                .on_press(Message::YtSelect(i))
                .style(iced::widget::button::text)
                .width(Length::Fill)
                .into(),
            );
        }

        scrollable(column(items).spacing(2))
            .height(Length::Fixed(340.0))
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
        footer = footer.push(text(st.clone()).size(11).color(color).width(Length::Fill));
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
        .width(520)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(theme::mantle())),
            border: iced::Border {
                color: theme::accent(),
                width: 1.0,
                radius: 8.0.into(),
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
