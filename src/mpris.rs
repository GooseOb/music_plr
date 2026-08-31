//! Cross-platform OS media controls via [souvlaki].
//!
//! souvlaki unifies the three platform mechanisms behind one API: MPRIS over
//! D-Bus on Linux, System Media Transport Controls on Windows, and the Now
//! Playing center on macOS. Inbound OS events are mapped to [`MprisCommand`]
//! and forwarded over `mpris_cmd_tx`; outbound state is pushed through
//! [`MprisUpdate`] drained from `update_rx`.

use std::{borrow::Cow, ffi::c_void, sync::mpsc, thread, time::Duration};

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig, SeekDirection,
};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy)]
pub enum MprisCommand {
    TogglePlayPause,
    NextTrack,
    PreviousTrack,
    Stop,
    Play,
    Pause,
    SetVolume(f32),
    Seek(i64),
}

#[derive(Debug, Clone)]
pub struct MprisUpdate {
    pub playback_status: Cow<'static, str>,
    pub title: String,
    pub artist: String,
    pub duration_secs: f32,
    pub has_track: bool,
}

/// Spawn the OS media-control server. `hwnd` is required on Windows (the handle
/// of the application window); it is ignored elsewhere.
pub fn start(
    cmd_tx: mpsc::Sender<MprisCommand>,
    update_rx: mpsc::Receiver<MprisUpdate>,
    hwnd: Option<*mut c_void>,
) {
    // Raw pointers aren't `Send`; carry the handle as an integer across the
    // thread boundary and rebuild it inside the worker thread.
    let hwnd_raw = hwnd.map(|p| p as usize);
    thread::spawn(move || {
        let hwnd = hwnd_raw.map(|p| p as *mut c_void);
        let config = PlatformConfig {
            dbus_name: "goosemusic",
            display_name: "GooseOb's Music Player",
            hwnd,
        };
        let mut controls = match MediaControls::new(config) {
            Ok(controls) => controls,
            Err(e) => {
                error!("Failed to create media controls: {}", e);
                return;
            }
        };

        if let Err(e) = controls.attach(move |event| {
            let cmd = match event {
                MediaControlEvent::Play => MprisCommand::Play,
                MediaControlEvent::Pause => MprisCommand::Pause,
                MediaControlEvent::Toggle => MprisCommand::TogglePlayPause,
                MediaControlEvent::Next => MprisCommand::NextTrack,
                MediaControlEvent::Previous => MprisCommand::PreviousTrack,
                MediaControlEvent::Stop => MprisCommand::Stop,
                MediaControlEvent::SetVolume(v) => MprisCommand::SetVolume(v as f32),
                MediaControlEvent::SeekBy(SeekDirection::Forward, d) => {
                    MprisCommand::Seek(d.as_micros() as i64)
                }
                MediaControlEvent::SeekBy(SeekDirection::Backward, d) => {
                    MprisCommand::Seek(-(d.as_micros() as i64))
                }
                // Events the player doesn't act on (position/URI/raise/quit).
                MediaControlEvent::Seek(_)
                | MediaControlEvent::SetPosition(_)
                | MediaControlEvent::OpenUri(_)
                | MediaControlEvent::Raise
                | MediaControlEvent::Quit => return,
            };
            let _ = cmd_tx.send(cmd);
        }) {
            error!("Failed to attach media control handler: {}", e);
            return;
        }

        info!("Media controls started");

        loop {
            while let Ok(update) = update_rx.try_recv() {
                let playback = match update.playback_status.as_ref() {
                    "Playing" => MediaPlayback::Playing { progress: None },
                    "Paused" => MediaPlayback::Paused { progress: None },
                    _ => MediaPlayback::Stopped,
                };
                if let Err(e) = controls.set_playback(playback) {
                    warn!("Failed to set playback status: {}", e);
                }
                if update.has_track {
                    let metadata = MediaMetadata {
                        title: Some(&update.title),
                        artist: Some(&update.artist),
                        album: None,
                        cover_url: None,
                        duration: if update.duration_secs > 0.0 {
                            Some(Duration::from_secs_f32(update.duration_secs))
                        } else {
                            None
                        },
                    };
                    if let Err(e) = controls.set_metadata(metadata) {
                        warn!("Failed to set metadata: {}", e);
                    }
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
}
