//! Application theme: the color [`Palette`], the [`AppTheme`] wrapper iced
//! widgets are parameterized over, and the layout tokens re-exported from
//! [`layout`].

use iced::{theme, Color, Theme};

mod catalog;
pub mod layout;

pub use layout::*;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub bg_hover: Color,
    pub bg_current: Color,
    pub bg_selected: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub button: Color,
    pub button_hover: Color,
    pub fg: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,
    pub overlay: Color,
    pub danger: Color,
    pub danger_hover: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

impl Palette {
    pub const fn dark() -> Self {
        Self {
            bg: Color::from_rgb8(0x14, 0x14, 0x18),
            bg_secondary: Color::from_rgb8(0x1a, 0x1a, 0x20),
            bg_tertiary: Color::from_rgb8(0x08, 0x08, 0x0e),
            bg_hover: Color::from_rgb8(0x2a, 0x2a, 0x34),
            bg_current: Color::from_rgb8(0x0a, 0x3a, 0x20),
            bg_selected: Color::from_rgb8(0x0a, 0x3a, 0x20),
            accent: Color::from_rgb8(0x14, 0xc8, 0x84),
            accent_hover: Color::from_rgb8(0x10, 0xba, 0x70),
            button: Color::from_rgb8(0x2a, 0x2a, 0x34),
            button_hover: Color::from_rgb8(0x3a, 0x3a, 0x44),
            fg: Color::from_rgb8(0xe0, 0xe0, 0xe0),
            fg_secondary: Color::from_rgb8(0x88, 0x88, 0xa0),
            fg_muted: Color::from_rgb8(0x55, 0x55, 0x60),
            overlay: Color::from_rgba8(0, 0, 0, 0.7),
            danger: Color::from_rgb8(0xc0, 0x40, 0x40),
            danger_hover: Color::from_rgb8(0xc0, 0x30, 0x30),
        }
    }
}
/// A custom iced theme that wraps the built-in [`iced::Theme`] for
/// widget-level styling (so all widget `Catalog` traits are satisfied)
/// while also exposing our rich [`Palette`] for custom style closures.
///
/// By using `AppTheme` as the `Theme` type parameter of iced widgets,
/// style closures receive `&AppTheme` and can read palette colors directly
/// via `theme.palette` — eliminating the need to thread `&Palette` through
/// every view function (no props drilling).
#[derive(Debug, Clone)]
pub struct AppTheme {
    pub inner: Theme,
    pub palette: Palette,
}

impl AppTheme {
    pub fn new(palette: Palette) -> Self {
        Self {
            inner: Theme::custom_with_fn(
                "music_plr",
                iced::theme::Palette {
                    background: palette.bg,
                    text: palette.fg,
                    primary: palette.accent,
                    success: palette.accent,
                    warning: palette.fg_secondary,
                    danger: palette.danger,
                },
                |_| {
                    iced::theme::palette::Extended::generate(iced::theme::Palette {
                        background: palette.bg,
                        text: palette.fg,
                        primary: palette.accent,
                        success: palette.accent,
                        warning: palette.fg_secondary,
                        danger: palette.danger,
                    })
                },
            ),
            palette,
        }
    }
}

impl From<Palette> for AppTheme {
    fn from(palette: Palette) -> Self {
        Self::new(palette)
    }
}

// Delegate theme::Base to the inner iced::Theme.
impl theme::Base for AppTheme {
    fn default(_preference: theme::Mode) -> Self {
        Self::new(Palette::default())
    }

    fn mode(&self) -> theme::Mode {
        self.inner.mode()
    }

    fn base(&self) -> theme::Style {
        theme::Style {
            background_color: self.palette.bg,
            text_color: self.palette.fg,
        }
    }

    fn palette(&self) -> Option<iced::theme::Palette> {
        Some(self.palette.into())
    }

    fn name(&self) -> &'static str {
        "music_plr"
    }
}

/// Maps our [`Palette`] fields onto iced's 6-color [`theme::Palette`].
impl From<Palette> for iced::theme::Palette {
    fn from(p: Palette) -> Self {
        Self {
            background: p.bg,
            text: p.fg,
            primary: p.accent,
            success: p.accent,
            warning: p.fg_secondary,
            danger: p.danger,
        }
    }
}
