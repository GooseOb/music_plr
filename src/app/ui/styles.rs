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

pub fn bg_transparent() -> impl Fn(&AppTheme) -> container::Style + 'static {
    |_| container::Style {
        background: None,
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

pub fn fg_secondary() -> impl Fn(&AppTheme) -> text::Style + 'static {
    |theme| text::Style {
        color: theme.palette.fg_secondary.into(),
        ..Default::default()
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
