use iced::{Background, Color, Length};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color,
    pub bg_secondary: Color,
    pub bg_elevated: Color,
    pub bg_hover: Color,
    pub bg_current: Color,
    pub bg_selected: Color,
    pub accent: Color,
    pub accent_fg: Color,
    pub fg: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,
    pub border: Color,
    pub overlay: Color,
    pub warning: Color,
    pub playbar_bg: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color::from_rgb8(0x14, 0x14, 0x18),
            bg_secondary: Color::from_rgb8(0x1a, 0x1a, 0x20),
            bg_elevated: Color::from_rgb8(0x1a, 0x1a, 0x22),
            bg_hover: Color::from_rgb8(0x2a, 0x2a, 0x34),
            bg_current: Color::from_rgb8(0x0a, 0x3a, 0x20),
            bg_selected: Color::from_rgb8(0x1a, 0x3a, 0x6a),
            accent: Color::from_rgb8(0x14, 0xc8, 0x84),
            accent_fg: Color::BLACK,
            fg: Color::from_rgb8(0xe0, 0xe0, 0xe0),
            fg_secondary: Color::from_rgb8(0x88, 0x88, 0xa0),
            fg_muted: Color::from_rgb8(0x55, 0x55, 0x60),
            border: Color::from_rgb8(0x33, 0x33, 0x40),
            overlay: Color::from_rgba8(0, 0, 0, 0.7),
            warning: Color::from_rgb8(0xf0, 0xa0, 0x30),
            playbar_bg: Color::from_rgb8(0x0f, 0x0f, 0x14),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color::from_rgb8(0xf5, 0xf5, 0xf5),
            bg_secondary: Color::from_rgb8(0xe8, 0xe8, 0xec),
            bg_elevated: Color::from_rgb8(0xff, 0xff, 0xff),
            bg_hover: Color::from_rgb8(0xe0, 0xe0, 0xe8),
            bg_current: Color::from_rgb8(0xd4, 0xf0, 0xe0),
            bg_selected: Color::from_rgb8(0xd0, 0xd8, 0xf0),
            accent: Color::from_rgb8(0x14, 0xc8, 0x84),
            accent_fg: Color::BLACK,
            fg: Color::from_rgb8(0x1a, 0x1a, 0x1a),
            fg_secondary: Color::from_rgb8(0x55, 0x55, 0x70),
            fg_muted: Color::from_rgb8(0x88, 0x88, 0x90),
            border: Color::from_rgb8(0xcc, 0xcc, 0xd0),
            overlay: Color::from_rgba8(0, 0, 0, 0.4),
            warning: Color::from_rgb8(0xc0, 0x70, 0x00),
            playbar_bg: Color::from_rgb8(0xd8, 0xd8, 0xdc),
        }
    }
}

pub const SIDEBAR_WIDTH: f32 = 240.0;
pub const SIDEBAR_ITEM_HEIGHT: f32 = 36.0;
pub const ROW_HEIGHT: f32 = 48.0;
pub const ROW_PADDING_H: f32 = 12.0;
pub const PLAYBAR_HEIGHT: f32 = 80.0;
pub const SEARCH_BAR_HEIGHT: f32 = 44.0;
pub const THUMBNAIL_SIZE: f32 = 36.0;
pub const PLAY_BUTTON_SIZE: f32 = 36.0;
pub const QUEUE_MIN_WIDTH: f32 = 240.0;
pub const MIN_TRACK_WIDTH: f32 = 400.0;
pub const DRAG_THRESHOLD: f32 = 5.0;
pub const DOUBLE_CLICK_MS: u128 = 300;

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 8.0;

pub const SPACING_XS: f32 = 4.0;
pub const SPACING_SM: f32 = 8.0;
pub const SPACING_MD: f32 = 12.0;
pub const SPACING_LG: f32 = 16.0;
pub const SPACING_XL: f32 = 20.0;

pub const ICON_SM: f32 = 14.0;
pub const ICON_MD: f32 = 16.0;
pub const ICON_LG: f32 = 18.0;

pub fn bg_color(_p: &Palette, c: Color) -> Background {
    Background::Color(c)
}

pub fn length(px: f32) -> Length {
    Length::Fixed(px)
}
