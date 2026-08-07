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

                            debug!("Playing cached file (re-decoded via ffmpeg): {:?}", cache_path);

                            // Cached files may be WAV (written by the streaming
                            // pipeline) or legacy WebM (raw yt-dlp output from
                            // older builds). Decoding through ffmpeg handles
                            // both formats uniformly, writing a temp WAV that
                            // rodio reads.
                            let temp_dir = std::env::temp_dir().join("music_plr");
                            let _ = std::fs::create_dir_all(&temp_dir);
                            let temp_path = temp_dir.join(format!(
                                "{}.wav",
                                cache_path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("cache")
                            ));
                            let temp_str = temp_path.to_string_lossy().to_string();

                            let mut ffmpeg_child = match Command::new("ffmpeg")
                                .args(["-i", "pipe:0", "-f", "wav", "-bitexact", "-y", &temp_str])
                                .stdin(Stdio::piped())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn()
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    warn!("Failed to spawn ffmpeg for cache: {}", e);
                                    continue;
                                }
                            };

                            let Some(ffmpeg_stdin) = ffmpeg_child.stdin.take() else {
                                warn!("ffmpeg stdin not available");
                                let _ = ffmpeg_child.kill();
                                continue;
                            };

                            let cache_path_clone = cache_path.clone();
                            thread::spawn(move || {
                                let file = match std::fs::File::open(&cache_path_clone) {
                                    Ok(f) => f,
                                    Err(e) => {
                                        warn!("Failed to open cache: {}", e);
                                        return;
                                    }
                                };
                                let mut reader = BufReader::new(file);
                                let mut buf = [0u8; 8192];
                                let mut writer = ffmpeg_stdin;
                                loop {
                                    match reader.read(&mut buf) {
                                        Ok(0) | Err(_) => break,
                                        Ok(n) => {
                                            if writer.write_all(&buf[..n]).is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            });

                            ffmpeg = Some(ffmpeg_child);
                            ytdlp = None;
                            // rodio reads the temp WAV; the cache file itself
                            // is left untouched (owned by StreamCache).
                            playback_file = Some(temp_path);
                            expected_duration = duration;
                            stream_active = true;
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
