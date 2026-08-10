use crate::theme::AppTheme;
use iced::{widget::svg, Color};

pub const ADD_ICON: &[u8] = include_bytes!("../icons/add.svg");
pub const BACK_ICON: &[u8] = include_bytes!("../icons/back.svg");
pub const CACHE_ICON: &[u8] = include_bytes!("../icons/cache.svg");
pub const DELETE_ICON: &[u8] = include_bytes!("../icons/delete.svg");
pub const DOWNLOAD_ICON: &[u8] = include_bytes!("../icons/download.svg");
pub const FOLDER_ICON: &[u8] = include_bytes!("../icons/folder.svg");
pub const FORWARD_ICON: &[u8] = include_bytes!("../icons/forward.svg");
pub const MUSIC_ICON: &[u8] = include_bytes!("../icons/music.svg");
pub const PAUSE_ICON: &[u8] = include_bytes!("../icons/pause.svg");
pub const PLAY_ICON: &[u8] = include_bytes!("../icons/play.svg");
pub const QUEUE_ICON: &[u8] = include_bytes!("../icons/queue.svg");
pub const RADIO_ICON: &[u8] = include_bytes!("../icons/radio.svg");
pub const REPEAT_ICON: &[u8] = include_bytes!("../icons/repeat.svg");
pub const SEARCH_ICON: &[u8] = include_bytes!("../icons/search.svg");
pub const SKIP_BACK_ICON: &[u8] = include_bytes!("../icons/skip-back.svg");
pub const SKIP_FORWARD_ICON: &[u8] = include_bytes!("../icons/skip-forward.svg");
pub const VOLUME_ICON: &[u8] = include_bytes!("../icons/volume.svg");

pub fn icon(icon_data: &'static [u8], color: Color, size: f32) -> svg::Svg<'static, AppTheme> {
    svg::Svg::new(svg::Handle::from_memory(icon_data))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
}
