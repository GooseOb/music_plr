use crate::theme::{self, AppTheme};

use iced::{
    border,
    widget::{button, container, text},
    Color,
};

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
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => p.accent_hover,
            _ => p.accent,
        };
        button::Style {
            background: Some(bg_color.into()),
            text_color: Color::BLACK,
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_queue(
    enabled: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => {
                if enabled {
                    p.accent_hover
                } else {
                    p.button_hover
                }
            }
            _ => {
                if enabled {
                    p.accent
                } else {
                    p.button
                }
            }
        };
        button::Style {
            background: Some(bg_color.into()),
            text_color: if enabled { Color::BLACK } else { p.fg },
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_danger() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => p.danger_hover,
            _ => p.danger,
        };
        button::Style {
            background: Some(bg_color.into()),
            text_color: Color::WHITE,
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_nav(
    enabled: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => {
                if enabled {
                    p.button_hover
                } else {
                    p.bg
                }
            }
            _ => {
                if enabled {
                    p.button
                } else {
                    p.bg
                }
            }
        };
        let text_color = if enabled { p.fg } else { p.fg_muted };
        button::Style {
            background: Some(bg_color.into()),
            text_color,
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_list_item(
    active: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => p.bg_hover,
            _ => {
                if active {
                    p.bg_current
                } else {
                    p.bg_secondary
                }
            }
        };
        let text_color = if active { p.fg } else { p.fg_secondary };
        button::Style {
            background: Some(bg_color.into()),
            text_color,
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_scope(
    selected: bool,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => {
                if selected {
                    p.accent_hover
                } else {
                    p.bg_hover
                }
            }
            _ => {
                if selected {
                    p.accent
                } else {
                    p.bg_tertiary
                }
            }
        };
        let text_color = if selected {
            Color::BLACK
        } else {
            p.fg_secondary
        };
        button::Style {
            background: Some(bg.into()),
            text_color,
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_popup_item() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => p.bg_hover,
            _ => p.bg_secondary,
        };
        button::Style {
            background: Some(bg.into()),
            text_color: p.fg,
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_panel_item(
    active: bool,
    text_color: Color,
) -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    move |theme, status| {
        let p = &theme.palette;
        let background = match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some((if active { p.bg_current } else { p.bg_hover }).into())
            }
            _ => {
                if active {
                    Some(p.bg_current.scale_alpha(0.7).into())
                } else {
                    None
                }
            }
        };
        button::Style {
            background,
            text_color,
            border: border::rounded(theme::RADIUS_MD),
            ..Default::default()
        }
    }
}

pub fn button_style_hist() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    |theme, status| {
        let p = &theme.palette;
        let background = match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(p.fg.scale_alpha(0.15).into())
            }
            _ => None,
        };
        let text_color = match status {
            button::Status::Hovered | button::Status::Pressed => p.fg,
            _ => p.fg_secondary,
        };
        button::Style {
            background,
            text_color,
            border: border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

pub fn button_style_album() -> impl Fn(&AppTheme, button::Status) -> button::Style + 'static {
    |theme, status| {
        let p = &theme.palette;
        let fg = match status {
            button::Status::Hovered | button::Status::Pressed => p.fg,
            _ => p.fg_secondary,
        };
        button::Style {
            background: None,
            text_color: fg,
            ..Default::default()
        }
    }
}

pub fn scroll_padding() -> iced::Padding {
    iced::Padding {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: theme::SPACING_MD,
    }
}
