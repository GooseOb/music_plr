use iced::{
    alignment,
    widget::{
        container, image, scrollable, svg, text, text_input, Button, Column, Container, Id,
        MouseArea, ProgressBar, Row, Space,
    },
    Color, Element, Length,
};

use super::styles::{
    bg_secondary, button_style_album, button_style_primary, button_style_scope, fg_accent,
    fg_secondary, icon_style_fg_muted, icon_style_primary,
};
use crate::{
    app::{
        dependency_dialog::DepOpState,
        interaction::{CardTarget, HoverTarget, Pressed},
        pane::PaneId,
        ui::{
            spinner::spinner,
            styles::{
                button_style_danger, button_style_playbar, icon_style_fg, icon_style_playbar_button,
            },
        },
        update::operation::{RailKey, ScrollTarget},
        Message, ViewKind,
    },
    data::library::Card,
    i18n::{Direction, Strings},
    icons,
    theme::{self, AppTheme},
};

pub fn thumbnail(size: f32, thumb: Option<&std::path::PathBuf>) -> Element<'_, Message, AppTheme> {
    if let Some(path) = thumb.filter(|p| p.exists()) {
        image(image::Handle::from_path(path))
            .width(size)
            .height(size)
            .border_radius(size / 4.0)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        icons::icon(icons::MUSIC_ICON, size)
            .style(icon_style_fg_muted())
            .into()
    }
}

/// The cover of a page assembled from its first four tracks' thumbnails: a
/// flush 2×2 board whose tiles round only their outward corner of the cover
/// (the widget primitives can't clip children into a rounded silhouette).
/// With no artwork at all it degrades to [`thumbnail`]'s own placeholder
/// glyph, so a freshly created playlist looks like every other page without
/// an image.
pub fn thumbnail_mosaic(
    size: f32,
    tiles: [Option<&std::path::PathBuf>; 4],
) -> Element<'_, Message, AppTheme> {
    if tiles.iter().all(|t| t.is_none_or(|p| !p.exists())) {
        return thumbnail(size, None);
    }
    let half = size / 2.0;
    let corner = half / 4.0;
    let tile = |thumb: Option<&std::path::PathBuf>,
                radius: iced::border::Radius|
     -> Element<'_, Message, AppTheme> {
        match thumb.filter(|p| p.exists()) {
            Some(path) => image(image::Handle::from_path(path))
                .width(half)
                .height(half)
                .border_radius(reflected(radius))
                .content_fit(iced::ContentFit::Cover)
                .into(),
            None => Container::new(Space::new())
                .width(half)
                .height(half)
                .style(move |theme: &AppTheme| container::Style {
                    background: Some(theme.palette.overlay.into()),
                    border: iced::border::rounded(radius),
                    ..Default::default()
                })
                .into(),
        }
    };
    let corner_radius =
        |top_left: f32, top_right: f32, bottom_left: f32, bottom_right: f32| iced::border::Radius {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        };
    Column::with_children([
        Row::with_children([
            tile(tiles[0], corner_radius(corner, 0.0, 0.0, 0.0)),
            tile(tiles[1], corner_radius(0.0, corner, 0.0, 0.0)),
        ])
        .into(),
        Row::with_children([
            tile(tiles[2], corner_radius(0.0, 0.0, corner, 0.0)),
            tile(tiles[3], corner_radius(0.0, 0.0, 0.0, corner)),
        ])
        .into(),
    ])
    .into()
}

/// The radius as `iced_wgpu`'s image shader applies it: point-reflected, so the
/// `top_left` field rounds the visual bottom-right and every other field its
/// opposite corner. Container quads round the named corners, so image tiles
/// pass their outward corner through here while board tiles don't.
fn reflected(radius: iced::border::Radius) -> iced::border::Radius {
    iced::border::Radius {
        top_left: radius.bottom_right,
        top_right: radius.bottom_left,
        bottom_right: radius.top_left,
        bottom_left: radius.top_right,
    }
}

/// A section heading, shared by the settings groups and the Home feed's
/// provider blocks: accent-colored and one step above body text, aligned to the
/// reading start so a right-to-left locale puts it on the right.
pub fn section_label<'a>(
    dir: Direction,
    label: impl text::IntoFragment<'a>,
) -> Element<'a, Message, AppTheme> {
    dir.leading(text(label).size(theme::TEXT_SIZE_LG).style(fg_accent()))
}

