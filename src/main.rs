mod app;
mod audio;
mod cache;
mod config;
mod downloads;
mod icons;
mod mpris;
mod playlists;
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
    .theme(iced::Theme::Dark)
    .title("music_plr")
    .run()
    .unwrap();
}
