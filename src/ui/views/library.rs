use iced::widget::{
    button, column, container, mouse_area, row, scrollable, text, text_input, Space,
};
// Names and titles can be in any script; Basic shaping has no font fallback.
use iced::widget::text::Shaping;
use iced::{Alignment, Element, Length};

use crate::app::{AppState, ContextMenuTarget, Message, PlaylistDialogMode, PlaylistTab, SortColumn, ViewMode};
use crate::library::models::Track;
use crate::persist::TableColumn;
use crate::ui::{icons, theme};

/// Heights in the track list. Every item has a fixed height, so only the
/// rows on screen need building (see `track_list_view`).
pub const ROW_H: f32 = 34.0;
/// One line of 13 px row text.
const CELL_LINE_H: f32 = 17.0;
pub const ALBUM_HEADER_H: f32 = 34.0;
pub const ALBUM_GAP_H: f32 = 6.0;
/// Rows built above and below the visible part, for smooth scrolling.
const OVERSCAN_PX: f32 = 600.0;
/// Largura da coluna de favorito (coração), compartilhada entre header e linhas.
const LIKE_COL_W: f32 = 60.0;

pub fn view(state: &AppState) -> Element<'_, Message> {
    let sidebar = sidebar_view(state);

    // The gap between the cards is the handle: invisible, but it shows the
    // resize cursor and can be dragged.
    let drag_handle = mouse_area(Space::new(Length::Fixed(theme::CARD_GAP), Length::Fill))
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::SidebarDragStart);

    // The track list is a card like the sidebar and the lyrics panel.
    let card = if state.glass_surfaces() { theme::sidebar_glass } else { theme::sidebar };
    let track_list = container(track_list_view(state))
        .padding(theme::card_inset())
        .style(card)
        .width(Length::Fill)
        .height(Length::Fill);

    row![sidebar, drag_handle, track_list]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

fn sidebar_view(state: &AppState) -> Element<'_, Message> {
    let tab_btn = |label: &'static str, mode: ViewMode, icon: &'static str| -> Element<'_, Message> {
        let active = state.view_mode == mode && state.selected_playlist.is_none();
        let color = if active { theme::accent() } else { theme::subtext() };
        button(
            row![
                text(icon).font(icons::NERD_FONT_MONO).size(14).color(color),
                text(label).size(11).color(color),
            ]
            .spacing(2)
            .align_y(Alignment::Center),
        )
        .on_press(Message::SelectViewMode(mode))
        .style(iced::widget::button::text)
        // Compact enough that all three tabs fit the default 200 px sidebar
        // (they used to wrap "Genres" letter by letter).
        .padding([4, 3])
        .into()
    };

    let tabs = row![
        tab_btn("Artists", ViewMode::Artists, icons::ICON_MUSIC),
        tab_btn("Albums", ViewMode::Albums, icons::ICON_LIST),
        tab_btn("Genres", ViewMode::Genres, icons::ICON_PODIUM),
    ]
    .spacing(0);

    let sidebar_search = row![
        text(icons::ICON_SEARCH)
            .font(icons::NERD_FONT_MONO)
            .size(22)
            .color(theme::overlay0()),
        text_input("Filter…", &state.sidebar_search)
            .on_input(Message::SidebarSearchChanged)
            .style(theme::dialog_input)
            .size(12)
            .padding([3, 6])
            .width(Length::Fill),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .padding([2, 4]);

    // Build sidebar item list based on ViewMode
    let list_items: Element<Message> = match state.view_mode {
        ViewMode::Artists => {
            let items = state.artists();
            let selected = state.selected_artist.clone();
            let active = state.current_track.as_ref().map(|t| t.artist.clone());
            build_sidebar_list(items, selected, active, Message::SelectArtist, Some(ContextMenuTarget::Artist))
        }
        ViewMode::Albums => {
            let items = state.albums();
            let selected = state.selected_album.clone();
            let active = state.current_track.as_ref().map(|t| t.album.clone());
            build_sidebar_list(items, selected, active, Message::SelectAlbum, Some(ContextMenuTarget::Album))
        }
        ViewMode::Genres => {
            let items = state.genres();
            let selected = state.selected_genre.clone();
            build_sidebar_list(items, selected, None, Message::SelectGenre, None)
        }
    };

    let list_with_context: Element<Message> = mouse_area(
        scrollable(list_items)
            .id(scrollable::Id::new("sidebar_scroll"))
            .height(Length::Fill),
    )
    .on_enter(Message::HoverSidebarList(true))
    .on_exit(Message::HoverSidebarList(false))
    .into();

    let playlist_panel = playlist_panel_view(state);

    // A hairline divider with a taller invisible grab area around it.
    let pl_resize = mouse_area(
        container(
            container(Space::new(Length::Fill, Length::Fixed(1.0))).style(move |_| {
                iced::widget::container::Style {
                    background: Some(iced::Background::Color(theme::surface0())),
                    ..Default::default()
                }
            }),
        )
        .padding([5, 10]),
    )
    .interaction(iced::mouse::Interaction::ResizingVertically)
    .on_press(Message::PlaylistDragStart);

    let sidebar_body = column![
        container(tabs).padding([4, 4]),
        sidebar_search,
        list_with_context,
        pl_resize,
        playlist_panel,
    ]
    .spacing(0);

    let sidebar_style = if state.glass_surfaces() { theme::sidebar_glass } else { theme::sidebar };
    container(sidebar_body)
        .padding(theme::card_inset())
        .style(sidebar_style)
        .width(Length::Fixed(state.sidebar_width))
        .height(Length::Fill)
        .into()
}

