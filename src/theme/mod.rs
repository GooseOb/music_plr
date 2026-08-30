//! Application theme: the color [`Palette`], the [`AppTheme`] wrapper iced
//! widgets are parameterized over, and the layout tokens re-exported from
//! [`layout`].

use iced::{theme, Color, Theme};
use serde::{Deserialize, Serialize};

mod catalog;
pub mod layout;

pub use layout::*;

/// Which [`Palette`] the [`AppTheme`] should use. Persisted in [`Config`]
/// so the user's choice survives restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeKind {
    #[default]
    Dark,
    Light,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 2] = [ThemeKind::Dark, ThemeKind::Light];

    pub fn label(self) -> &'static str {
        match self {
            ThemeKind::Dark => "Dark",
            ThemeKind::Light => "Light",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            ThemeKind::Dark => Palette::dark(),
            ThemeKind::Light => Palette::light(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub bg_hover: Color,
    pub bg_current: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub button: Color,
    pub button_hover: Color,
    pub fg: Color,
    pub fg_accent: Color,
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

fn blend_channels(c1: f32, c2: f32, ratio: f32) -> f32 {
    c1 * (1.0 - ratio) + c2 * ratio
}

pub fn blend_colors(c1: Color, c2: Color, ratio: f32) -> Color {
    Color::from_rgba(
        blend_channels(c1.r, c2.r, ratio),
        blend_channels(c1.g, c2.g, ratio),
        blend_channels(c1.b, c2.b, ratio),
        blend_channels(c1.a, c2.a, ratio),
    )
}

impl Palette {
    pub const fn dark() -> Self {
        Self {
            bg: Color::from_rgb8(0x14, 0x14, 0x18),
            bg_secondary: Color::from_rgb8(0x1a, 0x1a, 0x20),
            bg_tertiary: Color::from_rgb8(0x08, 0x08, 0x0e),
            bg_hover: Color::from_rgb8(0x2a, 0x2a, 0x34),
            bg_current: Color::from_rgb8(0x0a, 0x3a, 0x20),
            accent: Color::from_rgb8(0x14, 0xc8, 0x84),
            accent_hover: Color::from_rgb8(0x10, 0xba, 0x70),
            button: Color::from_rgb8(0x2a, 0x2a, 0x34),
            button_hover: Color::from_rgb8(0x3a, 0x3a, 0x44),
            fg: Color::from_rgb8(0xe0, 0xe0, 0xe0),
            fg_accent: Color::from_rgb8(0x14, 0xc8, 0x84),
            fg_secondary: Color::from_rgb8(0x88, 0x88, 0xa0),
            fg_muted: Color::from_rgb8(0x55, 0x55, 0x60),
            overlay: Color::from_rgba8(0, 0, 0, 0.7),
            danger: Color::from_rgb8(0xc0, 0x40, 0x40),
            danger_hover: Color::from_rgb8(0xc0, 0x30, 0x30),
        }
    }

    pub const fn light() -> Self {
        Self {
            bg: Color::from_rgb8(0xf6, 0xf6, 0xfa),
            bg_secondary: Color::from_rgb8(0xea, 0xea, 0xf0),
            bg_tertiary: Color::from_rgb8(0xdf, 0xdf, 0xe8),
            bg_hover: Color::from_rgb8(0xd5, 0xd5, 0xe0),
            bg_current: Color::from_rgb8(0xa0, 0xf0, 0xa0),
            accent: Color::from_rgb8(0x84, 0xe8, 0xa4),
            accent_hover: Color::from_rgb8(0x60, 0xe8, 0x80),
            button: Color::from_rgb8(0xc8, 0xc8, 0xd2),
            button_hover: Color::from_rgb8(0xb8, 0xb8, 0xc8),
            fg: Color::from_rgb8(0x18, 0x18, 0x20),
            fg_accent: Color::from_rgb8(0x04, 0x98, 0x34),
            fg_secondary: Color::from_rgb8(0x55, 0x55, 0x66),
            fg_muted: Color::from_rgb8(0x88, 0x88, 0x99),
            overlay: Color::from_rgba8(0, 0, 0, 0.5),
            danger: Color::from_rgb8(0xf0, 0x70, 0x70),
            danger_hover: Color::from_rgb8(0xe0, 0x60, 0x60),
        }
    }
}

impl From<ThemeKind> for Palette {
    fn from(kind: ThemeKind) -> Self {
        kind.palette()
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
                "goosemusic",
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
        "goosemusic"
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
