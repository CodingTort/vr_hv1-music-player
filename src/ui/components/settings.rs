use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input, Space};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::ui::{icons, theme};

#[allow(clippy::too_many_arguments)] // one argument per setting shown
pub fn view<'a>(
    theme_mode: &'a str,
    theme_preset: &'a str,
    use_glassmorphism: bool,
    background_kind: &'a str,
    background_color: &'a str,
    background_gradient_from: &'a str,
    background_gradient_to: &'a str,
    background_gradient_angle: f32,
    background_image: &'a str,
    corner_radius: f32,
    spectrum_style: &'a str,
    cover_size: f32,
) -> Element<'a, Message> {
    const THEME_MODES: &[&str] = &["auto", "manual"];
    const SPECTRUM_STYLES: &[&str] = &["mirrored", "bottom"];
    const BG_KINDS: &[&str] = &["none", "color", "gradient", "image", "album"];
    let presets = theme::get_available_presets();

    let content = column![
        row![
            text("Settings").size(18).font(icons::UI_FONT_BOLD).color(theme::accent()),
            Space::with_width(Length::Fill),
            button(
                text(icons::ICON_CLOSE).font(icons::NERD_FONT_MONO).color(theme::red()).size(32)
            )
            .on_press(Message::CloseSettings)
            .style(iced::widget::button::text),
        ].align_y(Alignment::Center),
        Space::with_height(16),
        row![
            text("Theme Mode").color(theme::text()).size(13).width(Length::Fixed(120.0)),
            pick_list(THEME_MODES, Some(theme_mode), |s| Message::SettingsThemeModeChanged(s.to_string()))
                .style(theme::pick_list)
                .padding([4, 8])
                .width(Length::Fill),
        ].spacing(12).align_y(Alignment::Center),
        row![
            text("Theme Preset").color(theme::text()).size(13).width(Length::Fixed(120.0)),
            pick_list(presets, Some(theme_preset), |s| Message::SettingsThemePresetChanged(s.to_string()))
                .style(theme::pick_list)
                .padding([4, 8])
                .width(Length::Fill),
        ].spacing(12).align_y(Alignment::Center),
        row![
            text("Corner Radius").color(theme::text()).size(13).width(Length::Fixed(120.0)),
            iced::widget::slider(4.0..=20.0, corner_radius, Message::SettingsCornerRadiusChanged)
                .width(Length::Fill),
            text(format!("{:.1}px", corner_radius)).color(theme::text()).size(13).width(Length::Fixed(48.0)),
        ].spacing(8).align_y(Alignment::Center),
        row![
            text("Cover Size").color(theme::text()).size(13).width(Length::Fixed(120.0)),
            iced::widget::slider(140.0..=400.0, cover_size, Message::SettingsCoverSizeChanged)
                .step(10.0_f32)
                .width(Length::Fill),
            text(format!("{cover_size:.0}px")).color(theme::text()).size(13).width(Length::Fixed(48.0)),
        ].spacing(8).align_y(Alignment::Center),
        row![
            text("Spectrum").color(theme::text()).size(13).width(Length::Fixed(120.0)),
            pick_list(SPECTRUM_STYLES, Some(spectrum_style), |s| Message::SettingsSpectrumStyleChanged(s.to_string()))
                .style(theme::pick_list)
                .padding([4, 8])
                .width(Length::Fill),
        ].spacing(12).align_y(Alignment::Center),
        row![
            Space::with_width(Length::Fixed(120.0)),
            checkbox("Use glassmorphism", use_glassmorphism)
                .on_toggle(Message::SettingsGlassmorphismToggled),
        ].align_y(Alignment::Center),
        row![
            text("Background").color(theme::text()).size(13).width(Length::Fixed(120.0)),
            pick_list(BG_KINDS, Some(background_kind), |s| Message::SettingsBackgroundKindChanged(s.to_string()))
                .style(theme::pick_list)
                .padding([4, 8])
                .width(Length::Fill),
        ].spacing(12).align_y(Alignment::Center),
        if background_kind == "color" {
            row![
                text("BG Color").color(theme::text()).size(13).width(Length::Fixed(120.0)),
                text_input("#HEX", background_color)
                    .on_input(Message::SettingsBackgroundColorChanged)
                    .style(theme::dialog_input)
                    .width(Length::Fill),
            ].spacing(8).align_y(Alignment::Center)
        } else { row![Space::new(Length::Fill, Length::Fixed(0.0))] },
        if background_kind == "gradient" {
            column![
                row![
                    text("From").color(theme::text()).size(13).width(Length::Fixed(120.0)),
                    text_input("#HEX", background_gradient_from)
                        .on_input(Message::SettingsBackgroundGradientFromChanged)
                        .style(theme::dialog_input)
                        .width(Length::Fill),
                ].spacing(8).align_y(Alignment::Center),
                row![
                    text("To").color(theme::text()).size(13).width(Length::Fixed(120.0)),
                    text_input("#HEX", background_gradient_to)
                        .on_input(Message::SettingsBackgroundGradientToChanged)
                        .style(theme::dialog_input)
                        .width(Length::Fill),
                ].spacing(8).align_y(Alignment::Center),
                row![
                    text("Angle").color(theme::text()).size(13).width(Length::Fixed(120.0)),
                    iced::widget::slider(0.0..=360.0, background_gradient_angle, Message::SettingsBackgroundGradientAngleChanged)
                        .width(Length::Fill),
                    text(format!("{:.0}°", background_gradient_angle)).color(theme::text()).size(13).width(Length::Fixed(48.0)),
                ].spacing(8).align_y(Alignment::Center),
            ].spacing(8)
        } else { column![] },
        if background_kind == "image" {
            row![
                text("Image Path").color(theme::text()).size(13).width(Length::Fixed(120.0)),
                text_input("Path to image...", background_image)
                    .on_input(Message::SettingsBackgroundImageChanged)
                    .style(theme::dialog_input)
                    .width(Length::Fill),
                button(text("Browse..."))
                    .on_press(Message::SettingsSelectBackgroundImage)
                    .style(theme::secondary_button)
                    .padding([4, 8]),
            ].spacing(8).align_y(Alignment::Center)
        } else { row![Space::new(Length::Fill, Length::Fixed(0.0))] },
        Space::with_height(16),
        row![
            Space::with_width(Length::Fill),
            button(text("Apply"))
                .on_press(Message::SettingsApply)
                .style(theme::primary_button)
                .padding([8, 16]),
        ],
    ]
    .spacing(12)
    .padding(24);

    let dialog = container(content)
        .width(560)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(theme::base())),
            border: iced::Border { color: theme::accent(), width: 1.0, radius: crate::ui::theme::corner_radius().into() },
            shadow: iced::Shadow {
                color: theme::mantle(),
                offset: iced::Vector { x: 0.0, y: 4.0 },
                blur_radius: 12.0,
            },
            ..Default::default()
        });

    container(dialog)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.6))),
            ..Default::default()
        })
        .into()
}