fn build_sidebar_list(
    items: Vec<String>,
    selected: Option<String>,
    active: Option<String>,
    make_msg: fn(String) -> Message,
    make_ctx: Option<fn(String) -> ContextMenuTarget>,
) -> Element<'static, Message> {
    if items.is_empty() {
        return container(text("Nothing here").color(theme::overlay0()).size(13))
            .center_x(Length::Fill)
            .padding([12, 0])
            .width(Length::Fill)
            .into();
    }

    let rows: Vec<Element<'static, Message>> = items
        .into_iter()
        .map(|name| {
            let is_selected = selected.as_deref() == Some(name.as_str());
            let is_active = active.as_deref() == Some(name.as_str());
            let color = if is_selected {
                theme::accent()
            } else if is_active {
                theme::green()
            } else {
                theme::text()
            };

            let ctx = make_ctx.map(|f| f(name.clone()));
            let msg = make_msg(name.clone());
            let btn = button(text(name).color(color).size(13).width(Length::Fill).shaping(Shaping::Advanced))
                .on_press(msg)
                .style(iced::widget::button::text)
                .width(Length::Fill)
                .padding([5, 10]);

            let styled: Element<'static, Message> = if is_selected {
                container(btn).style(theme::selected_row).width(Length::Fill).into()
            } else {
                container(btn).width(Length::Fill).into()
            };

            if let Some(target) = ctx {
                mouse_area(styled)
                    .on_right_press(Message::ToggleContextMenu(Some(target)))
                    .into()
            } else {
                styled
            }
        })
        .collect();

    column(rows).spacing(1).into()
}

// ── Playlist panel ────────────────────────────────────────────────────────────

