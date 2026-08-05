#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::if_not_else,
    clippy::module_name_repetitions,
    clippy::option_if_let_else,
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
    .title("music_plr")
    .run()
    .unwrap();
}