pub fn inner_row_layout<'a>(
    dir: Direction,
    leading: Element<'a, Message, AppTheme>,
    thumbnail: Element<'a, Message, AppTheme>,
    title: &'a str,
    subtitle: Element<'a, Message, AppTheme>,
    trailing: Element<'a, Message, AppTheme>,
) -> Row<'a, Message, AppTheme> {
    let align = dir.align();
    dir.row([
        leading,
        thumbnail,
        Column::with_children([
            text(title)
                .size(theme::TEXT_SIZE_MD)
                .width(Length::Fill)
                .align_x(align)
                .into(),
            subtitle,
        ])
        .spacing(theme::SPACING_2XS)
        .align_x(align)
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

/// The magnifier button that runs a search: the search bar's, reused by the
/// Edit popup's manual-search step so both read as the same control.
/// `on_press` is `None` while no query can run (blank query, or one already
/// in flight), which renders it disabled.
pub fn search_button(on_press: Option<Message>) -> Button<'static, Message, AppTheme> {
    Button::new(icons::icon(icons::SEARCH_ICON, theme::ICON_SIZE_LG).style(icon_style_primary()))
        .padding(theme::SPACING_SM)
        .style(button_style_primary())
        .on_press_maybe(on_press)
}

/// Build a segmented row of scope/provider chips. Each item is
/// `(label, selected, on_press)`; `pad_x`/`pad_y` set the button padding.
pub fn scope_tab_row<I, S>(dir: Direction, items: I) -> Element<'static, Message, AppTheme>
where
    I: IntoIterator<Item = (S, bool, Message)>,
    S: text::IntoFragment<'static>,
{
    let tabs = items
        .into_iter()
        .map(|(label, selected, on_press)| scope_button(label, selected).on_press(on_press).into());
    dir.row(tabs)
        .spacing(theme::SPACING_XS)
        .wrap()
        .align_x(dir.align())
        .into()
}

/// Horizontally scrollable variant of [`scope_tab_row`]: same chips, no
/// wrapping, overflowing tabs scroll instead of spilling onto extra lines.
/// The message is `Option` so tabs can be disabled (`None`); `Fill` parents
/// control the scroller footprint (e.g. a right-aligned trailing row after
/// a `Fill` sibling).
pub fn scope_tab_row_h_scroll<I, S>(items: I) -> Element<'static, Message, AppTheme>
where
    I: IntoIterator<Item = (S, bool, Option<Message>)>,
    S: text::IntoFragment<'static>,
{
    let tabs = items.into_iter().map(|(label, selected, on_press)| {
        scope_button(label, selected)
            .on_press_maybe(on_press)
            .into()
    });
    scrollable(Row::with_children(tabs).spacing(theme::SPACING_XS))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(theme::SPACING_XXS)
                .scroller_width(theme::SPACING_XXS),
        ))
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
        .style(icon_style_primary()),
    )
    .style(button_style_primary())
}

pub fn toggle_bookmark_button(is_saved: bool) -> Button<'static, Message, AppTheme> {
    Button::new(
        icons::icon(icons::BOOKMARK_ICON, theme::ICON_SIZE_SM)
            .style(icon_style_playbar_button(is_saved)),
    )
    .padding(theme::SPACING_SM)
    .style(button_style_scope(is_saved))
}

/// The header button that opens a page in the desktop's browser. Shared by
/// every page that has one (artist headers, album/playlist pages).
pub fn open_in_browser_button(
    tr: &'static crate::i18n::Strings,
    url: &str,
) -> Element<'static, Message, AppTheme> {
    text_button(tr.ctx_open_in_browser)
        .on_press(Message::OpenInBrowser(url.to_string()))
        .into()
}

pub fn subtitle_artists<'a>(
    pane: PaneId,
    track: &'a crate::types::Track,
    dir: Direction,
) -> Element<'a, Message, AppTheme> {
    if track.artists.is_empty() {
        return Space::new().into();
    }
    let mut children: Vec<Element<'a, Message, AppTheme>> =
        Vec::with_capacity(track.artists.len() * 2);
    for (index, name) in track.artists.iter().enumerate() {
        if index > 0 {
            children.push(
                text(", ")
                    .size(theme::TEXT_SIZE_SM)
                    .style(fg_secondary())
                    .into(),
            );
        }
        let name_element = text(name).size(theme::TEXT_SIZE_SM);
        children.push(match track.artist_target(index) {
            Some((id, source)) => link_button(name_element)
                .on_press(Message::OpenArtist {
                    pane,
                    url: source.artist_page_url(id).unwrap_or_default(),
                    id: id.to_string(),
                    name: name.clone(),
                    source,
                })
                .into(),
            None => name_element.style(fg_secondary()).into(),
        });
    }
    dir.row(children).into()
}

pub fn labeled_input<'a>(
    dir: Direction,
    label: &'a str,
    value: &str,
    placeholder: &'a str,
    on_input: Option<fn(String) -> Message>,
) -> Element<'a, Message, AppTheme> {
    let mut input = text_input(placeholder, value)
        .align_x(dir.align())
        .padding([theme::SPACING_SM, theme::SPACING_MD]);
    if let Some(on_input) = on_input {
        input = input.on_input(on_input);
    }
    Column::with_children([
        Container::new(text(label))
            .padding([0.0, theme::SPACING_XS])
            .into(),
        input.into(),
    ])
    .spacing(theme::SPACING_XS)
    .align_x(dir.align())
    .into()
}

