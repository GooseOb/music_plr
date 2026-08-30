use iced::{
    alignment,
    widget::{container, image, text, text_input, Button, Column, Container, Id, ProgressBar, Row},
    Color, Element, Length,
};

use super::styles::{
    bg_secondary, button_style_primary, button_style_scope, fg_accent, fg_secondary, icon_fg_muted,
    icon_primary,
};
use crate::{
    app::{
        dependency_dialog::DepOpState,
        ui::{spinner::spinner, styles::icon_playbar_button},
        Message,
    },
    i18n::Strings,
    icons,
    theme::{self, AppTheme},
};

pub fn thumbnail(size: f32, thumb: Option<&std::path::PathBuf>) -> Element<'_, Message, AppTheme> {
    if let Some(path) = thumb {
        image(image::Handle::from_path(path))
            .width(size)
            .height(size)
            .border_radius(size / 4.0)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        icons::icon(icons::MUSIC_ICON, size)
            .style(icon_fg_muted())
            .into()
    }
}

pub fn inner_row_layout<'a>(
    leading: Element<'a, Message, AppTheme>,
    thumbnail: Element<'a, Message, AppTheme>,
    title: &'a str,
    subtitle: Element<'a, Message, AppTheme>,
    trailing: Element<'a, Message, AppTheme>,
) -> Row<'a, Message, AppTheme> {
    Row::with_children([
        leading,
        thumbnail,
        Column::with_children([
            text(title)
                .size(theme::TEXT_SIZE_MD)
                .width(Length::Fill)
                .into(),
            subtitle,
        ])
        .spacing(theme::SPACING_2XS)
        .into(),
        trailing,
    ])
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
    if let Some(id) = id {
        container.id(id)
    } else {
        container
    }
}

pub fn empty_state<'a>(msg: impl text::IntoFragment<'a>) -> Element<'a, Message, AppTheme> {
    Container::new(text(msg).style(fg_secondary()))
        .center(Length::Fill)
        .into()
}

pub fn loading_state<'a>(msg: impl text::IntoFragment<'a>) -> Element<'a, Message, AppTheme> {
    iced::widget::Container::new(
        iced::widget::Column::with_children([
            spinner(theme::SPINNER_SIZE),
            text::Text::new(msg).style(fg_secondary()).into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_x(alignment::Horizontal::Center),
    )
    .center(Length::Fill)
    .into()
}

pub fn scope_button(
    label: impl text::IntoFragment<'static>,
    selected: bool,
) -> Button<'static, Message, AppTheme> {
    Button::new(text(label).size(theme::TEXT_SIZE_SM))
        .padding([theme::SPACING_XS, theme::SPACING_SM])
        .style(button_style_scope(selected))
}

/// Build a segmented row of scope/provider chips. Each item is
/// `(label, selected, on_press)`; `pad_x`/`pad_y` set the button padding.
pub fn scope_tab_row<I, S>(items: I) -> Element<'static, Message, AppTheme>
where
    I: IntoIterator<Item = (S, bool, Message)>,
    S: text::IntoFragment<'static>,
{
    let tabs = items
        .into_iter()
        .map(|(label, selected, on_press)| scope_button(label, selected).on_press(on_press).into());
    Row::with_children(tabs)
        .spacing(theme::SPACING_XS)
        .wrap()
        .into()
}

pub fn play_pause_button(is_playing: bool) -> Button<'static, Message, AppTheme> {
    Button::new(
        icons::icon(
            if is_playing {
                icons::PAUSE_ICON
            } else {
                icons::PLAY_ICON
            },
            theme::ICON_SIZE_LG,
        )
        .style(icon_primary()),
    )
    .style(button_style_primary())
}

pub fn toggle_bookmark_button(is_saved: bool) -> Button<'static, Message, AppTheme> {
    Button::new(
        icons::icon(icons::BOOKMARK_ICON, theme::ICON_SIZE_SM).style(icon_playbar_button(is_saved)),
    )
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
            .on_press(Message::OpenArtist {
                id,
                name: name.to_string(),
                source,
            })
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

/// Download progress bar with a live percentage label, for dependency installs.
/// Shared by the startup dialog and the Settings Dependencies section so the
/// progress rendering can't drift between the two. Callers must ensure `total`
/// is non-zero (otherwise a plain "installing" label is more appropriate).
pub fn dep_progress_bar(downloaded: u64, total: u64) -> Element<'static, Message, AppTheme> {
    let pct = ((downloaded as f64 / total as f64) * 100.0) as u16;
    let bar = Container::new(ProgressBar::new(
        std::ops::RangeInclusive::new(0.0, total as f32),
        downloaded as f32,
    ))
    .height(8)
    .style(bg_secondary());
    Column::with_children([
        bar.into(),
        text(format!("{pct}%"))
            .size(theme::TEXT_SIZE_XS)
            .style(fg_secondary())
            .into(),
    ])
    .spacing(theme::SPACING_XS)
    .into()
}

/// Install-phase status (installing / installed / failed) for a dependency,
/// shared by the startup dialog and the Settings Dependencies section so the
/// status text can't drift. Returns `None` when there is no install state yet
/// (e.g. an uninstalled dep, or a dep waiting on delete/base-state text), letting
/// the caller layer its own branches on top.
pub fn dep_install_status(
    op: Option<&DepOpState>,
    tr: &Strings,
) -> Option<Element<'static, Message, AppTheme>> {
    if op.is_some_and(|o| o.installing) {
        return Some(
            match op.and_then(|o| (o.progress.1 > 0).then_some(o.progress)) {
                Some((downloaded, total)) => dep_progress_bar(downloaded, total),
                None => text(tr.deps_installing).style(fg_secondary()).into(),
            },
        );
    }
    if let Some(res) = op.and_then(|o| o.install_result.as_ref()) {
        return Some(match res {
            Ok(()) => text(tr.deps_installed).style(fg_accent()).into(),
            Err(e) => text(format!("{}: {}", tr.deps_failed, e))
                .style(fg_secondary())
                .into(),
        });
    }
    None
}
