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

pub const SIDEBAR_WIDTH: f32 = 240.0;
pub const SIDEBAR_ITEM_HEIGHT: f32 = 36.0;
pub const ROW_HEIGHT: f32 = 48.0;
pub const THUMBNAIL_SIZE: f32 = 36.0;
pub const QUEUE_MIN_WIDTH: f32 = 240.0;
pub const DRAG_THRESHOLD: f32 = 5.0;
