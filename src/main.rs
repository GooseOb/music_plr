#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

/// Display name of the application (window title, settings header, media-control
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
mod media_controls;
mod providers;
mod theme;
mod types;
mod util;

use tracing_subscriber::{filter::EnvFilter, prelude::*};

fn main() {
    let log_path = crate::data::data_path("goosemusic.log");
    if let Some(dir) = log_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let log_file = std::fs::File::create(&log_path)
        .unwrap_or_else(|e| panic!("Failed to create log file {}: {e}", log_path.display()));

    // In release builds, redirect stderr to the log file so panic messages,
    // C-level errors, and child process output are captured alongside tracing.
    #[cfg(all(not(debug_assertions), unix))]
    {
        use std::os::unix::io::AsRawFd;
        let fd = log_file.as_raw_fd();
        unsafe {
            libc::dup2(fd, libc::STDERR_FILENO);
        }
    }

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(log_file.try_clone().expect("Failed to clone log file handle"))
        .with_ansi(false)
        .with_filter(EnvFilter::new("warn"));
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_filter(EnvFilter::from_default_env());
    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();
    tracing::info!("Starting {} (log: {:?})", APP_NAME, log_path);

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
