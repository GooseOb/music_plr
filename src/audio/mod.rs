use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tracing::{debug, warn};

mod growing;
mod normalization;
mod symphonia_source;

pub use normalization::compute_normalization_gain;

use growing::GrowingMediaSource;
use symphonia_source::SymphoniaStreamingSource;

pub struct AudioPlayer {
    cmd_tx: Sender<PlayerCommand>,
    state: Arc<Mutex<PlayerState>>,
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct PlayerState {
    pub is_playing: bool,
    pub duration: f32,
    pub progress: f32,
    pub volume: f32,
    pub stream_finished: bool,
    pub cache_ready: bool,
    pub has_output: bool,
}

enum PlayerCommand {
    StreamAndCache {
        url: String,
        duration: f32,
        cache_path: PathBuf,
        gain: f32,
    },
    StreamHttp {
        url: String,
        duration: f32,
        cache_path: PathBuf,
        gain: f32,
    },
    PlayCached {
        cache_path: PathBuf,
        duration: f32,
        gain: f32,
    },
    Pause,
    Resume,
    SetVolume(f32),
    Seek(Duration),
}

impl AudioPlayer {
    /// Spawn the dedicated output thread and return a handle to it.
    ///
    /// The thread body is one long-lived state machine: it owns the sink,
    /// yt-dlp child, and cache-file handles as locals, and each loop iteration
    /// drains a command then polls playback/download progress. Splitting it
    /// further would mean hoisting that state into a struct purely to satisfy
    /// a line count, so the length is deliberate.
    #[allow(unused_assignments, clippy::too_many_lines)]
    pub fn new(initial_volume: f32) -> Self {
        let state = Arc::new(Mutex::new(PlayerState {
            is_playing: false,
            duration: 0.0,
            progress: 0.0,
            volume: initial_volume,
            stream_finished: false,
            cache_ready: false,
            has_output: false,
        }));

        let (cmd_tx, cmd_rx) = mpsc::channel::<PlayerCommand>();
        let state_clone = state.clone();

        thread::spawn(move || {
            let mut output: Option<(rodio::OutputStream, rodio::Sink)> = None;
            let mut ytdlp: Option<std::process::Child> = None;
            // Set to `true` while the copy thread is still draining yt-dlp's
            // stdout into the cache file. The native decoder reads the (growing)
            // cache file and blocks at EOF until this flips to `false`, then
            // treats EOF as genuine end-of-track.
            let mut writer_alive: Option<Arc<AtomicBool>> = None;
            // The cache file rodio currently reads. For `StreamAndCache` this
            // is the persistent cache file (written directly from yt-dlp's
            // stdout by a copy thread, then decoded by symphonia); for
            // `PlayCached` it is the already-complete cache file. It is
            // intentionally *never* deleted here — the `StreamCache` owns its
            // lifecycle (LRU eviction) — which also avoids a use-after-unlink
            // race while rodio is still reading.
            let mut playback_file: Option<PathBuf> = None;
            let mut stream_url: Option<String> = None;
            let mut expected_duration: f32 = 0.0;
            let mut stream_active: bool = false;
            // The normalization gain for the currently-streaming track, set
            // when `StreamAndCache` arrives and read by the progressive-decode
            // block below (which runs outside that match arm's scope).
            let mut pending_gain: f32 = 1.0;

            /// Kill yt-dlp, stop playback, and reset pipeline state.
            /// Deliberately leaves cache files alone — those are owned by
            /// `StreamCache` and must persist for future replays.
            macro_rules! reset_pipeline {
                () => {
                    if let Some(mut p) = ytdlp.take() {
                        let _ = p.kill();
                        let _ = p.wait();
                    }
                    // yt-dlp's stdout is drained into the cache file by the copy
                    // thread; killing yt-dlp ends that thread. Drop the
                    // `writer_alive` flag so any in-flight reader stops blocking.
                    writer_alive.take();
                    stream_active = false;
                    stream_url = None;
                    playback_file = None;
                    if let Some((_, s)) = &output {
                        s.stop();
                    }
                    output = None;
                    if let Ok(mut st) = state_clone.lock() {
                        st.stream_finished = false;
                        st.cache_ready = false;
                        st.has_output = false;
                    }
                };
            }

            loop {
                match cmd_rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(cmd) => match cmd {
                        PlayerCommand::StreamAndCache {
                            url,
                            duration,
                            cache_path,
                            gain,
                        } => {
                            if let Some(ref current) = stream_url {
                                if current == &url {
                                    debug!("Ignoring duplicate StreamAndCache for same URL");
                                    continue;
                                }
                            }

                            reset_pipeline!();

                            if let Some(dir) = cache_path.parent() {
                                let _ = std::fs::create_dir_all(dir);
                            }

                            warn!(
                                "Streaming yt-dlp raw audio to cache file: {} (duration={})",
                                cache_path.display(),
                                duration
                            );

                            let Some((child, alive_flag)) =
                                spawn_stream_to_cache(&url, &cache_path)
                            else {
                                continue;
                            };

                            ytdlp = Some(child);
                            writer_alive = Some(alive_flag);
                            playback_file = Some(cache_path);
                            stream_url = Some(url);
                            expected_duration = duration;
                            stream_active = true;
                            pending_gain = gain;
                        }

                        PlayerCommand::StreamHttp {
                            url,
                            duration,
                            cache_path,
                            gain,
                        } => {
                            if let Some(ref current) = stream_url {
                                if current == &url {
                                    debug!("Ignoring duplicate StreamHttp for same URL");
                                    continue;
                                }
                            }

                            reset_pipeline!();

                            if let Some(dir) = cache_path.parent() {
                                let _ = std::fs::create_dir_all(dir);
                            }

                            debug!(
                                "Streaming HTTP audio to cache file: {} (duration={})",
                                cache_path.display(),
                                duration
                            );

                            let Some(alive_flag) = spawn_http_stream_to_cache(&url, &cache_path)
                            else {
                                continue;
                            };

                            ytdlp = None;
                            writer_alive = Some(alive_flag);
                            playback_file = Some(cache_path);
                            stream_url = Some(url);
                            expected_duration = duration;
                            stream_active = true;
                            pending_gain = gain;
                        }

                        PlayerCommand::PlayCached {
                            cache_path,
                            duration,
                            gain,
                        } => {
                            reset_pipeline!();

                            debug!(
                                "Playing cached file (decoded directly via symphonia): {:?}",
                                cache_path
                            );

                            match Self::start_source(
                                &cache_path,
                                None,
                                duration,
                                &state_clone,
                                gain,
                            ) {
                                Some(active) => output = Some(active),
                                None => {
                                    warn!("Failed to play cached file {:?}", cache_path);
                                }
                            }

                            // No temp file or streaming state for direct playback;
                            // the cache file is owned by StreamCache and left on disk.
                            playback_file = None;
                            expected_duration = duration;
                            stream_active = false;
                        }

                        PlayerCommand::Pause => {
                            if let Some((_, s)) = &output {
                                s.pause();
                                if let Ok(mut st) = state_clone.lock() {
                                    st.is_playing = false;
                                }
                            }
                        }
                        PlayerCommand::Resume => {
                            if let Some((_, s)) = &output {
                                s.play();
                                if let Ok(mut st) = state_clone.lock() {
                                    st.is_playing = true;
                                }
                            }
                        }
                        PlayerCommand::SetVolume(v) => {
                            if let Some((_, s)) = &output {
                                s.set_volume(v);
                            }
                            if let Ok(mut st) = state_clone.lock() {
                                st.volume = v;
                            }
                        }
                        PlayerCommand::Seek(pos) => {
                            if let Some((_, s)) = &output {
                                let _ = s.try_seek(pos);
                            }
                        }
                    },
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                if let Some((_, s)) = &output {
                    if let Ok(mut st) = state_clone.lock() {
                        st.is_playing = !s.empty() && !s.is_paused();
                        if st.duration > 0.0 && !s.empty() {
                            st.progress = (s.get_pos().as_secs_f32() / st.duration).min(1.0);
                        } else if s.empty() {
                            st.is_playing = false;
                            st.progress = 0.0;
                            st.stream_finished = true;
                        }
                    }
                }

                // Detect download completion: the cache file is whole once
                // yt-dlp has exited *and* the copy thread has drained its
                // stdout into the cache file (`writer_alive` flips to false).
                // This only flips `cache_ready` (for cache registration); it
                // does NOT gate playback — decoding starts as soon as enough
                // of the file has arrived (see below), so the track streams
                // progressively rather than after a full download. `PlayCached`
                // has no streaming state and finishes via the sink-empty path.
                if stream_active {
                    // `ytdlp` is `None` for the direct-HTTP path (StreamHttp),
                    // so treat a missing child process as already finished.
                    let child_done = ytdlp
                        .as_mut()
                        .is_none_or(|p| p.try_wait().ok().flatten().is_some());
                    let copy_done = writer_alive
                        .as_ref()
                        .is_none_or(|w| !w.load(Ordering::SeqCst));

                    if child_done && copy_done {
                        if let Some(exit) = ytdlp.as_mut().and_then(|p| p.try_wait().ok().flatten())
                        {
                            if !exit.success() {
                                warn!("yt-dlp exited with error");
                            }
                        }
                        ytdlp.take();
                        writer_alive.take();
                        // Download complete: register the cache and end the
                        // streaming state. Playback (already started above)
                        // continues independently of `stream_active`.
                        stream_active = false;
                        if let Ok(mut st) = state_clone.lock() {
                            st.cache_ready = true;
                        }
                    }
                }

                // Begin decoding once the container header has landed (so the
                // sequential probe succeeds) — but only while `output` is
                // still `None`, so we never restart the track mid-playback, and
                // only while `stream_active` (the completion block below relies
                // on it to flip `cache_ready` once the download finishes).
                // symphonia demuxes sequentially from the still-growing file and
                // never seeks during init, so playback starts within a few KB
                // and the reader blocks at EOF until the copy thread is done.
                if stream_active && output.is_none() {
                    if let Some(path) = playback_file.as_ref() {
                        let ready = std::fs::metadata(path).is_ok_and(|m| m.len() > 8192);
                        if ready {
                            output = Self::start_source(
                                path,
                                writer_alive.clone(),
                                expected_duration,
                                &state_clone,
                                pending_gain,
                            );
                        }
                    }
                }
            }

            // No temp file to remove: the playback file is the cache, owned by
            // `StreamCache`. Leaving it on disk is correct.
        });

        Self { cmd_tx, state }
    }

