use iced::Color;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub bg_secondary: Color,
    pub bg_hover: Color,
    pub bg_current: Color,
    pub bg_selected: Color,
    pub accent: Color,
    pub fg: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,
    pub overlay: Color,
    pub warning: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            bg: Color::from_rgb8(0x14, 0x14, 0x18),
            bg_secondary: Color::from_rgb8(0x1a, 0x1a, 0x20),
            bg_hover: Color::from_rgb8(0x2a, 0x2a, 0x34),
            bg_current: Color::from_rgb8(0x0a, 0x3a, 0x20),
            bg_selected: Color::from_rgb8(0x1a, 0x3a, 0x6a),
            accent: Color::from_rgb8(0x14, 0xc8, 0x84),
            fg: Color::from_rgb8(0xe0, 0xe0, 0xe0),
            fg_secondary: Color::from_rgb8(0x88, 0x88, 0xa0),
            fg_muted: Color::from_rgb8(0x55, 0x55, 0x60),
            overlay: Color::from_rgba8(0, 0, 0, 0.7),
            warning: Color::from_rgb8(0xf0, 0xa0, 0x30),
        }
    }
}

pub const SIDEBAR_WIDTH: f32 = 300.0;
pub const SIDEBAR_ITEM_HEIGHT: f32 = 44.0;
pub const ROW_HEIGHT: f32 = 48.0;
pub const THUMBNAIL_SIZE: f32 = 36.0;
pub const QUEUE_MIN_WIDTH: f32 = 240.0;
pub const MIN_TRACK_WIDTH: f32 = 400.0;
pub const DRAG_THRESHOLD: f32 = 5.0;
pub const PLAYBAR_THUMBNAIL_SIZE: f32 = 56.0;
pub const TRACK_LEADING_WIDTH: f32 = 30.0;
pub const DURATION_WIDTH: f32 = 54.0;

pub const SPACING_XS: f32 = 4.0;
pub const SPACING_SM: f32 = 8.0;
pub const SPACING_MD: f32 = 12.0;
pub const SPACING_LG: f32 = 16.0;
pub const SPACING_XL: f32 = 20.0;

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 8.0;
pub const SEARCH_BTN_SIZE: f32 = 30.0;

pub const ICON_SIZE_SM: f32 = 14.0;
pub const ICON_SIZE_MD: f32 = 16.0;
pub const ICON_SIZE_LG: f32 = 18.0;

pub const TEXT_SIZE_XS: u32 = 11;
pub const TEXT_SIZE_SM: u32 = 12;
pub const TEXT_SIZE_DEFAULT: u32 = 14;
pub const TEXT_SIZE_MD: u32 = 14;
pub const TEXT_SIZE_LG: u32 = 16;

pub const BUTTON_HEIGHT: f32 = 28.0;
pub const BUTTON_WIDTH: f32 = 80.0;
pub const QUEUE_BTN_WIDTH: f32 = 36.0;
pub const VOLUME_SLIDER_WIDTH: f32 = 80.0;
pub const PLAYBAR_TRACK_INFO_WIDTH: f32 = 164.0;
pub const TIME_TEXT_WIDTH: f32 = 48.0;
pub const CONTEXT_MENU_WIDTH: f32 = 190.0;
pub const DIALOG_WIDTH: f32 = 300.0;
pub const DIALOG_HEIGHT: f32 = 140.0;
pub const DELETE_BTN_SIZE: f32 = 24.0;
pub const QUEUE_WIDTH_RATIO: f32 = 0.2;
