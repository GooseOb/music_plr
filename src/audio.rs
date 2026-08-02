use rodio::Source;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
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
            let mut temp_wav: Option<PathBuf> = None;
            let mut stream_url: Option<String> = None;
            let mut expected_duration: f32 = 0.0;
            let mut stream_active: bool = false;

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

                            Self::kill_processes(
                                &mut ffmpeg,
                                &mut ytdlp,
                                &mut temp_wav,
                                &mut stream_url,
                            );
                            stream_active = false;
                            if let Some((_, s)) = &output {
                                s.stop();
                            }
                            output = None;
                            if let Ok(mut st) = state_clone.lock() {
                                st.stream_finished = false;
                            }

                            let id = url
                                .split("v=")
                                .nth(1)
                                .and_then(|s| s.split('&').next())
                                .unwrap_or("stream");
                            let temp_dir = std::env::temp_dir().join("music_plr");
                            let _ = std::fs::create_dir_all(&temp_dir);
                            let temp_path = temp_dir.join(format!("{}.wav", id));
                            let temp_str = temp_path.to_string_lossy().to_string();

                            debug!("Spawning yt-dlp, teeing raw output to cache + ffmpeg");

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

                            let ytdlp_stdout = match ytdlp_child.stdout.take() {
                                Some(s) => s,
                                None => {
                                    warn!("yt-dlp stdout not available");
                                    continue;
                                }
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
                                    &temp_str,
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
                                    let _ = std::fs::remove_file(&temp_path);
                                    continue;
                                }
                            };

                            let ffmpeg_stdin = match ffmpeg_child.stdin.take() {
                                Some(s) => s,
                                None => {
                                    warn!("ffmpeg stdin not available");
                                    let _ = ytdlp_child.kill();
                                    let _ = ffmpeg_child.kill();
                                    let _ = std::fs::remove_file(&temp_path);
                                    continue;
                                }
                            };

                            // Ensure cache directory exists
                            if let Some(dir) = cache_path.parent() {
                                let _ = std::fs::create_dir_all(dir);
                            }

                            // Spawn tee thread: reads yt-dlp stdout, writes to both
                            // cache file and ffmpeg stdin
                            let cache_path_clone = cache_path.clone();
                            thread::spawn(move || {
                                let mut cache_file = match std::fs::File::create(&cache_path_clone)
                                {
                                    Ok(f) => f,
                                    Err(e) => {
                                        warn!("Failed to create cache file: {}", e);
                                        return;
                                    }
                                };
                                let mut buf = [0u8; 8192];
                                let mut reader = ytdlp_stdout;
                                let mut writer = ffmpeg_stdin;
                                loop {
                                    match reader.read(&mut buf) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            if let Err(e) = cache_file.write_all(&buf[..n]) {
                                                warn!("Cache write error: {}", e);
                                                break;
                                            }
                                            if writer.write_all(&buf[..n]).is_err() {
                                                // ffmpeg stdin closed (broken pipe) - OK
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
                            temp_wav = Some(temp_path);
                            stream_url = Some(url);
                            expected_duration = duration;
                            stream_active = true;
                        }

                        PlayerCommand::PlayCached {
                            cache_path,
                            duration,
                        } => {
                            Self::kill_processes(
                                &mut ffmpeg,
                                &mut ytdlp,
                                &mut temp_wav,
                                &mut stream_url,
                            );
                            stream_active = false;
                            if let Some((_, s)) = &output {
                                s.stop();
                            }
                            output = None;
                            if let Ok(mut st) = state_clone.lock() {
                                st.stream_finished = false;
                            }

                            let id = cache_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("cache");
                            let temp_dir = std::env::temp_dir().join("music_plr");
                            let _ = std::fs::create_dir_all(&temp_dir);
                            let temp_path = temp_dir.join(format!("{}.wav", id));
                            let temp_str = temp_path.to_string_lossy().to_string();

                            debug!("Playing cached file through ffmpeg: {:?}", cache_path);

                            // Pipe the cached file through stdin (sequential read)
                            // to handle truncated webm containers gracefully.
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

                            let ffmpeg_stdin = match ffmpeg_child.stdin.take() {
                                Some(s) => s,
                                None => {
                                    warn!("ffmpeg stdin not available");
                                    let _ = ffmpeg_child.kill();
                                    let _ = std::fs::remove_file(&temp_path);
                                    continue;
                                }
                            };

                            // Feed cache file into ffmpeg stdin in a background thread
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
                                        Ok(0) => break,
                                        Ok(n) => {
                                            if writer.write_all(&buf[..n]).is_err() {
                                                break;
                                            }
                                        }
                                        Err(_) => break,
                                    }
                                }
                            });

                            ffmpeg = Some(ffmpeg_child);
                            ytdlp = None;
                            temp_wav = Some(temp_path);
                            stream_url = None;
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

                // Detect stream completion
                if stream_active {
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
                    }
                }

                // Start playback once WAV has enough data
                if output.is_none() && stream_active {
                    if let Some(ref path) = temp_wav {
                        let ready = std::fs::metadata(path)
                            .map(|m| m.len() > 2048)
                            .unwrap_or(false);
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
                                            let vol = state_clone
                                                .lock()
                                                .map(|st| st.volume)
                                                .unwrap_or(1.0);
                                            let actual_duration = if expected_duration > 0.0 {
                                                expected_duration
                                            } else {
                                                source
                                                    .total_duration()
                                                    .map(|d| d.as_secs_f32())
                                                    .unwrap_or(0.0)
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
            }

            // Cleanup on thread exit
            if let Some(ref path) = temp_wav {
                let _ = std::fs::remove_file(path);
            }
        });

        Self { cmd_tx, state }
    }

    fn kill_processes(
        ffmpeg: &mut Option<std::process::Child>,
        ytdlp: &mut Option<std::process::Child>,
        temp_wav: &mut Option<PathBuf>,
        stream_url: &mut Option<String>,
    ) {
        if let Some(mut p) = ffmpeg.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
        if let Some(mut p) = ytdlp.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
        if let Some(ref path) = temp_wav {
            let _ = std::fs::remove_file(path);
        }
        *temp_wav = None;
        *stream_url = None;
    }

    pub fn play_stream_cache(&mut self, url: &str, duration: f32, cache_path: PathBuf) {
        let _ = self.cmd_tx.send(PlayerCommand::StreamAndCache {
            url: url.to_string(),
            duration,
            cache_path,
        });
    }

    pub fn play_cached(&mut self, cache_path: PathBuf, duration: f32) {
        let _ = self.cmd_tx.send(PlayerCommand::PlayCached {
            cache_path,
            duration,
        });
    }

    pub fn pause(&mut self) {
        let _ = self.cmd_tx.send(PlayerCommand::Pause);
    }

    pub fn resume(&mut self) {
        let _ = self.cmd_tx.send(PlayerCommand::Resume);
    }

    pub fn set_volume(&mut self, vol: f32) {
        let _ = self.cmd_tx.send(PlayerCommand::SetVolume(vol));
    }

    pub fn seek(&mut self, pos: Duration) {
        let _ = self.cmd_tx.send(PlayerCommand::Seek(pos));
    }

    pub fn get_state(&self) -> PlayerState {
        self.state
            .lock()
            .map(|st| st.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }
}
