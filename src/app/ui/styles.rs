use crate::theme::{self, AppTheme, Palette};

use iced::{
    border,
    widget::{button, container, text},
    Color,
};

/// Row background for list rows: `current` is `(idle_alpha, hovered_alpha)`
/// applied to `bg_current` for the current/active row (alpha 1.0 = solid),
/// otherwise `bg_hover` when hovered, else `base`.
pub fn row_bg(p: &Palette, current: Option<(f32, f32)>, hovered: bool, base: Color) -> Color {
    match current {
        Some((idle, hot)) => p.bg_current.scale_alpha(if hovered { hot } else { idle }),
        None if hovered => p.bg_hover,
        None => base,
    }
}

/// Button style skeleton: `bg`/`text_color` receive the palette and whether
/// the button is hovered or pressed; `None` bg means transparent.
pub fn button_style<B, T>(
    bg: B,
    text_color: T,
    radius: f32,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static
where
    B: Fn(&Palette, bool) -> Option<Color> + 'static,
    T: Fn(&Palette, bool) -> Color + 'static,
{
    move |theme, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: bg(&theme.palette, hot).map(Into::into),
            text_color: text_color(&theme.palette, hot),
            border: border::rounded(radius),
            ..Default::default()
        }
    }
}

pub fn bg_secondary() -> impl Fn(&AppTheme) -> container::Style + 'static {
    |theme| container::Style {
        background: Some(theme.palette.bg_secondary.into()),
        ..Default::default()
    }
}

pub fn bg_tertiary() -> impl Fn(&AppTheme) -> container::Style + 'static {
    |theme| container::Style {
        background: Some(theme.palette.bg_tertiary.into()),
        ..Default::default()
    }
}

pub fn bg_overlay() -> impl Fn(&AppTheme) -> container::Style + 'static {
    |theme| container::Style {
        background: Some(theme.palette.overlay.into()),
        ..Default::default()
    }
}

pub fn bg_popup() -> impl Fn(&AppTheme) -> container::Style + 'static {
    move |theme| container::Style {
        background: Some(theme.palette.bg_secondary.into()),
        border: border::rounded(theme::RADIUS_MD),
        ..Default::default()
    }
}

pub fn bg_search_hist() -> impl Fn(&AppTheme) -> container::Style + 'static {
    move |theme| container::Style {
        background: Some(theme.palette.bg_tertiary.into()),
        border: border::rounded(theme::RADIUS_MD),
        ..Default::default()
    }
}

pub fn fg_secondary() -> impl Fn(&AppTheme) -> text::Style + 'static {
    |theme| text::Style {
        color: theme.palette.fg_secondary.into(),
    }
}

pub fn fg_accent() -> impl Fn(&AppTheme) -> text::Style + 'static {
    |theme| text::Style {
        color: theme.palette.accent.into(),
    }
}

pub fn button_style_primary() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        |p, hot| Some(if hot { p.accent_hover } else { p.accent }),
        |_, _| Color::BLACK,
        theme::RADIUS_SM,
    )
}

pub fn button_style_queue(
    enabled: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        move |p, hot| {
            Some(if hot {
                if enabled {
                    p.accent_hover
                } else {
                    p.button_hover
                }
            } else if enabled {
                p.accent
            } else {
                p.button
            })
        },
        move |p, _| if enabled { Color::BLACK } else { p.fg },
        theme::RADIUS_SM,
    )
}

pub fn button_style_danger() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        |p, hot| Some(if hot { p.danger_hover } else { p.danger }),
        |_, _| Color::WHITE,
        theme::RADIUS_SM,
    )
}

pub fn button_style_nav(
    enabled: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        move |p, hot| {
            Some(if enabled {
                if hot {
                    p.button_hover
                } else {
                    p.button
                }
            } else {
                p.bg
            })
        },
        move |p, _| if enabled { p.fg } else { p.fg_muted },
        theme::RADIUS_SM,
    )
}

pub fn button_style_list_item(
    active: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        move |p, hot| {
            Some(if hot {
                p.bg_hover
            } else if active {
                p.bg_current
            } else {
                p.bg_secondary
            })
        },
        move |p, _| if active { p.fg } else { p.fg_secondary },
        theme::RADIUS_SM,
    )
}

pub fn button_style_scope(
    selected: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        move |p, hot| {
            Some(if hot {
                if selected {
                    p.accent_hover
                } else {
                    p.bg_hover
                }
            } else if selected {
                p.accent
            } else {
                p.bg_tertiary
            })
        },
        move |p, _| {
            if selected {
                Color::BLACK
            } else {
                p.fg_secondary
            }
        },
        theme::RADIUS_SM,
    )
}

pub fn button_style_popup_item() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        |p, hot| Some(if hot { p.bg_hover } else { p.bg_secondary }),
        |p, _| p.fg,
        theme::RADIUS_SM,
    )
}

pub fn context_menu_item_style(active: bool) -> impl Fn(&AppTheme) -> container::Style + 'static {
    move |theme| {
        let p = &theme.palette;
        container::Style {
            background: if active {
                Some(p.bg_hover.into())
            } else {
                None
            },
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_panel_item(
    active: bool,
    text_color: Color,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        move |p, hot| {
            if hot {
                Some(if active { p.bg_current } else { p.bg_hover })
            } else if active {
                Some(p.bg_current.scale_alpha(0.7))
            } else {
                None
            }
        },
        move |_, _| text_color,
        theme::RADIUS_MD,
    )
}

pub fn button_style_hist() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        |p, hot| hot.then(|| p.fg.scale_alpha(0.15)),
        |p, hot| if hot { p.fg } else { p.fg_secondary },
        theme::RADIUS_SM,
    )
}

pub fn button_style_album() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    button_style(
        |_, _| None,
        |p, hot| if hot { p.fg } else { p.fg_secondary },
        0.0,
    )
}

pub fn scroll_padding() -> iced::Padding {
    iced::Padding {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: theme::SPACING_MD,
    }
}