pub fn disabled_text_input_row<'a>(
    dir: Direction,
    label: &'a str,
    value: &str,
) -> Element<'a, Message, AppTheme> {
    labeled_input(dir, label, value, "", None::<fn(String) -> Message>)
}

pub fn text_input_row<'a>(
    dir: Direction,
    label: &'a str,
    value: &str,
    placeholder: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message, AppTheme> {
    labeled_input(dir, label, value, placeholder, Some(on_input))
}

pub fn load_state_tracks<'a>(
    content: &'a crate::load_state::LoadState<Vec<crate::types::Track>>,
    tr: &Strings,
    loading_msg: &'a str,
) -> Result<&'a [crate::types::Track], Element<'a, Message, AppTheme>> {
    match content {
        crate::load_state::LoadState::Ready(tracks) => Ok(tracks),
        crate::load_state::LoadState::Loading => Err(loading_state(loading_msg)),
        crate::load_state::LoadState::Failed(e) => Err(empty_state((tr.couldnt_load)(e))),
    }
}

/// Download progress bar with a live percentage label, for dependency installs.
/// Shared by the startup dialog and the Settings Dependencies section so the
/// progress rendering can't drift between the two. Callers must ensure `total`
/// is non-zero (otherwise a plain "installing" label is more appropriate).
pub fn dep_progress_bar(
    downloaded: u64,
    total: u64,
    tr: &Strings,
) -> Element<'static, Message, AppTheme> {
    let pct = ((downloaded as f64 / total as f64) * 100.0) as u64;
    let bar = Container::new(ProgressBar::new(
        std::ops::RangeInclusive::new(0.0, total as f32),
        downloaded as f32,
    ))
    .height(8)
    .style(bg_secondary());
    Column::with_children([
        bar.into(),
        text((tr.percent)(pct))
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
                Some((downloaded, total)) => dep_progress_bar(downloaded, total, tr),
                None => text(tr.deps_installing).style(fg_secondary()).into(),
            },
        );
    }
    if let Some(res) = op.and_then(|o| o.install_result.as_ref()) {
        return Some(match res {
            Ok(()) => text(tr.deps_installed).style(fg_accent()).into(),
            Err(e) => text((tr.deps_failed_detail)(e.as_str()))
                .style(fg_secondary())
                .into(),
        });
    }
    None
}

pub fn dialog_button(
    label: impl text::IntoFragment<'static>,
) -> Button<'static, Message, AppTheme> {
    Button::new(Container::new(text(label)).center_x(Length::Fill)).padding(theme::SPACING_SM)
}

pub fn dialog_button_row(
    dir: Direction,
    buttons: impl IntoIterator<Item = Element<'static, Message, AppTheme>>,
) -> Row<'static, Message, AppTheme> {
    dir.row(buttons)
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
}

pub fn text_button(label: impl text::IntoFragment<'static>) -> Button<'static, Message, AppTheme> {
    Button::new(text(label)).padding([theme::SPACING_XS, theme::SPACING_MD])
}

pub fn icon_button(
    icon_data: &'static [u8],
    size: f32,
    style: impl Fn(&AppTheme, svg::Status) -> svg::Style + 'static,
) -> Button<'static, Message, AppTheme> {
    Button::new(icons::icon(icon_data, size).style(style)).padding(theme::SPACING_2XS)
}

pub fn icon_button_delete() -> Button<'static, Message, AppTheme> {
    icon_button(icons::DELETE_ICON, theme::ICON_SIZE_MD, icon_style_fg())
        .style(button_style_danger())
}

pub fn link_button<'a>(
    content: impl Into<Element<'a, Message, AppTheme>>,
) -> Button<'a, Message, AppTheme> {
    Button::new(content).padding(0).style(button_style_album())
}

pub fn playbar_toggle_button_unsized(
    icon_data: &'static [u8],
    active: bool,
) -> Button<'static, Message, AppTheme> {
    icon_button(
        icon_data,
        theme::ICON_SIZE_MD,
        icon_style_playbar_button(active),
    )
    .style(button_style_playbar(active))
}

pub fn playbar_toggle_button(
    icon_data: &'static [u8],
    active: bool,
) -> Button<'static, Message, AppTheme> {
    playbar_toggle_button_unsized(icon_data, active)
        .width(theme::QUEUE_BTN_WIDTH)
        .height(theme::QUEUE_BTN_WIDTH)
}

