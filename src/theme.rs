use iced::Color;

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
    pub warning: Color,
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
            // #141418
            bg: Color::from_rgb8(0x14, 0x14, 0x18),
            // #1a1a20
            bg_secondary: Color::from_rgb8(0x1a, 0x1a, 0x20),
            // #08080e
            bg_tertiary: Color::from_rgb8(0x08, 0x08, 0x0e),
            // #2a2a34
            bg_hover: Color::from_rgb8(0x2a, 0x2a, 0x34),
            // #0a3a20
            bg_current: Color::from_rgb8(0x0a, 0x3a, 0x20),
            bg_selected: Color::from_rgb8(0x0a, 0x3a, 0x20),
            // #14c884
            accent: Color::from_rgb8(0x14, 0xc8, 0x84),
            // #10ba70
            accent_hover: Color::from_rgb8(0x10, 0xba, 0x70),
            // #2a2a34
            button: Color::from_rgb8(0x2a, 0x2a, 0x34),
            // #3a3a44
            button_hover: Color::from_rgb8(0x3a, 0x3a, 0x44),
            // #e0e0e0
            fg: Color::from_rgb8(0xe0, 0xe0, 0xe0),
            // #8888a0
            fg_secondary: Color::from_rgb8(0x88, 0x88, 0xa0),
            // #555560
            fg_muted: Color::from_rgb8(0x55, 0x55, 0x60),
            // #000000b3
            overlay: Color::from_rgba8(0, 0, 0, 0.7),
            // #f0a030
            warning: Color::from_rgb8(0xf0, 0xa0, 0x30),
            // #c04040
            danger: Color::from_rgb8(0xc0, 0x40, 0x40),
            // #c03030
            danger_hover: Color::from_rgb8(0xc0, 0x30, 0x30),
        }
    }
}

pub const SIDEBAR_WIDTH: f32 = 300.0;
pub const SEARCH_PAGE_SIZE: usize = 10;
pub const SIDEBAR_ITEM_HEIGHT: f32 = 44.0;
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
pub const SPACING_SM: f32 = 8.0;
pub const SPACING_MD: f32 = 12.0;
pub const SPACING_LG: f32 = 16.0;
pub const SPACING_XL: f32 = 20.0;

pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 12.0;
pub const RADIUS_LG: f32 = 16.0;
pub const SEARCH_BTN_SIZE: f32 = 35.0;
pub const SEARCH_BAR_HEIGHT: f32 = 66.0;
pub const NOTIFICATION_HEIGHT: f32 = 28.0;
pub const SEARCH_HISTORY_ITEM_HEIGHT: f32 = 32.0;
pub const SEARCH_DROPDOWN_MAX_HEIGHT: f32 = 240.0;

pub const ICON_SIZE_SM: f32 = 14.0;
pub const ICON_SIZE_MD: f32 = 16.0;
pub const ICON_SIZE_LG: f32 = 18.0;

pub const TEXT_SIZE_XS: u32 = 12;
pub const TEXT_SIZE_SM: u32 = 13;
pub const TEXT_SIZE_DEFAULT: u32 = 15;
pub const TEXT_SIZE_MD: u32 = 15;
pub const TEXT_SIZE_LG: u32 = 18;

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
