use rodio::Source;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
}

enum PlayerCommand {
    Play(Vec<u8>, f32),
    PlayStream(String, f32),
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
        }));

        let (cmd_tx, cmd_rx) = mpsc::channel::<PlayerCommand>();
        let state_clone = state.clone();

        thread::spawn(move || {
            let mut output: Option<(rodio::OutputStream, rodio::Sink)> = None;
            let mut stream_process: Option<std::process::Child> = None;
            let mut ytdlp_process: Option<std::process::Child> = None;
            let mut stream_path: Option<PathBuf> = None;
            let mut stream_url: Option<String> = None;
            let mut stream_duration: f32 = 0.0;

            loop {
                match cmd_rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(cmd) => match cmd {
                        PlayerCommand::Play(bytes, expected_duration) => {
                            eprintln!("[audio] Play command received, {} bytes", bytes.len());
                            Self::kill_processes(
                                &mut stream_process,
                                &mut ytdlp_process,
                                &mut stream_path,
                                &mut stream_url,
                            );
                            if let Some((_, s)) = &output {
                                s.stop();
                            }
                            output = None;

                            if let Ok((stream, handle)) = rodio::OutputStream::try_default() {
                                if let Ok(sink) = rodio::Sink::try_new(&handle) {
                                    let cursor = Cursor::new(bytes);
                                    if let Ok(source) = rodio::Decoder::new(cursor) {
                                        let vol =
                                            state_clone.lock().map(|st| st.volume).unwrap_or(1.0);
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
                                        }
                                        output = Some((stream, sink));
                                    }
                                }
                            }
                        }

                        PlayerCommand::PlayStream(url, expected_duration) => {
                            if let Some(ref current) = stream_url {
                                if current == &url {
                                    eprintln!("[audio] Ignoring duplicate PlayStream for same URL");
                                    continue;
                                }
                            }

                            Self::kill_processes(
                                &mut stream_process,
                                &mut ytdlp_process,
                                &mut stream_path,
                                &mut stream_url,
                            );
                            if let Some((_, s)) = &output {
                                s.stop();
                            }
                            output = None;

                            // Create temp path
                            let id = url
                                .split("v=")
                                .nth(1)
                                .and_then(|s| s.split('&').next())
                                .unwrap_or("stream");
                            let temp_dir = std::env::temp_dir().join("music_plr");
                            let _ = std::fs::create_dir_all(&temp_dir);
                            let temp_path = temp_dir.join(format!("{}.wav", id));
                            let temp_str = temp_path.to_string_lossy().to_string();

                            eprintln!("[audio] Piping yt-dlp into ffmpeg...");

                            // Pipe yt-dlp audio output directly into ffmpeg (single combined step)
                            let mut ytdlp = match Command::new("yt-dlp")
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
                                    eprintln!("[audio] Failed to spawn yt-dlp: {}", e);
                                    continue;
                                }
                            };

                            let ytdlp_stdout = match ytdlp.stdout.take() {
                                Some(s) => s,
                                None => {
                                    eprintln!("[audio] yt-dlp stdout not available");
                                    continue;
                                }
                            };

                            let ffmpeg = match Command::new("ffmpeg")
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
                                .stdin(ytdlp_stdout)
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn()
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    eprintln!("[audio] Failed to spawn ffmpeg: {}", e);
                                    let _ = ytdlp.kill();
                                    let _ = std::fs::remove_file(&temp_path);
                                    continue;
                                }
                            };

                            // Store pending stream state, playback starts in main loop
                            stream_process = Some(ffmpeg);
                            ytdlp_process = Some(ytdlp);
                            stream_path = Some(temp_path);
                            stream_url = Some(url);
                            stream_duration = expected_duration;
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
                        }
                    }
                }

                if stream_process
                    .as_mut()
                    .is_some_and(|p| p.try_wait().is_ok_and(|s| s.is_some()))
                {
                    stream_process.take();
                }
                if ytdlp_process
                    .as_mut()
                    .is_some_and(|p| p.try_wait().is_ok_and(|s| s.is_some()))
                {
                    ytdlp_process.take();
                }

                if output.is_none() {
                    if let Some(ref path) = stream_path {
                        if let Some(ref mut process) = stream_process {
                            let ready = std::fs::metadata(path)
                                .map(|m| m.len() > 2048)
                                .unwrap_or(false);
                            let exited = process.try_wait().ok().flatten().is_some();

                            if ready || exited {
                                eprintln!("[audio] Stream ready, starting playback");
                                if let Ok((stream, handle)) = rodio::OutputStream::try_default() {
                                    if let Ok(sink) = rodio::Sink::try_new(&handle) {
                                        if let Ok(file) = std::fs::File::open(path) {
                                            if let Ok(source) = rodio::Decoder::new(file) {
                                                let vol = state_clone
                                                    .lock()
                                                    .map(|st| st.volume)
                                                    .unwrap_or(1.0);
                                                let actual_duration = if stream_duration > 0.0 {
                                                    stream_duration
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
            }

            // Cleanup on thread exit
            if let Some(ref path) = stream_path {
                let _ = std::fs::remove_file(path);
            }
        });

        Self { cmd_tx, state }
    }

    fn kill_processes(
        stream_process: &mut Option<std::process::Child>,
        ytdlp_process: &mut Option<std::process::Child>,
        stream_path: &mut Option<PathBuf>,
        stream_url: &mut Option<String>,
    ) {
        if let Some(mut p) = stream_process.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
        if let Some(mut p) = ytdlp_process.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
        if let Some(ref path) = stream_path {
            let _ = std::fs::remove_file(path);
        }
        *stream_path = None;
        *stream_url = None;
    }

    pub fn play(&mut self, audio_data: Vec<u8>, duration: f32) {
        let _ = self.cmd_tx.send(PlayerCommand::Play(audio_data, duration));
    }

    pub fn play_stream(&mut self, url: &str, duration: f32) {
        let _ = self
            .cmd_tx
            .send(PlayerCommand::PlayStream(url.to_string(), duration));
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
