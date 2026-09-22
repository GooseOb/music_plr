use iced::{
    alignment,
    widget::{text, Button, Column, Container, MouseArea, Row, Space, Stack},
    Element, Length,
};

use super::{
    content::view_main_content,
    styles::{button_style_nav, fg_secondary},
    theme, Message, MusicPlayer,
};
use crate::{
    app::{pane::PaneId, ui::styles::icon_color, ViewKind},
    icons,
    theme::{AppTheme, Palette},
};

pub(super) fn view_split_tree<'a>(
    player: &'a MusicPlayer,
    node: &'a crate::app::pane::SplitNode,
) -> Element<'a, Message, AppTheme> {
    match node {
        crate::app::pane::SplitNode::Leaf(pane) => view_pane(player, *pane),
        crate::app::pane::SplitNode::Row { first, second } => Row::with_children([
            Container::new(view_split_tree(player, first))
                .width(Length::FillPortion(1))
                .height(Length::Fill)
                .into(),
            Container::new(view_split_tree(player, second))
                .width(Length::FillPortion(1))
                .height(Length::Fill)
                .into(),
        ])
        .height(Length::Fill)
        .into(),
        crate::app::pane::SplitNode::Column { first, second } => Column::with_children([
            Container::new(view_split_tree(player, first))
                .height(Length::FillPortion(1))
                .width(Length::Fill)
                .into(),
            Container::new(view_split_tree(player, second))
                .height(Length::FillPortion(1))
                .width(Length::Fill)
                .into(),
        ])
        .width(Length::Fill)
        .into(),
    }
}

fn view_pane(player: &MusicPlayer, pane: PaneId) -> Element<'_, Message, AppTheme> {
    let multi = player.split_root.leaf_count() > 1;
    let focused = multi && player.focused_pane_id == pane;
    let mut children = Vec::with_capacity(2);
    if multi {
        children.push(view_pane_header(player, pane));
    }
    children.push(view_main_content(player, pane));

    let body = Column::with_children(children)
        .width(Length::Fill)
        .height(Length::Fill);
    let content: Element<'_, Message, AppTheme> = MouseArea::new(body)
        .on_press(Message::FocusPane(pane))
        .on_enter(Message::FocusPane(pane))
        .into();
    // The Stack and its overlay are always rendered (even unfocused and
    // single-pane) so the widget tree keeps a stable shape across focus
    // changes — iced reconciles widget state positionally by type, and
    // adding/removing the wrapper would discard the scrollables' scroll
    // offsets. Only the ring's width toggles. The overlay is a plain
    // container, so it never intercepts mouse events meant for the content
    // beneath, and a zero-width border draws nothing.
    let ring_width = if focused { 2.0 } else { 0.0 };
    Stack::with_children([
        content,
        Container::new(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |theme: &AppTheme| iced::widget::container::Style {
                border: iced::Border {
                    width: ring_width,
                    color: theme.palette.accent,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into(),
    ])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_pane_header(player: &MusicPlayer, pane: PaneId) -> Element<'_, Message, AppTheme> {
    let can_back = player.can_navigate_back(pane);
    let can_forward = player.can_navigate_forward(pane);
    let p = player.app_theme.palette;
    Row::with_children([
        header_button(&p, can_back, icons::BACK_ICON, Message::NavigateBack(pane)).into(),
        header_button(
            &p,
            can_forward,
            icons::FORWARD_ICON,
            Message::NavigateForward(pane),
        )
        .into(),
        text(pane_title(player, pane))
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .width(Length::Fill)
            .into(),
        header_button(&p, true, icons::CLOSE_ICON, Message::ClosePane(pane)).into(),
    ])
    .spacing(theme::SPACING_XS)
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_XS, theme::SPACING_SM])
    .into()
}

fn header_button(
    p: &Palette,
    can: bool,
    icon_data: &'static [u8],
    on_press: Message,
) -> Button<'static, Message, AppTheme> {
    Button::new(
        icons::icon(icon_data, theme::ICON_SIZE_SM).style(icon_color(if can {
            p.fg
        } else {
            p.fg_muted
        })),
    )
    .padding(theme::SPACING_XS)
    .style(button_style_nav(can))
    .on_press_maybe(can.then_some(on_press))
}

fn pane_title(player: &MusicPlayer, pane: PaneId) -> String {
    match &player.view_data_in(pane).kind {
        ViewKind::Search(s) if s.query.is_empty() => player.strings.search.to_string(),
        ViewKind::Search(s) => s.query.clone(),
        ViewKind::SongRadio(label) | ViewKind::ArtistRadio(label) => label.clone(),
        ViewKind::Artist(entry) => entry.name.clone(),
        ViewKind::Album(r) => r.name.clone(),
        ViewKind::PlaylistView(r) => r.name.clone(),
        ViewKind::Playlist(entry) => entry.name.clone(),
        ViewKind::Downloads => player.strings.downloads.to_string(),
        ViewKind::Settings => player.strings.settings.to_string(),
    }
}
