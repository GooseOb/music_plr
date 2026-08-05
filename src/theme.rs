use iced::{theme, widget, Color, Theme};

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

pub const SIDEBAR_WIDTH: f32 = 300.0;
pub const SEARCH_PAGE_SIZE: usize = 10;
pub const SIDEBAR_ITEM_HEIGHT: f32 = SPACING_XS + ICON_SIZE_MD + 2.0 * SPACING_SM;
/// Y-offset from the top of the sidebar container to the playlist list
/// scrollable area. Used as a fallback when `sidebar_bounds` hasn't been
/// populated yet (iced's `on_scroll` doesn't fire when content fits).
pub const SIDEBAR_PLAYLIST_LIST_OFFSET_Y: f32 = SPACING_SM // sidebar padding
    + SPACING_XS
    + BUTTON_HEIGHT + 2.0 * SPACING_MD // nav_buttons
    + SPACING_XS
    + 1.0 // rule
    + SPACING_XS
    + SIDEBAR_ITEM_HEIGHT // Search
    + SIDEBAR_ITEM_HEIGHT // Downaloads
    + 1.0 // rule
    + SPACING_XS;
pub const ROW_HEIGHT: f32 = 48.0;
pub const THUMBNAIL_SIZE: f32 = 36.0;
pub const QUEUE_MIN_WIDTH: f32 = 240.0;
pub const DRAG_THRESHOLD: f32 = 1.0;
pub const DRAG_AUTO_SCROLL_ZONE: f32 = 50.0;
pub const DRAG_AUTO_SCROLL_SPEED: f32 = 16.0;
pub const DROP_LINE_HEIGHT: f32 = 2.0;
pub const PLAYBAR_THUMBNAIL_SIZE: f32 = 56.0;
pub const TRACK_LEADING_WIDTH: f32 = 30.0;
pub const DURATION_WIDTH: f32 = 54.0;

pub const SPACING_XS: f32 = 4.0;
pub const SPACING_XS2: f32 = 6.0;
pub const SPACING_SM: f32 = 8.0;
pub const SPACING_MD: f32 = 12.0;
pub const SPACING_LG: f32 = 16.0;
pub const SPACING_XL: f32 = 20.0;

pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 12.0;
// pub const RADIUS_LG: f32 = 16.0;
pub const SEARCH_BTN_SIZE: f32 = 35.0;
pub const SEARCH_BAR_HEIGHT: f32 = 66.0;
pub const SEARCH_HISTORY_ITEM_HEIGHT: f32 = 32.0;
pub const SEARCH_DROPDOWN_MAX_HEIGHT: f32 = 240.0;

pub const ICON_SIZE_SM: f32 = 14.0;
pub const ICON_SIZE_MD: f32 = 16.0;
pub const ICON_SIZE_LG: f32 = 18.0;

pub const TEXT_SIZE_XS: u32 = 12;
pub const TEXT_SIZE_SM: u32 = 13;
pub const TEXT_SIZE_MD: u32 = 15;
pub const TEXT_SIZE_LG: u32 = 18;

pub const BUTTON_HEIGHT: f32 = 28.0;
pub const QUEUE_BTN_WIDTH: f32 = 36.0;
pub const VOLUME_SLIDER_WIDTH: f32 = 80.0;
pub const PLAYBAR_TRACK_INFO_WIDTH: f32 = 164.0;
pub const TIME_TEXT_WIDTH: f32 = 48.0;
pub const CONTEXT_MENU_WIDTH: f32 = 190.0;
pub const DIALOG_WIDTH: f32 = 300.0;
pub const DELETE_BTN_SIZE: f32 = 24.0;
pub const QUEUE_WIDTH_RATIO: f32 = 0.2;

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

// Implement widget Catalog traits for AppTheme.

impl widget::container::Catalog for AppTheme {
    type Class<'a> = widget::container::StyleFn<'a, AppTheme>;

    fn default<'a>() -> Self::Class<'a> {
        let inner = <iced::Theme as widget::container::Catalog>::default();
        Box::new(move |theme: &AppTheme| widget::container::Catalog::style(&theme.inner, &inner))
    }

    fn style(&self, class: &Self::Class<'_>) -> widget::container::Style {
        class(self)
    }
}

impl widget::rule::Catalog for AppTheme {
    type Class<'a> = widget::rule::StyleFn<'a, AppTheme>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(move |theme| widget::rule::Style {
            color: theme.palette.fg_muted,
            radius: iced::border::Radius::new(0),
            fill_mode: widget::rule::FillMode::Full,
            snap: true,
        })
    }

    fn style(&self, class: &Self::Class<'_>) -> widget::rule::Style {
        class(self)
    }
}

impl widget::text::Catalog for AppTheme {
    type Class<'a> = widget::text::StyleFn<'a, AppTheme>;