fn playlist_panel_view(state: &AppState) -> Element<'_, Message> {
    let is_playlists = state.playlist_tab == PlaylistTab::Playlists;

    let tab_row: Element<Message> = row![
        button(
            text("Playlists").size(12).color(
                if is_playlists { theme::accent() } else { theme::subtext() },
            ),
        )
        .on_press(Message::SelectPlaylistTab(PlaylistTab::Playlists))
        .style(iced::widget::button::text)
        .padding([3, 6]),
        button(
            text("Auto").size(12).color(
                if !is_playlists { theme::accent() } else { theme::subtext() },
            ),
        )
        .on_press(Message::SelectPlaylistTab(PlaylistTab::Autoplaylists))
        .style(iced::widget::button::text)
        .padding([3, 6]),
        Space::with_width(Length::Fill),
        // Import a playlist file (CSV of YouTube ids).
        button(
            text(icons::ICON_IMPORT)
                .font(icons::NERD_FONT_MONO)
                .size(20)
                .color(theme::subtext()),
        )
        .on_press(Message::ImportPlaylist)
        .style(iced::widget::button::text)
        .padding([3, 6]),
        button(
            text(icons::ICON_PLUS)
                .font(icons::NERD_FONT_MONO)
                .size(26)
                .color(theme::accent()),
        )
        .on_press(Message::OpenPlaylistDialog(PlaylistDialogMode::Create))
        .style(iced::widget::button::text)
        .padding([3, 6]),
    ]
    .spacing(0)
    .align_y(Alignment::Center)
    .padding([0, 4])
    .into();

    let list: Element<Message> = if is_playlists {
        let playlists = crate::persist::playlist_names();

        if playlists.is_empty() {
            container(
                text("No playlists. Press C or + to create.")
                    .size(12)
                    .color(theme::overlay0()),
            )
            .center_x(Length::Fill)
            .padding([8, 8])
            .width(Length::Fill)
            .into()
        } else {
            let selected_pl = state.selected_playlist.clone();

            let rows: Vec<Element<Message>> = playlists
                .into_iter()
                .map(|pl| {
                    let is_sel = selected_pl.as_deref() == Some(pl.as_str());

                    let delete_btn: Element<Message> = if is_sel {
                        row![
                            // Export as a CSV of YouTube ids.
                            button(
                                text(icons::ICON_EXPORT)
                                    .font(icons::NERD_FONT_MONO)
                                    .size(18)
                                    .color(theme::subtext()),
                            )
                            .on_press(Message::ExportPlaylist(pl.clone()))
                            .style(iced::widget::button::text)
                            .padding([2, 4]),
                            button(
                                text(icons::ICON_TRASH)
                                    .font(icons::NERD_FONT_MONO)
                                    .size(22)
                                    .color(theme::red()),
                            )
                            .on_press(Message::DeletePlaylist(pl.clone()))
                            .style(iced::widget::button::text)
                            .padding([2, 4]),
                        ]
                        .align_y(Alignment::Center)
                        .into()
                    } else {
                        Space::with_width(20).into()
                    };

                    let msg = Message::SelectPlaylist(pl.clone());
                    let pl_label = pl.clone();
                    let pl_btn = button(
                        row![
                            text(pl_label)
                                .size(12)
                                .color(if is_sel { theme::accent() } else { theme::text() })
                                .width(Length::Fill)
                                .shaping(Shaping::Advanced),
                            delete_btn,
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    )
                    .on_press(msg)
                    .style(iced::widget::button::text)
                    .width(Length::Fill)
                    .padding([4, 8]);

                    if is_sel {
                        container(pl_btn).style(theme::selected_row).width(Length::Fill).into()
                    } else {
                        container(pl_btn).width(Length::Fill).into()
                    }
                })
                .collect();

            column(rows).spacing(1).into()
        }
    } else {
        let selected_pl = state.selected_playlist.clone();
        let autoplaylists: Vec<(&'static str, &'static str, iced::Color)> = vec![
            ("Liked Songs", icons::ICON_HEART, theme::red()),
            ("Recently Played", icons::ICON_CLOCK, theme::accent()),
            ("Most Played", icons::ICON_PODIUM, theme::green()),
        ];
        let rows: Vec<Element<Message>> = autoplaylists
            .into_iter()
            .map(|(name, icon, color)| {
                let is_sel = selected_pl.as_deref() == Some(name);
                let pl_btn = button(
                    row![
                        text(icon).font(icons::NERD_FONT_MONO).size(22).color(color),
                        text(name)
                            .size(12)
                            .color(if is_sel { theme::accent() } else { theme::text() }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::SelectPlaylist(name.to_string()))
                .style(iced::widget::button::text)
                .width(Length::Fill)
                .padding([4, 8]);

                if is_sel {
                    container(pl_btn).style(theme::selected_row).width(Length::Fill).into()
                } else {
                    container(pl_btn).width(Length::Fill).into()
                }
            })
            .collect();
        column(rows).spacing(1).into()
    };

    let scroll_h = (state.playlist_height - 30.0).max(0.0);

    // No box of its own: the hairline above separates it, and a square
    // border here would cover the card's rounded corners.
    container(
        column![tab_row, scrollable(list).height(Length::Fixed(scroll_h))]
            .spacing(0),
    )
    .height(Length::Fixed(state.playlist_height))
    .width(Length::Fill)
    .into()
}

// ── Track list ────────────────────────────────────────────────────────────────

fn track_list_view(state: &AppState) -> Element<'_, Message> {
    if state.all_tracks.is_empty() {
        return container(text("Scanning library…").color(theme::overlay0()).size(15))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    if state.tracks.is_empty() {
        let hint = if state.selected_artist.is_none()
            && state.selected_album.is_none()
            && state.selected_genre.is_none()
            && state.selected_playlist.is_none()
            && state.search_query.is_empty()
        {
            "Select an artist, album or genre"
        } else if !state.search_query.is_empty() {
            "No tracks match your search"
        } else {
            "No tracks found"
        };
        return container(text(hint).color(theme::overlay0()).size(14))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    // Fetch column config once
    let visible_cols = crate::persist::get(|db| db.table_columns.clone());
    let col_widths = column_widths(&visible_cols);

    let sort_col = state.sort_column;
    let sort_asc = state.sort_ascending;
    let group = state.group_by_album;

    // Toolbar (busca) e cabeçalho de colunas ficam fixos no topo, fora do scroll.
    let toolbar = toolbar_view(state);
    let header = build_column_header(visible_cols.clone(), col_widths.clone(), sort_col, sort_asc);

    let current_id = state.current_track.as_ref().map(|t| t.id);
    let selected_ids: Vec<i64> = state.selected_tracks.iter().map(|t| t.id).collect();
    let multi_selected = state.selected_tracks.clone();

    // The list as items of known height. Only those near the visible part are
    // built (this view is rebuilt ~30 times a second while the visualiser
    // runs, and building every row of a large library made the app lag);
    // the rest is empty space of the same height.
    enum Item<'a> {
        Header(&'a str, usize),
        Track(&'a Track),
        Gap,
    }
    let mut items: Vec<Item> = Vec::with_capacity(state.tracks.len() + 8);
    if group {
        let mut i = 0;
        while i < state.tracks.len() {
            let album = state.tracks[i].album.as_str();
            let end = state.tracks[i..].iter().position(|t| t.album != album).map_or(state.tracks.len(), |n| i + n);
            items.push(Item::Header(album, end - i));
            items.extend(state.tracks[i..end].iter().map(Item::Track));
            items.push(Item::Gap);
            i = end;
        }
    } else {
        items.extend(state.tracks.iter().map(Item::Track));
    }
    let height = |item: &Item| match item {
        Item::Header(..) => ALBUM_HEADER_H,
        Item::Track(_) => ROW_H,
        Item::Gap => ALBUM_GAP_H,
    };
    let view_h = if state.tracklist_view_h > 0.0 { state.tracklist_view_h } else { state.window_height };
    let heights: Vec<f32> = items.iter().map(height).collect();
    let (shown, above, below) = visible_items(&heights, state.tracklist_offset, view_h);

    let mut rows: Vec<Element<Message>> = Vec::new();
    for item in &items[shown] {
        rows.push(match item {
            Item::Header(album, count) => container(
                row![
                    text(icons::ICON_LIST)
                        .font(icons::NERD_FONT_MONO)
                        .size(22)
                        .color(theme::accent()),
                    text(album.to_string())
                        .size(12)
                        .color(theme::accent())
                        .font(icons::UI_FONT_BOLD)
                        .width(Length::Fill)
                        .shaping(Shaping::Advanced),
                    text(format!("{count} tracks")).size(11).color(theme::overlay0()),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .padding([0, 12]),
            )
            .style(theme::album_header)
            .width(Length::Fill)
            .height(Length::Fixed(ALBUM_HEADER_H))
            .align_y(iced::alignment::Vertical::Center)
            .into(),
            Item::Track(track) => build_track_row(
                (*track).clone(),
                current_id,
                &selected_ids,
                &multi_selected,
                visible_cols.clone(),
                col_widths.clone(),
            ),
            Item::Gap => Space::with_height(ALBUM_GAP_H).into(),
        });
    }
    if above > 0.0 {
        rows.insert(0, Space::with_height(above).into());
    }
    if below > 0.0 {
        rows.push(Space::with_height(below).into());
    }

    let scroll = scrollable(column(rows).spacing(0))
        .id(scrollable::Id::new("tracklist_scroll"))
        .on_scroll(|v| Message::TracklistScrolled(v.absolute_offset().y, v.bounds().height))
        .height(Length::Fill);

    let content = column![toolbar, header, scroll]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);

    mouse_area(container(content).width(Length::Fill).height(Length::Fill))
        .on_enter(Message::HoverTracklist(true))
        .on_exit(Message::HoverTracklist(false))
        .into()
}

/// Which items of the list (by height) to build for a scroll position: the
/// visible ones plus `OVERSCAN_PX` either side, and the heights of the
/// unbuilt parts above and below. A position past the end (left from a longer
/// list) counts as the end, as the scrollable itself treats it.
fn visible_items(heights: &[f32], offset: f32, view_h: f32) -> (std::ops::Range<usize>, f32, f32) {
    let total: f32 = heights.iter().sum();
    let offset = offset.min((total - view_h).max(0.0)).max(0.0);
    let (from, to) = (offset - OVERSCAN_PX, offset + view_h + OVERSCAN_PX);
    let (mut y, mut first, mut last, mut above) = (0.0f32, heights.len(), 0, 0.0f32);
    for (i, &h) in heights.iter().enumerate() {
        if y + h < from {
            above += h;
        } else if y <= to {
            first = first.min(i);
            last = i + 1;
        }
        y += h;
    }
    if first > last {
        return (0..0, total, 0.0);
    }
    let shown: f32 = heights[first..last].iter().sum();
    (first..last, above, total - above - shown)
}

fn build_column_header(
    cols: Vec<TableColumn>,
    widths: Vec<Length>,
    sort_col: Option<SortColumn>,
    sort_asc: bool,
) -> Element<'static, Message> {
    let header_cells: Vec<Element<'static, Message>> = cols
        .into_iter()
        .zip(widths.into_iter())
        .map(|(col, w)| -> Element<'static, Message> {
            let sc = table_col_to_sort(col);
            let sort_arrow = if sort_col == Some(sc) {
                if sort_asc { " ↑" } else { " ↓" }
            } else {
                ""
            };
            let label = format!("{}{}", col.label(), sort_arrow);
            let color = if sort_col == Some(sc) { theme::accent() } else { theme::subtext() };

            let hdr_btn = button(text(label).size(11).color(color).font(icons::UI_FONT_BOLD))
                .on_press(Message::SortBy(sc))
                .style(iced::widget::button::text)
                .padding([4, 0])
                .width(w);

            mouse_area(hdr_btn)
                .on_right_press(Message::ToggleContextMenu(Some(
                    ContextMenuTarget::Header(col),
                )))
                .into()
        })
        .collect();

    // Célula vazia final equivalente à coluna de favorito nas linhas de faixa,
    // para que as colunas FillPortion resolvam à mesma largura nos dois lugares.
    let header_row = row(header_cells)
        .push(Space::with_width(Length::Fixed(LIKE_COL_W)))
        .spacing(0)
        .align_y(Alignment::Center);

    container(header_row)
        .padding([0, 12])
        .width(Length::Fill)
        .into()
}

fn toolbar_view(state: &AppState) -> Element<'_, Message> {
    use iced::widget::checkbox;

    let search_input: Element<Message> = row![
        text(icons::ICON_SEARCH)
            .font(icons::NERD_FONT_MONO)
            .size(24)
            .color(theme::overlay0()),
        text_input("Search…", &state.search_query)
            .on_input(Message::SearchChanged)
            .style(theme::dialog_input)
            .size(12)
            .padding([3, 6])
            .width(Length::Fixed(160.0)),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into();

    let group_color = if state.group_by_album { theme::accent() } else { theme::subtext() };
    let group_btn = button(
        row![
            text(icons::ICON_LIST).font(icons::NERD_FONT_MONO).size(22).color(group_color),
            text("Album").size(11).color(group_color),
        ]
        .spacing(3)
        .align_y(Alignment::Center),
    )
    .on_press(Message::ToggleGroupByAlbum)
    .style(iced::widget::button::text)
    .padding([3, 6]);

    let restore_btn: Element<Message> = if !state.hidden_artists_albums.is_empty() {
        button(
            text(format!("Restore {} hidden", state.hidden_artists_albums.len()))
                .size(11)
                .color(theme::red()),
        )
        .on_press(Message::RestoreHiddenItems)
        .style(iced::widget::button::text)
        .padding([3, 6])
        .into()
    } else {
        Space::with_width(0).into()
    };

    let bar = container(
        row![
            search_input,
            Space::with_width(8),
            checkbox("Title", state.filter_title).on_toggle(|_| Message::ToggleFilterTitle).size(12).text_size(11),
            checkbox("Artist", state.filter_artist).on_toggle(|_| Message::ToggleFilterArtist).size(12).text_size(11),
            checkbox("Album", state.filter_album).on_toggle(|_| Message::ToggleFilterAlbum).size(12).text_size(11),
            checkbox("Genre", state.filter_genre).on_toggle(|_| Message::ToggleFilterGenre).size(12).text_size(11),
            group_btn,
            restore_btn,
            button(text("?").size(12).color(theme::subtext()))
                .on_press(Message::OpenShortcuts)
                .style(iced::widget::button::text)
                .padding([3, 6]),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .padding([4, 12])
        // Flows onto a second line when the track list is narrow, instead of
        // squeezing the labels into vertical letters.
        .wrap(),
    )
    .width(Length::Fill);
    // A hairline underneath rather than a square bordered box, which would
    // cover the card's rounded corners.
    let hairline = container(Space::new(Length::Fill, Length::Fixed(1.0))).style(|_| {
        iced::widget::container::Style {
            background: Some(iced::Background::Color(theme::surface0())),
            ..Default::default()
        }
    });
    column![bar, container(hairline).padding([0, 8])].into()
}

fn build_track_row(
    track: Track,
    current_id: Option<i64>,
    selected_ids: &[i64],
    multi_selected: &[Track],
    cols: Vec<TableColumn>,
    widths: Vec<Length>,
) -> Element<'static, Message> {
    let is_current = current_id == Some(track.id);
    let is_selected = selected_ids.contains(&track.id);
    let liked = track.liked;
    let is_multi = selected_ids.len() > 1 && is_selected;

    let base_color = if is_current { theme::accent() } else { theme::text() };

    let cells: Vec<Element<'static, Message>> = cols
        .into_iter()
        .zip(widths.into_iter())
        .map(|(col, w)| -> Element<'static, Message> {
            let cell_text = match col {
                TableColumn::TrackNumber => track.track_number.map(|n| n.to_string()).unwrap_or_else(|| "·".to_string()),
                TableColumn::Title => {
                    if is_current { format!("▶ {}", track.title) } else { track.title.clone() }
                }
                TableColumn::Artist => track.artist.clone(),
                TableColumn::Album => track.album.clone(),
                TableColumn::Genre => track.genre.clone(),
                TableColumn::Year => track.year.map(|y| y.to_string()).unwrap_or_default(),
                TableColumn::DiscNumber => track.disc_number.map(|d| d.to_string()).unwrap_or_default(),
                TableColumn::Duration => track.duration_str(),
                TableColumn::Plays => track.play_count.to_string(),
                TableColumn::DatePlayed => track.date_played.clone().unwrap_or_default(),
            };
            let cell_color = match col {
                TableColumn::Title => base_color,
                TableColumn::Artist | TableColumn::Album | TableColumn::Genre if is_current => {
                    theme::with_alpha(theme::accent(), 0.8)
                }
                TableColumn::Artist | TableColumn::Album | TableColumn::Genre => theme::subtext(),
                _ => theme::subtext(),
            };
            // Rows have a fixed height, so a cell shows one line: long text
            // that wraps is cut after its first line instead of showing two
            // squeezed half-lines. (iced 0.13 ignores `Wrapping::None` for
            // plain text, so the clipping does it.)
            container(text(cell_text).size(13).color(cell_color).shaping(Shaping::Advanced))
                .width(w)
                .height(Length::Fixed(CELL_LINE_H))
                .padding(iced::Padding { right: 10.0, ..iced::Padding::ZERO })
                .clip(true)
                .into()
        })
        .collect();

    let like_icon: Element<'static, Message> = if liked || is_selected || is_current {
        let like_msg = Message::ToggleLikeTrack(Track { cover_data: None, ..track.clone() });
        button(
            text(icons::ICON_HEART)
                .font(icons::NERD_FONT_MONO)
                .size(22)
                .color(if liked { theme::red() } else { theme::with_alpha(theme::overlay0(), 0.5) }),
        )
        .on_press(like_msg)
        .style(iced::widget::button::text)
        .width(Length::Fixed(LIKE_COL_W))
        .padding([0, 4])
        .into()
    } else {
        Space::with_width(Length::Fixed(LIKE_COL_W)).into()
    };

    let track_row_inner = row(cells)
        .push(like_icon)
        .spacing(0)
        .align_y(Alignment::Center)
        .padding([0, 12]);

    let row_styled: Element<'static, Message> = if is_selected {
        container(track_row_inner)
            .style(theme::selected_row)
            .height(Length::Fixed(ROW_H))
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center)
            .into()
    } else if is_current {
        container(track_row_inner)
            .style(|_| iced::widget::container::Style {
                background: Some(iced::Background::Color(theme::with_alpha(theme::accent(), 0.08))),
                ..Default::default()
            })
            .height(Length::Fixed(ROW_H))
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center)
            .into()
    } else {
        container(track_row_inner)
            .height(Length::Fixed(ROW_H))
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center)
            .into()
    };

    let select_msg = Message::SelectTrack(track.clone());
    let ctx_msg = if is_multi {
        Message::ToggleContextMenu(Some(ContextMenuTarget::MultipleTracks(multi_selected.to_vec())))
    } else {
        Message::ToggleContextMenu(Some(ContextMenuTarget::Track(track)))
    };

    let row_btn = button(row_styled)
        .on_press(select_msg)
        .style(iced::widget::button::text)
        .width(Length::Fill)
        .padding(0);

    mouse_area(row_btn).on_right_press(ctx_msg).into()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn column_widths(cols: &[TableColumn]) -> Vec<Length> {
    cols.iter()
        .map(|col| match col {
            TableColumn::TrackNumber => Length::Fixed(32.0),
            TableColumn::Title => Length::FillPortion(4),
            TableColumn::Artist => Length::FillPortion(3),
            TableColumn::Album => Length::FillPortion(3),
            TableColumn::Genre => Length::FillPortion(2),
            TableColumn::Year => Length::Fixed(48.0),
            TableColumn::DiscNumber => Length::Fixed(44.0),
            TableColumn::Duration => Length::Fixed(78.0),
            TableColumn::Plays => Length::Fixed(56.0),
            TableColumn::DatePlayed => Length::Fixed(110.0),
        })
        .collect()
}

