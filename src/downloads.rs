use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadRegistry {
    tracks: HashMap<String, String>,
}

impl DownloadRegistry {
    pub fn load() -> Self {
        let path = registry_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    #[cfg(not(test))]
    pub fn save(&self) {
        let path = registry_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, s);
        }
    }

    #[cfg(test)]
    pub fn save(&self) {}

    pub fn register(&mut self, url: &str, path: &str) {
        self.tracks.insert(url.to_string(), path.to_string());
        self.save();
    }

    pub fn remove(&mut self, url: &str) -> Option<String> {
        let result = self.tracks.remove(url);
        self.save();
        result
    }

    pub fn get_path(&self, url: &str) -> Option<&str> {
        self.tracks.get(url).map(|s| s.as_str())
    }

    pub fn contains(&self, url: &str) -> bool {
        self.tracks.contains_key(url)
    }
}

fn registry_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("", "", "music_plr") {
        dirs.config_dir().join("downloads.json")
    } else {
        PathBuf::from("downloads.json")
    }
}
