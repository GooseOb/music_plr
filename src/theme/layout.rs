//! Layout constants: spacing scale, sizes, and derived geometry.
//!
//! These are pure design tokens with no dependency on the palette or on
//! iced, so they live apart from the theme plumbing in `mod.rs`.

pub const SIDEBAR_WIDTH: f32 = 300.0;
pub const SEARCH_PAGE_SIZE: usize = 10;
pub const SIDEBAR_ITEM_HEIGHT: f32 = SPACING_XS + ICON_SIZE_MD + 2.0 * SPACING_SM;
pub const ROW_HEIGHT: f32 = 48.0;
pub const THUMBNAIL_SIZE: f32 = 36.0;
pub const QUEUE_MIN_WIDTH: f32 = 240.0;
pub const DRAG_THRESHOLD: f32 = 1.0;
pub const DRAG_AUTO_SCROLL_ZONE: f32 = 50.0;
pub const DRAG_AUTO_SCROLL_SPEED: f32 = 16.0;
pub const DROP_LINE_HEIGHT: f32 = 2.0;
pub const PLAYBAR_THUMBNAIL_SIZE: f32 = 56.0;
pub const TRACK_LEADING_WIDTH: f32 = 30.0;

pub const SPACING_XS: f32 = 4.0;
pub const SPACING_2XS: f32 = 6.0;
pub const SPACING_SM: f32 = 8.0;
pub const SPACING_MD: f32 = 12.0;
pub const SPACING_LG: f32 = 16.0;
pub const SPACING_XL: f32 = 20.0;
pub const SPACING_2XL: f32 = 32.0;

pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 12.0;
// pub const RADIUS_LG: f32 = 16.0;
pub const SEARCH_BTN_SIZE: f32 = 35.0;
pub const SEARCH_BAR_HEIGHT: f32 = 66.0;
pub const SEARCH_HISTORY_ITEM_HEIGHT: f32 = ICON_SIZE_SM + 2.0 * SPACING_XS + SPACING_SM / 2.0;
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