fn table_col_to_sort(col: TableColumn) -> SortColumn {
    match col {
        TableColumn::TrackNumber => SortColumn::TrackNumber,
        TableColumn::Title => SortColumn::Title,
        TableColumn::Artist => SortColumn::Artist,
        TableColumn::Album => SortColumn::Album,
        TableColumn::Genre => SortColumn::Genre,
        TableColumn::Year => SortColumn::Year,
        TableColumn::DiscNumber => SortColumn::DiscNumber,
        TableColumn::Duration => SortColumn::Duration,
        TableColumn::Plays => SortColumn::Plays,
        TableColumn::DatePlayed => SortColumn::DatePlayed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_rows_near_the_view_are_built() {
        let rows = vec![ROW_H; 3000]; // 102 000 px of songs
        // At the top: from row 0, as far as the view and the overscan reach.
        let (shown, above, below) = visible_items(&rows, 0.0, 700.0);
        assert_eq!(shown.start, 0);
        assert_eq!(shown.end, ((700.0 + OVERSCAN_PX) / ROW_H).floor() as usize + 1);
        assert_eq!(above, 0.0);
        assert!((above + shown.len() as f32 * ROW_H + below - 3000.0 * ROW_H).abs() < 0.01, "total height kept");
        // Somewhere in the middle: about view + 2 overscans of rows, not 3000.
        let (shown, above, _) = visible_items(&rows, 34_000.0, 700.0);
        assert!(shown.len() < 60, "{} rows built", shown.len());
        assert!(above > 0.0 && shown.contains(&1000));
        // A position left over from a longer list: shown from its end.
        let short = vec![ROW_H; 5];
        let (shown, above, below) = visible_items(&short, 34_000.0, 700.0);
        assert_eq!((shown, above, below), (0..5, 0.0, 0.0));
        // Empty list.
        assert_eq!(visible_items(&[], 500.0, 700.0).0, 0..0);
    }
}
