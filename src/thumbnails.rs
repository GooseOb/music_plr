use std::path::PathBuf;
use tracing::{debug, warn};

fn project_dirs() -> directories::ProjectDirs {
    directories::ProjectDirs::from("", "", "music_plr").expect("project dirs")
}

fn thumbnails_dir() -> PathBuf {
    project_dirs().cache_dir().join("thumbnails")
}

pub fn thumbnail_path(video_id: &str) -> PathBuf {
    thumbnails_dir().join(format!("{video_id}.jpg"))
}

pub fn thumbnail_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg")
}

pub fn download(video_id: &str, url: &str) {
    let path = thumbnail_path(video_id);
    if path.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(thumbnails_dir());
    let url = if url.is_empty() {
        thumbnail_url(video_id)
    } else {
        url.to_string()
    };
    match ureq::get(&url).call() {
        Ok(resp) => {
            let mut body = resp.into_body();
            let mut reader = body.as_reader();
            if let Ok(mut file) = std::fs::File::create(&path) {
                if std::io::copy(&mut reader, &mut file).is_err() {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Err(e) => {
            warn!("Failed to download {video_id}: {e}");
        }
    }
}
