use rodio::Source;
use std::{
    io::{BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tracing::{debug, warn};

pub struct AudioPlayer {
    cmd_tx: Sender<PlayerCommand>,
    state: Arc<Mutex<PlayerState>>,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub is_playing: bool,
    pub duration: f32,
    pub progress: f32,
    pub volume: f32,
    pub stream_finished: bool,
    /// Set once the stream pipeline (yt-dlp + ffmpeg) has finished writing
    /// the cache file to disk. Distinct from `stream_finished`, which is only
    /// true after the track has *played* to the end — `cache_ready` fires as
    /// soon as the download completes, so the cache can be registered
    /// independently of whether the user listened to the whole track.
    pub cache_ready: bool,
}

enum PlayerCommand {
    StreamAndCache {
        url: String,
        duration: f32,
        cache_path: PathBuf,
    },
    PlayCached {
        cache_path: PathBuf,
        duration: f32,
    },
    Pause,
    Resume,
    SetVolume(f32),
    Seek(Duration),
}

impl AudioPlayer {
    #[allow(unused_assignments)]
    pub fn new(initial_volume: f32) -> Self {
        let state = Arc::new(Mutex::new(PlayerState {
            is_playing: false,
            duration: 0.0,
            progress: 0.0,
            volume: initial_volume,
            stream_finished: false,
            cache_ready: false,
        }));

        let (cmd_tx, cmd_rx) = mpsc::channel::<PlayerCommand>();
        let state_clone = state.clone();

        thread::spawn(move || {
            let mut output: Option<(rodio::OutputStream, rodio::Sink)> = None;
            let mut ffmpeg: Option<std::process::Child> = None;
            let mut ytdlp: Option<std::process::Child> = None;
            // The WAV file rodio currently reads. For `StreamAndCache` this is
            // the persistent cache file (written by ffmpeg); for `PlayCached`
            // it is the already-complete cache file. It is intentionally
            // *never* deleted here — the `StreamCache` owns its lifecycle
            // (LRU eviction) — which also avoids a use-after-unlink race
            // while rodio is still reading.
            let mut playback_file: Option<PathBuf> = None;
            let mut stream_url: Option<String> = None;
            let mut expected_duration: f32 = 0.0;
            let mut stream_active: bool = false;

            /// Kill ffmpeg/yt-dlp, stop playback, and reset pipeline state.
            /// Deletes the previous temp WAV (from a `PlayCached` replay) but
            /// deliberately leaves cache files alone — those are owned by
            /// `StreamCache` and must persist for future replays.
            macro_rules! reset_pipeline {
                () => {
                    if let Some(mut p) = ffmpeg.take() {
                        let _ = p.kill();
                        let _ = p.wait();
                    }
                    if let Some(mut p) = ytdlp.take() {
                        let _ = p.kill();
                        let _ = p.wait();
                    }
                    // Only remove the playback file if it lives in the temp dir
                    // (the transient WAV produced for `PlayCached`); the cache
                    // file under the project cache dir is never touched here.
                    if let Some(ref path) = playback_file {
                        if path.starts_with(std::env::temp_dir().join("music_plr")) {
                            let _ = std::fs::remove_file(path);
                        }
                    }
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
                    }
                };
            }

            /// Try to start rodio playback from `playback_file` once enough
            /// data has been written (streaming) or immediately (cached).
            macro_rules! try_start_playback {
                () => {
                    if output.is_none() && stream_active {
                        if let Some(ref path) = playback_file {
                            let ready = std::fs::metadata(path).is_ok_and(|m| m.len() > 2048);
                            let exited = ffmpeg
                                .as_mut()
                                .and_then(|p| p.try_wait().ok().flatten())
                                .is_some();

                            if ready || exited {
                                debug!("WAV ready, starting playback");
                                if let Ok((stream, handle)) = rodio::OutputStream::try_default() {
                                    if let Ok(sink) = rodio::Sink::try_new(&handle) {
                                        if let Ok(file) = std::fs::File::open(path) {
                                            if let Ok(source) =
                                                rodio::Decoder::new(BufReader::new(file))
                                            {
                                                let vol =
                                                    state_clone.lock().map_or(1.0, |st| st.volume);
                                                let actual_duration = if expected_duration > 0.0 {
                                                    expected_duration
                                                } else {
                                                    source
                                                        .total_duration()
                                                        .map_or(0.0, |d| d.as_secs_f32())
                                                };
                                                sink.set_volume(vol);
                                                sink.append(source);
                                                sink.play();
                                                if let Ok(mut st) = state_clone.lock() {
                                                    st.is_playing = true;
                                                    st.duration = actual_duration;
                                                    st.progress = 0.0;
                                                    st.stream_finished = false;
                                                }
                                                output = Some((stream, sink));
                                            }
                                        }
                                    }
                                }
                            }
                        }
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

                            debug!(
                                "Streaming through ffmpeg to cache WAV: {}",
                                cache_path.display()
                            );

                            let cache_str = cache_path.to_string_lossy().to_string();
                            let mut ytdlp_child = match Command::new("yt-dlp")
                                .args([
                                    "-f",
                                    "bestaudio",
                                    "-o",
                                    "-",
                                    "--no-warnings",
                                    "--no-check-formats",
                                    "--extractor-args",
                                    "youtube:skip=webpage,dash,msn,player_client=android",
                                    &url,
                                ])
                                .stdout(Stdio::piped())
                                .stderr(Stdio::null())
                                .spawn()
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    warn!("Failed to spawn yt-dlp: {}", e);
                                    continue;
                                }
                            };

                            let Some(ytdlp_stdout) = ytdlp_child.stdout.take() else {
                                warn!("yt-dlp stdout not available");
                                continue;
                            };

                            let mut ffmpeg_child = match Command::new("ffmpeg")
                                .args([
                                    "-i",
                                    "pipe:0",
                                    "-f",
                                    "wav",
                                    "-bitexact",
                                    "-flush_packets",
                                    "1",
                                    "-y",
                                    &cache_str,
                                ])
                                .stdin(Stdio::piped())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn()
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    warn!("Failed to spawn ffmpeg: {}", e);
                                    let _ = ytdlp_child.kill();
                                    continue;
                                }
                            };

                            let Some(ffmpeg_stdin) = ffmpeg_child.stdin.take() else {
                                warn!("ffmpeg stdin not available");
                                let _ = ytdlp_child.kill();
                                let _ = ffmpeg_child.kill();
                                continue;
                            };

                            // Pipe yt-dlp's raw output straight into ffmpeg,
                            // which decodes it to the persistent cache WAV.
                            // No separate tee/cache copy: the cache file *is*
                            // the playback file, so there is exactly one copy
                            // on disk during streaming.
                            thread::spawn(move || {
                                let mut buf = [0u8; 8192];
                                let mut reader = ytdlp_stdout;
                                let mut writer = ffmpeg_stdin;
                                loop {
                                    match reader.read(&mut buf) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            if writer.write_all(&buf[..n]).is_err() {
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            warn!("yt-dlp read error: {}", e);
                                            break;
                                        }
                                    }
                                }
                                // Dropping writer closes ffmpeg's stdin, signaling EOF
                            });

                            ffmpeg = Some(ffmpeg_child);
                            ytdlp = Some(ytdlp_child);
                            playback_file = Some(cache_path);
                            stream_url = Some(url);
                            expected_duration = duration;
                            stream_active = true;
                        }

                        PlayerCommand::PlayCached {
                            cache_path,
                            duration,
                        } => {
                            reset_pipeline!();

                            debug!(
                                "Playing cached file (decoded directly via rodio/symphonia): {:?}",
                                cache_path
                            );

                            // The file is already on disk (a streamed cache WAV or a
                            // local import), so decode it directly with rodio's
                            // symphonia-backed decoders — no ffmpeg subprocess needed.
                            match std::fs::File::open(&cache_path) {
                                Ok(file) => match rodio::Decoder::new(BufReader::new(file)) {
                                    Ok(source) => {
                                        match rodio::OutputStream::try_default() {
                                            Ok((stream, handle)) => {
                                                match rodio::Sink::try_new(&handle) {
                                                    Ok(sink) => {
                                                        let vol = state_clone
                                                            .lock()
                                                            .map_or(1.0, |st| st.volume);
                                                        let actual_duration = if duration > 0.0 {
                                                            duration
                                                        } else {
                                                            source
                                                                .total_duration()
                                                                .map_or(0.0, |d| d.as_secs_f32())
                                                        };
                                                        sink.set_volume(vol);
                                                        sink.append(source);
                                                        sink.play();
                                                        if let Ok(mut st) = state_clone.lock() {
                                                            st.is_playing = true;
                                                            st.duration = actual_duration;
                                                            st.progress = 0.0;
                                                            st.stream_finished = false;
                                                            st.cache_ready = false;
                                                        }
                                                        output = Some((stream, sink));
                                                    }
                                                    Err(e) => {
                                                        warn!(
                                                            "Failed to create sink for cache: {}",
                                                            e
                                                        );
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                warn!(
                                                    "Failed to open output stream for cache: {}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to decode cached file {:?}: {}", cache_path, e);
                                    }
                                },
                                Err(e) => warn!("Failed to open cached file {:?}: {}", cache_path, e),
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

                // Detect stream completion (only meaningful while ffmpeg is the
                // decoder; `PlayCached` has no ffmpeg and finishes via the
                // sink-empty path above).
                if stream_active && ffmpeg.is_some() {
                    let ffmpeg_exit = ffmpeg.as_mut().and_then(|p| p.try_wait().ok().flatten());
                    let ytdlp_exit = ytdlp.as_mut().and_then(|p| p.try_wait().ok().flatten());

                    let done = if ytdlp.is_some() {
                        ffmpeg_exit.is_some() && ytdlp_exit.is_some()
                    } else {
                        ffmpeg_exit.is_some()
                    };

                    if done {
                        if ffmpeg_exit.is_some_and(|s| !s.success()) {
                            warn!("ffmpeg exited with error");
                        }
                        ffmpeg.take();
                        ytdlp.take();
                        stream_active = false;
                        // `playback_file` (the cache WAV) is intentionally kept
                        // so rodio can finish reading; `StreamCache` owns
                        // deletion.
                        if let Ok(mut st) = state_clone.lock() {
                            st.cache_ready = true;
                        }
                    }
                }

                try_start_playback!();
            }

            // No temp file to remove: the playback file is the cache, owned by
            // `StreamCache`. Leaving it on disk is correct.
        });

        Self { cmd_tx, state }
    }

    pub fn play_stream_cache(&self, url: &str, duration: f32, cache_path: PathBuf) {
        let _ = self.cmd_tx.send(PlayerCommand::StreamAndCache {
            url: url.to_string(),
            duration,
            cache_path,
        });
    }

    pub fn play_cached(&self, cache_path: PathBuf, duration: f32) {
        let _ = self.cmd_tx.send(PlayerCommand::PlayCached {
            cache_path,
            duration,
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
}
