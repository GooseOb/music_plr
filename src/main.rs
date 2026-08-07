#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]

mod app;
mod audio;
mod cache;
mod config;
mod downloads;
mod icons;
mod mpris;
mod playlists;
mod search_history;
mod session;
mod theme;
mod thumbnails;
mod types;
mod util;
mod youtube;

use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    tracing::info!("Starting music_plr");

    iced::application(
        app::MusicPlayer::new,
        app::MusicPlayer::update,
        app::MusicPlayer::view,
    )
    .subscription(app::MusicPlayer::subscription)
    .theme(|state: &app::MusicPlayer| state.app_theme.clone())
    .settings(iced::Settings {
        default_text_size: iced::Pixels(theme::TEXT_SIZE_MD as f32),
        ..Default::default()
    })
    .title("music_plr")
    .run()
    .unwrap();
}
