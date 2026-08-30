#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

/// Display name of the application (window title, settings header, MPRIS
/// identity). The short lowercase identifier used for internal identifiers
/// (package name, D-Bus service name, config/data/cache directories, iced
/// theme name) is `"goosemusic"`.
pub const APP_NAME: &str = "GooseOb's Music Player";

mod app;
mod audio;
mod data;
mod deps;
mod i18n;
mod icons;
mod load_state;
mod lyrics;
mod mpris;
mod providers;
mod theme;
mod types;
mod util;

use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    tracing::info!("Starting {}", APP_NAME);

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
    .title(APP_NAME)
    .run()
    .unwrap();
}