    fn default<'a>() -> Self::Class<'a> {
        let inner = <iced::Theme as widget::text::Catalog>::default();
        Box::new(move |theme: &AppTheme| widget::text::Catalog::style(&theme.inner, &inner))
    }

    fn style(&self, class: &Self::Class<'_>) -> widget::text::Style {
        class(self)
    }
}

impl widget::svg::Catalog for AppTheme {
    type Class<'a> = widget::svg::StyleFn<'a, AppTheme>;

    fn default<'a>() -> Self::Class<'a> {
        let inner = <iced::Theme as widget::svg::Catalog>::default();
        Box::new(move |theme: &AppTheme, status| {
            widget::svg::Catalog::style(&theme.inner, &inner, status)
        })
    }

    fn style(&self, class: &Self::Class<'_>, status: widget::svg::Status) -> widget::svg::Style {
        class(self, status)
    }
}

impl widget::button::Catalog for AppTheme {
    type Class<'a> = widget::button::StyleFn<'a, AppTheme>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(|theme, status| {
            let p = &theme.palette;
            let bg_color = match status {
                widget::button::Status::Hovered | widget::button::Status::Pressed => p.button_hover,
                _ => p.button,
            };
            widget::button::Style {
                background: Some(bg_color.into()),
                text_color: p.fg,
                border: iced::border::rounded(crate::theme::RADIUS_SM),
                ..Default::default()
            }
        })
    }

    fn style(
        &self,
        class: &Self::Class<'_>,
        status: widget::button::Status,
    ) -> widget::button::Style {
        class(self, status)
    }
}

impl widget::scrollable::Catalog for AppTheme {
    type Class<'a> = widget::scrollable::StyleFn<'a, AppTheme>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(|theme, status| {
            let p = &theme.palette;
            let rail = widget::scrollable::Rail {
                background: Some(p.bg_tertiary.scale_alpha(0.6).into()),
                border: iced::border::rounded(0),
                scroller: widget::scrollable::Scroller {
                    background: match status {
                        widget::scrollable::Status::Active { .. } => p.fg_muted.scale_alpha(0.6),
                        widget::scrollable::Status::Hovered { .. } => p.fg_muted.scale_alpha(0.8),
                        widget::scrollable::Status::Dragged { .. } => p.fg_muted,
                    }
                    .into(),
                    border: iced::border::rounded(crate::theme::SPACING_SM),
                },
            };

            widget::scrollable::Style {
                container: widget::container::Style::default(),
                vertical_rail: rail,
                horizontal_rail: rail,
                gap: None,
                auto_scroll: widget::scrollable::AutoScroll {
                    background: p.overlay.into(),
                    border: iced::border::rounded(u32::MAX),
                    shadow: iced::Shadow {
                        color: Color::BLACK.scale_alpha(0.7),
                        offset: iced::Vector::ZERO,
                        blur_radius: 2.0,
                    },
                    icon: p.fg_muted,
                },
            }
        })
    }

    fn style(
        &self,
        class: &Self::Class<'_>,
        status: widget::scrollable::Status,
    ) -> widget::scrollable::Style {
        class(self, status)
    }
}

impl widget::slider::Catalog for AppTheme {
    type Class<'a> = widget::slider::StyleFn<'a, AppTheme>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(|theme, _status| {
            let p = &theme.palette;
            let color = p.accent;
            widget::slider::Style {
                rail: widget::slider::Rail {
                    backgrounds: (color.into(), p.bg_secondary.into()),
                    width: 4.0,
                    border: iced::border::rounded(2.0),
                },
                handle: widget::slider::Handle {
                    shape: widget::slider::HandleShape::Circle { radius: 7.0 },
                    background: color.into(),
                    border_color: Color::TRANSPARENT,
                    border_width: 0.0,
                },
            }
        })
    }

    fn style(
        &self,
        class: &Self::Class<'_>,
        status: widget::slider::Status,
    ) -> widget::slider::Style {
        class(self, status)
    }
}

impl widget::text_input::Catalog for AppTheme {
    type Class<'a> = widget::text_input::StyleFn<'a, AppTheme>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(|theme, status| {
            let p = &theme.palette;
            let border_color = match status {
                widget::text_input::Status::Hovered => p.fg_muted,
                widget::text_input::Status::Focused { .. } => p.accent,
                _ => Color::TRANSPARENT,
            };
            widget::text_input::Style {
                background: p.bg_tertiary.into(),
                border: iced::border::rounded(crate::theme::RADIUS_MD)
                    .color(border_color)
                    .width(1),
                icon: p.fg_muted,
                placeholder: p.fg_muted,
                value: p.fg,
                selection: p.bg_selected,
            }
        })
    }

    fn style(
        &self,
        class: &Self::Class<'_>,
        status: widget::text_input::Status,
    ) -> widget::text_input::Style {
        class(self, status)
    }
}
