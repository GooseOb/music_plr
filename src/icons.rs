use crate::theme::AppTheme;
use iced::{widget::svg, Color, Length};
use std::sync::OnceLock;

/// Compile-time icon registry. Returns the raw SVG bytes for a named icon.
/// Falls back to the "music.svg" icon if the name is unknown, so the app
/// never panics on a typo'd icon reference.
#[allow(clippy::match_same_arms)]
fn icon_data(name: &str) -> &'static str {
    match name {
        "search.svg" => include_str!("../icons/search.svg"),
        "skip-forward.svg" => include_str!("../icons/skip-forward.svg"),
        "delete.svg" => include_str!("../icons/delete.svg"),
        "pause.svg" => include_str!("../icons/pause.svg"),
        "volume.svg" => include_str!("../icons/volume.svg"),
        "radio.svg" => include_str!("../icons/radio.svg"),
        "sync.svg" => include_str!("../icons/sync.svg"),
        "folder.svg" => include_str!("../icons/folder.svg"),
        "play.svg" => include_str!("../icons/play.svg"),
        "skip-back.svg" => include_str!("../icons/skip-back.svg"),
        "download.svg" => include_str!("../icons/download.svg"),
        "music.svg" => include_str!("../icons/music.svg"),
        "back.svg" => include_str!("../icons/back.svg"),
        "forward.svg" => include_str!("../icons/forward.svg"),
        "queue.svg" => include_str!("../icons/queue.svg"),
        "add.svg" => include_str!("../icons/add.svg"),
        _ => include_str!("../icons/music.svg"),
    }
}

/// Cached SVG handles so we don't re-allocate the byte Vec on every render.
static ICON_CACHE: OnceLock<Vec<(&'static str, svg::Handle)>> = OnceLock::new();

fn cached_handle(name: &str) -> &'static svg::Handle {
    let cache = ICON_CACHE.get_or_init(|| {
        let names = [
            "search.svg",
            "skip-forward.svg",
            "delete.svg",
            "pause.svg",
            "volume.svg",
            "radio.svg",
            "sync.svg",
            "folder.svg",
            "play.svg",
            "skip-back.svg",
            "download.svg",
            "music.svg",
            "back.svg",
            "forward.svg",
            "queue.svg",
            "add.svg",
        ];
        names
            .iter()
            .map(|&n| {
                (
                    n,
                    svg::Handle::from_memory(icon_data(n).as_bytes().to_vec()),
                )
            })
            .collect()
    });
    cache
        .iter()
        .find(|(n, _)| *n == name)
        .or_else(|| cache.iter().find(|(n, _)| *n == "music.svg"))
        .map(|(_, h)| h)
        .expect("music.svg missing from icon cache")
}

#[allow(dead_code)]
pub fn handle(name: &str) -> svg::Handle {
    cached_handle(name).clone()
}

pub fn icon(name: &str, color: Color, size: f32) -> svg::Svg<'static, AppTheme> {
    svg::Svg::new(cached_handle(name).clone())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}
