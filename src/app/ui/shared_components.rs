use iced::{
    alignment,
    widget::{container, image, text, text_input, Button, Column, Container, Id, Row},
    Color, Element, Length,
};

use crate::{
    app::Message,
    icons,
    theme::{self, AppTheme, Palette},
};

use super::styles::{button_style_primary, button_style_scope, fg_secondary};

/// Render a thumbnail image if it exists on disk, otherwise a music-note
/// placeholder. `thumb` is the resolved path from the thumbnail index
/// (`Some`) or `None` when not yet downloaded.
pub fn thumbnail<'a>(
    p: &'a Palette,
    size: f32,
    thumb: Option<&'a std::path::PathBuf>,
) -> Element<'a, Message, AppTheme> {
    if let Some(path) = thumb {
        image(image::Handle::from_path(path))
            .width(size)
            .height(size)
            .border_radius(size / 4.0)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        icons::icon(icons::MUSIC_ICON, p.fg_muted, size).into()
    }
}

/// The shared inner row layout used by both track rows and the non-track
/// card rows (artists/albums/playlists): leading | optional thumbnail |
/// title(+subtitle) | optional trailing. `subtitle`/`trailing` are `None`
/// when not needed.
pub fn inner_row_layout<'a>(
    leading: Element<'a, Message, AppTheme>,
    thumbnail: Option<Element<'a, Message, AppTheme>>,
    title: &'a str,
    subtitle: Option<Element<'a, Message, AppTheme>>,
    trailing: Option<Element<'a, Message, AppTheme>>,
) -> Row<'a, Message, AppTheme> {
    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::with_capacity(5);
    children.push(leading);
    if let Some(thumbnail) = thumbnail {
        children.push(thumbnail);
    }
    let title_el = text(title).size(theme::TEXT_SIZE_MD).width(Length::Fill);
    children.push(match subtitle {
        Some(sub) => Column::with_children([title_el.into(), sub])
            .spacing(theme::SPACING_2XS)
            .into(),
        None => title_el.into(),
    });
    if let Some(trailing) = trailing {
        children.push(trailing);
    }
    Row::with_children(children)
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_XS, theme::SPACING_SM])
}

/// Wraps row content in a container with an optional fixed height and
/// background color. `id` (when `Some`) tags the container so the bounds
/// `Operation` can capture its measured geometry for drop-target
/// hit-testing.
pub fn track_row<'a>(
    content: impl Into<Element<'a, Message, AppTheme>>,
    bg: Color,
    id: Option<Id>,
    border: Option<Color>,
) -> Container<'a, Message, AppTheme> {
    let container = Container::new(content)
        .height(theme::ROW_HEIGHT)
        .style(move |_: &AppTheme| container::Style {
            background: Some(bg.into()),
            border: border.map_or(iced::border::Border::default(), |c| iced::border::Border {
                width: 1.0,
                color: c,
                radius: 0.0.into(),
            }),
            ..Default::default()
        });
    match id {
        Some(id) => container.id(id),
        None => container,
    }
}

pub fn empty_state<'a>(msg: impl text::IntoFragment<'a>) -> Element<'a, Message, AppTheme> {
    Container::new(text(msg).style(fg_secondary()).center())
        .padding(theme::SPACING_XL)
        .into()
}

/// Build a segmented row of scope/provider chips. Each item is
/// `(label, selected, on_press)`; `pad_x`/`pad_y` set the button padding.
pub fn scope_tab_row<I, S>(items: I) -> Element<'static, Message, AppTheme>
where
    I: IntoIterator<Item = (S, bool, Message)>,
    S: text::IntoFragment<'static>,
{
    let tabs = items.into_iter().map(|(label, selected, on_press)| {
        Button::new(text(label).size(theme::TEXT_SIZE_SM))
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .style(button_style_scope(selected))
            .on_press(on_press)
            .into()
    });
    Row::with_children(tabs)
        .spacing(theme::SPACING_XS)
        .align_y(alignment::Vertical::Center)
        .wrap()
        .into()
}

pub fn play_pause_button(is_playing: bool) -> Button<'static, Message, AppTheme> {
    Button::new(icons::icon(
        if is_playing {
            icons::PAUSE_ICON
        } else {
            icons::PLAY_ICON
        },
        Color::BLACK,
        theme::ICON_SIZE_LG,
    ))
    .style(button_style_primary())
}

pub fn toggle_bookmark_button(p: &Palette, is_saved: bool) -> Button<'static, Message, AppTheme> {
    Button::new(icons::icon(
        icons::BOOKMARK_ICON,
        if is_saved { Color::BLACK } else { p.fg_muted },
        theme::ICON_SIZE_SM,
    ))
    .padding(theme::SPACING_SM)
    .style(button_style_scope(is_saved))
}

pub fn subtitle_artist(
    name: &str,
    size: u32,
    artist_target: Option<(String, crate::providers::ProviderId)>,
) -> Element<'_, Message, AppTheme> {
    let artist = text(name).size(size);
    if let Some((id, source)) = artist_target {
        Button::new(artist)
            .padding(0)
            .style(super::styles::button_style_album())
            .on_press(Message::Browse(
                crate::app::ViewKind::Artist {
                    id,
                    name: name.to_string(),
                },
                source,
            ))
            .into()
    } else {
        artist.style(fg_secondary()).into()
    }
}

pub fn disabled_text_input_row<'a>(label: &'a str, value: &str) -> Element<'a, Message, AppTheme> {
    Column::with_children([
        Container::new(text(label))
            .padding([0.0, theme::SPACING_XS])
            .into(),
        text_input("", value)
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .into(),
    ])
    .spacing(theme::SPACING_XS)
    .into()
}

pub fn text_input_row<'a>(
    label: &'a str,
    value: &str,
    placeholder: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message, AppTheme> {
    Column::with_children([
        Container::new(text(label))
            .padding([0.0, theme::SPACING_XS])
            .into(),
        text_input(placeholder, value)
            .on_input(on_input)
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .into(),
    ])
    .spacing(theme::SPACING_XS)
    .into()
}
