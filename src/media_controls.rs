//! Cross-platform OS media controls via [souvlaki].
//!
//! souvlaki unifies the three platform mechanisms behind one API: MPRIS over
//! D-Bus on Linux, System Media Transport Controls on Windows, and the Now
//! Playing center on macOS. Inbound OS events ([`MediaControlEvent`]) are
//! forwarded verbatim over `media_event_tx`; outbound state is pushed through
//! [`MediaUpdate`] drained from `update_rx`.

use std::{ffi::c_void, sync::mpsc, thread, time::Duration};

pub use souvlaki::{MediaControlEvent, MediaPlayback};
use souvlaki::{MediaControls, MediaMetadata, PlatformConfig};
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct MediaUpdate {
    pub playback: MediaPlayback,
    pub title: String,
    pub artist: String,
    pub duration_secs: f32,
    pub has_track: bool,
}

/// Spawn the OS media-control server. `hwnd` is required on Windows (the handle
/// of the application window); it is ignored elsewhere.
pub fn start(
    event_tx: mpsc::Sender<MediaControlEvent>,
    update_rx: mpsc::Receiver<MediaUpdate>,
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

        let tx = event_tx.clone();
        if let Err(e) = controls.attach(move |event| {
            let _ = tx.send(event);
        }) {
            error!("Failed to attach media control handler: {}", e);
            return;
        }

        info!("Media controls started");

        loop {
            while let Ok(update) = update_rx.try_recv() {
                let MediaUpdate {
                    playback,
                    title,
                    artist,
                    duration_secs,
                    has_track,
                } = update;
                if let Err(e) = controls.set_playback(playback) {
                    warn!("Failed to set playback status: {}", e);
                }
                if has_track {
                    let metadata = MediaMetadata {
                        title: Some(&title),
                        artist: Some(&artist),
                        album: None,
                        cover_url: None,
                        duration: if duration_secs > 0.0 {
                            Some(Duration::from_secs_f32(duration_secs))
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