    /// Build and start a rodio `Sink` decoding `path` via symphonia.
    ///
    /// Uses `SymphoniaStreamingSource` rather than `rodio::Decoder::new`: the
    /// latter hardcodes `byte_len() == None` on its `MediaSource`, which makes
    /// symphonia's MKV/MP4 init seek and trip rodio's `unreachable!` panic.
    ///
    /// For a live stream `writer_alive` is `Some` — the reader blocks at EOF
    /// until the copy thread finishes; for a cached file it is `None` (real
    /// EOF, and seekable for replay). Returns `None` if the file can't be
    /// opened or the format can't be probed yet (retry on the streaming path).
    fn start_source(
        path: &PathBuf,
        writer_alive: Option<Arc<AtomicBool>>,
        duration: f32,
        state: &Arc<Mutex<PlayerState>>,
        gain: f32,
    ) -> Option<(rodio::OutputStream, rodio::Sink)> {
        let file = std::fs::File::open(path).ok()?;
        let source = SymphoniaStreamingSource::new(
            GrowingMediaSource { file, writer_alive },
            duration,
            gain,
        )
        .ok()?;
        let (stream, handle) = rodio::OutputStream::try_default().ok()?;
        let sink = rodio::Sink::try_new(&handle).ok()?;
        let vol = state.lock().map_or(1.0, |st| st.volume);
        sink.set_volume(vol);
        sink.append(source);
        sink.play();
        if let Ok(mut st) = state.lock() {
            st.is_playing = true;
            st.duration = duration;
            st.progress = 0.0;
            st.stream_finished = false;
            st.cache_ready = false;
            st.has_output = true;
        }
        Some((stream, sink))
    }