/// Width of one square art card and of its picture, shared by every card shelf
/// (an artist page's sections, the Home feed's rails).
pub const CARD_WIDTH: f32 = 140.0;
pub const CARD_IMAGE_SIZE: f32 = 120.0;

/// A horizontal shelf of cards: the row in a sideways scrollable that reports
/// its own offset, so a card's right-click can be mapped back to window
/// coordinates. Callers decide what an empty rail draws.
pub fn card_rail<'a, I>(pane: PaneId, rail: RailKey, cards: I) -> Element<'a, Message, AppTheme>
where
    I: IntoIterator<Item = Element<'a, Message, AppTheme>>,
{
    scrollable(
        Row::with_children(cards)
            .spacing(theme::SPACING_MD)
            .padding([theme::SPACING_SM, theme::SPACING_XL]),
    )
    .id(super::card_row_id(pane, rail))
    .direction(scrollable::Direction::Horizontal(
        scrollable::Scrollbar::new(),
    ))
    .on_scroll(move |vp| super::scrolled(ScrollTarget::CardRow(pane, rail), vp.absolute_offset()))
    .into()
}

/// The body every tile shares: the picture with its title underneath and an
/// optional subtitle. Both [`art_card`] and [`click_card`] draw it, so the two
/// kinds of tile cannot drift apart.
fn card_body<'a>(
    image: Element<'a, Message, AppTheme>,
    title: &'a str,
    subtitle: &str,
    title_style: impl Fn(&AppTheme) -> iced::widget::text::Style + 'a,
) -> Container<'a, Message, AppTheme> {
    let mut body = vec![
        image,
        text(title)
            .size(theme::TEXT_SIZE_SM)
            .width(CARD_WIDTH)
            .wrapping(text::Wrapping::Word)
            .style(title_style)
            .into(),
    ];
    let trimmed_subtitle = subtitle.trim();
    if !trimmed_subtitle.is_empty() {
        body.push(
            text(trimmed_subtitle.to_string())
                .size(theme::TEXT_SIZE_XS)
                .style(fg_secondary())
                .into(),
        );
    }
    Container::new(Column::with_children(body).spacing(theme::SPACING_2XS))
        .padding(theme::SPACING_2XS)
        .width(CARD_WIDTH)
}

/// One square art card on a shelf: picture with title and subtitle underneath.
/// It is a full drag-and-drop citizen — a press starts a card drag (onto a
/// playlist, the library or the queue), a plain click opens `target` and a
/// right-click opens the card menu — and hovering recolours its title.
pub fn art_card<'a>(
    player: &'a super::MusicPlayer,
    pane: PaneId,
    target: ViewKind,
    title: &'a str,
    subtitle: &str,
    rail: RailKey,
) -> Element<'a, Message, AppTheme> {
    let card = target
        .card_ref()
        .expect("a card shelf only renders artist, album and playlist cards");
    let item = Card::new(
        target
            .library_item()
            .expect("a card shelf only renders artist, album and playlist cards"),
    );
    let is_hovered = player.drag.is_hovered_card(&item);
    let is_dragging_this = matches!(
        player.drag.pressed,
        Some(crate::app::interaction::PressedDrag {
            what: Pressed::Card(ref c, _),
            ..
        }) if c == &item
    );
    let hot = is_hovered || is_dragging_this;
    let thumb_path = player.thumbnail_index.get(card.provider, card.id);
    let image = Container::new(thumbnail(CARD_IMAGE_SIZE, thumb_path)).height(CARD_IMAGE_SIZE);
    let body = card_body(image.into(), title, subtitle, move |t: &AppTheme| {
        iced::widget::text::Style {
            color: Some(if hot {
                t.palette.fg
            } else {
                t.palette.fg_secondary
            }),
        }
    });
    let area = MouseArea::new(body)
        .interaction(player.drag.clickable_cursor_interaction())
        .on_press(Message::DragPress(Pressed::Card(item.clone(), Some(pane))))
        .on_enter(Message::HoverStart(HoverTarget::Card(item.clone())))
        .on_exit(Message::HoverEnd(HoverTarget::Card(item)));
    super::right_click::RightClickArea::new(
        area,
        CardTarget {
            host: ScrollTarget::Cards(pane),
            pane,
            target,
            rail: Some(rail),
        },
        CardTarget::into_message,
    )
    .into()
}

/// A tile that only carries a click: the same shape as [`art_card`] for things
/// with no card identity of their own (a track, a local playlist), so it has
/// neither a drag nor a menu.
pub fn click_card<'a>(
    image: Element<'a, Message, AppTheme>,
    title: &'a str,
    subtitle: &str,
    message: Message,
) -> Element<'a, Message, AppTheme> {
    MouseArea::new(card_body(image, title, subtitle, |_| {
        iced::widget::text::Style::default()
    }))
    .on_press(message)
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}
