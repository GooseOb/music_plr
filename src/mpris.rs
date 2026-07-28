use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;
use zbus::connection;
use zbus::interface;
use zbus::zvariant;

#[derive(Debug, Clone)]
pub enum MprisCommand {
    TogglePlayPause,
    NextTrack,
    PreviousTrack,
    SetVolume(f32),
    Seek(i64),
}

#[derive(Debug, Clone)]
pub struct MprisUpdate {
    pub playback_status: String,
    pub title: String,
    pub artist: String,
    pub duration_secs: f32,
    pub position_us: i64,
    pub volume: f32,
    pub has_track: bool,
}

struct MprisData {
    playback_status: String,
    title: String,
    artist: String,
    duration_us: i64,
    position_us: i64,
    volume: f64,
    has_track: bool,
}

struct MediaPlayer2;

#[interface(name = "org.mpris.MediaPlayer2")]
impl MediaPlayer2 {
    fn can_quit(&self) -> bool {
        false
    }

    fn can_raise(&self) -> bool {
        false
    }

    fn has_track_list(&self) -> bool {
        false
    }

    fn identity(&self) -> &str {
        "Music PLR"
    }

    fn desktop_entry(&self) -> &str {
        "music_plr"
    }

    fn supported_uri_schemes(&self) -> Vec<&str> {
        vec!["file", "https"]
    }

    fn supported_mime_types(&self) -> Vec<&str> {
        vec![]
    }
}

struct PlayerInterface {
    data: std::sync::Mutex<MprisData>,
    cmd_tx: mpsc::Sender<MprisCommand>,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerInterface {
    async fn next(&self) {
        let _ = self.cmd_tx.send(MprisCommand::NextTrack);
    }

    async fn previous(&self) {
        let _ = self.cmd_tx.send(MprisCommand::PreviousTrack);
    }

    async fn pause(&self) {
        let _ = self.cmd_tx.send(MprisCommand::TogglePlayPause);
    }

    async fn play_pause(&self) {
        let _ = self.cmd_tx.send(MprisCommand::TogglePlayPause);
    }

    async fn stop(&self) {
        let _ = self.cmd_tx.send(MprisCommand::TogglePlayPause);
    }

    async fn play(&self) {
        let _ = self.cmd_tx.send(MprisCommand::TogglePlayPause);
    }

    async fn seek(&self, offset: i64) {
        let _ = self.cmd_tx.send(MprisCommand::Seek(offset));
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.data.lock().unwrap().playback_status.clone()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, zvariant::Value<'static>> {
        let d = self.data.lock().unwrap();
        let mut map = HashMap::new();
        if d.has_track {
            map.insert(
                "mpris:trackid".into(),
                zvariant::Value::from("/org/mpris/MediaPlayer2/Track/1"),
            );
            map.insert("xesam:title".into(), zvariant::Value::from(d.title.clone()));
            map.insert(
                "xesam:artist".into(),
                zvariant::Value::from(vec![d.artist.clone()]),
            );
            if d.duration_us > 0 {
                map.insert("mpris:length".into(), zvariant::Value::from(d.duration_us));
            }
        }
        map
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.data.lock().unwrap().volume
    }

    #[zbus(property)]
    async fn set_volume(&self, vol: f64) {
        if let Ok(mut d) = self.data.lock() {
            d.volume = vol;
        }
        let _ = self.cmd_tx.send(MprisCommand::SetVolume(vol as f32));
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        self.data.lock().unwrap().position_us
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
}

pub fn start(cmd_tx: mpsc::Sender<MprisCommand>, update_rx: mpsc::Receiver<MprisUpdate>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("[mpris] Failed to create tokio runtime: {}", e);
                return;
            }
        };

        rt.block_on(async move {
            let conn = match connection::Builder::session() {
                Ok(builder) => match builder.build().await {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[mpris] Failed to connect to D-Bus session bus: {}", e);
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("[mpris] Failed to create D-Bus session builder: {}", e);
                    return;
                }
            };

            let data = std::sync::Mutex::new(MprisData {
                playback_status: "Stopped".into(),
                title: String::new(),
                artist: String::new(),
                duration_us: 0,
                position_us: 0,
                volume: 0.8,
                has_track: false,
            });

            let media_player2 = MediaPlayer2;
            let player = PlayerInterface { data, cmd_tx };

            if let Err(e) = conn
                .object_server()
                .at("/org/mpris/MediaPlayer2", media_player2)
                .await
            {
                eprintln!("[mpris] Failed to register MediaPlayer2: {}", e);
                return;
            }

            if let Err(e) = conn
                .object_server()
                .at("/org/mpris/MediaPlayer2", player)
                .await
            {
                eprintln!("[mpris] Failed to register Player: {}", e);
                return;
            }

            if let Err(e) = conn.request_name("org.mpris.MediaPlayer2.music_plr").await {
                eprintln!("[mpris] Failed to request D-Bus name: {}", e);
                return;
            }

            eprintln!("[mpris] MPRIS server started");

            loop {
                while let Ok(update) = update_rx.try_recv() {
                    if let Ok(iface_ref) = conn
                        .object_server()
                        .interface::<_, PlayerInterface>("/org/mpris/MediaPlayer2")
                        .await
                    {
                        let guard = iface_ref.get_mut().await;
                        let mut data = guard.data.lock().unwrap();
                        data.playback_status = update.playback_status;
                        data.title = update.title;
                        data.artist = update.artist;
                        data.duration_us = (update.duration_secs * 1_000_000.0) as i64;
                        data.position_us = update.position_us;
                        data.volume = update.volume as f64;
                        data.has_track = update.has_track;
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    });
}