    pub fn play_stream_cache(&self, url: &str, duration: f32, cache_path: PathBuf, gain: f32) {
        let _ = self.cmd_tx.send(PlayerCommand::StreamAndCache {
            url: url.to_string(),
            duration,
            cache_path,
            gain,
        });
    }

    pub fn play_stream_http(&self, url: &str, duration: f32, cache_path: PathBuf, gain: f32) {
        let _ = self.cmd_tx.send(PlayerCommand::StreamHttp {
            url: url.to_string(),
            duration,
            cache_path,
            gain,
        });
    }

    pub fn play_cached(&self, cache_path: PathBuf, duration: f32, gain: f32) {
        let _ = self.cmd_tx.send(PlayerCommand::PlayCached {
            cache_path,
            duration,
            gain,
        });
    }

    pub fn pause(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Pause);
    }

    pub fn resume(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Resume);
    }

    pub fn set_volume(&self, vol: f32) {
        let _ = self.cmd_tx.send(PlayerCommand::SetVolume(vol));
    }

    pub fn seek(&self, pos: Duration) {
        let _ = self.cmd_tx.send(PlayerCommand::Seek(pos));
    }

    /// Clear the `stream_finished` flag in the shared state. Used by the tick
    /// loop to avoid busy-looping the auto-advance when there is no next track
    /// (e.g. a corrupt cached file that emptied without more queue items).
    pub fn clear_stream_finished(&self) {
        if let Ok(mut st) = self.state.lock() {
            st.stream_finished = false;
        }
    }

    pub fn get_state(&self) -> PlayerState {
        self.state
            .lock()
            .map_or_else(|e| e.into_inner().clone(), |st| st.clone())
    }

    pub fn has_output(&self) -> bool {
        self.state.lock().is_ok_and(|st| st.has_output)
    }
}

