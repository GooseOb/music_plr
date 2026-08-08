//! `widget::*::Catalog` implementations for [`AppTheme`].
//!
//! Boilerplate required so every iced widget can be parameterized over
//! `AppTheme` instead of `iced::Theme`; separated from `mod.rs` because
//! it is never edited when tweaking the design.

use super::{AppTheme, RADIUS_MD, RADIUS_SM, SPACING_SM};
use iced::{widget, Color};

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
                border: iced::border::rounded(RADIUS_SM),
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
                    border: iced::border::rounded(SPACING_SM),
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
                border: iced::border::rounded(RADIUS_MD)
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
