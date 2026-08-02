use iced::{widget::svg, Color, Length};

pub fn handle(name: &str) -> svg::Handle {
    let data = match name {
        "search.svg" => include_str!("../icons/search.svg"),
        "edit.svg" => include_str!("../icons/edit.svg"),
        "skip-forward.svg" => include_str!("../icons/skip-forward.svg"),
        "delete.svg" => include_str!("../icons/delete.svg"),
        "home.svg" => include_str!("../icons/home.svg"),
        "pause.svg" => include_str!("../icons/pause.svg"),
        "volume.svg" => include_str!("../icons/volume.svg"),
        "radio.svg" => include_str!("../icons/radio.svg"),
        "sync.svg" => include_str!("../icons/sync.svg"),
        "folder.svg" => include_str!("../icons/folder.svg"),
        "chevron-right.svg" => include_str!("../icons/chevron-right.svg"),
        "play.svg" => include_str!("../icons/play.svg"),
        "skip-back.svg" => include_str!("../icons/skip-back.svg"),
        "download.svg" => include_str!("../icons/download.svg"),
        "music.svg" => include_str!("../icons/music.svg"),
        "back.svg" => include_str!("../icons/back.svg"),
        "forward.svg" => include_str!("../icons/forward.svg"),
        "queue.svg" => include_str!("../icons/queue.svg"),
        _ => panic!("Unknown icon: {}", name),
    };
    svg::Handle::from_memory(data.as_bytes().to_vec())
}

pub fn icon(name: &str, color: Color, size: f32) -> svg::Svg<'static> {
    svg::Svg::new(handle(name))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}