/// Spawn a direct HTTP stream of `url` into `cache_path` (used by non-yt-dlp
/// providers). The response body is
/// written straight to the cache file; symphonia decodes the growing file
/// during playback, so there is no transmux step.
fn spawn_http_stream_to_cache(url: &str, cache_path: &std::path::Path) -> Option<Arc<AtomicBool>> {
    let Ok(mut resp) = ureq::get(url).call() else {
        warn!("Failed to start HTTP stream: {url}");
        return None;
    };

    let alive_flag = Arc::new(AtomicBool::new(true));
    let path = cache_path.to_path_buf();
    let flag = alive_flag.clone();
    thread::spawn(move || {
        let mut file = match std::fs::File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to create cache file: {e}");
                flag.store(false, Ordering::SeqCst);
                return;
            }
        };
        let mut reader = resp.body_mut().as_reader();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if file.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("HTTP stream read error: {e}");
                    break;
                }
            }
        }
        debug!(
            "http stream copy thread done: {} ({} bytes)",
            path.display(),
            file.metadata().map_or(0, |m| m.len())
        );
        flag.store(false, Ordering::SeqCst);
    });

    Some(alive_flag)
}

/// Spawn `yt-dlp` streaming `url` to stdout plus a thread copying its
/// stdout into `cache_path`.
///
/// Returns the child process and a "writer alive" flag that the copy thread
/// clears once the download finishes, so a reader blocked at EOF on the still
/// growing cache file knows when EOF is genuine. Returns `None` if yt-dlp
/// could not be spawned or exposed no stdout.
fn spawn_stream_to_cache(
    url: &str,
    cache_path: &std::path::Path,
) -> Option<(std::process::Child, Arc<AtomicBool>)> {
    // Request AAC-in-M4A: symphonia can decode AAC (unlike Opus/WebM, which
    // neither rodio's `symphonia-all` nor the standalone `symphonia` 0.5 crate
    // can decode), and YouTube serves it as a fast-start DASH stream (moov at
    // the front) that demuxes sequentially — ideal for streaming.
    let mut child = match Command::new("yt-dlp")
        .args([
            "-f",
            "bestaudio[ext=m4a]/bestaudio",
            "-o",
            "-",
            "--no-warnings",
            "--no-check-formats",
            "--extractor-args",
            "youtube:player_client=web_embedded",
            url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to spawn yt-dlp: {}", e);
            return None;
        }
    };

    let Some(mut stdout) = child.stdout.take() else {
        warn!("yt-dlp stdout not available");
        return None;
    };

    // yt-dlp emits raw `bestaudio` bytes on stdout; write them straight to the
    // cache file. symphonia decodes that growing file directly during
    // playback, so there is no transmux step — exactly one copy on disk.
    let alive_flag = Arc::new(AtomicBool::new(true));
    let path = cache_path.to_path_buf();
    let flag = alive_flag.clone();
    thread::spawn(move || {
        let mut file = match std::fs::File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to create cache file: {}", e);
                flag.store(false, Ordering::SeqCst);
                return;
            }
        };
        let mut buf = [0u8; 8192];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if file.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("yt-dlp read error: {}", e);
                    break;
                }
            }
        }
        // Download finished: signal the reader that no more bytes are coming
        // so symphonia sees a genuine EOF.
        debug!(
            "stream copy thread done: {} ({} bytes)",
            path.display(),
            file.metadata().map_or(0, |m| m.len())
        );
        flag.store(false, Ordering::SeqCst);
    });

    Some((child, alive_flag))
}
