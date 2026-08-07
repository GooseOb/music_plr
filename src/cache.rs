use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    size_bytes: u64,
    last_accessed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CacheIndex {
    entries: HashMap<String, CacheEntry>,
}

pub struct StreamCache {
    max_size_bytes: u64,
    cache_dir: PathBuf,
    index_path: PathBuf,
    index: CacheIndex,
    current_total: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn project_dirs() -> directories::ProjectDirs {
    directories::ProjectDirs::from("", "", "music_plr")
        .expect("failed to determine project directories")
}

fn cache_dir() -> PathBuf {
    project_dirs().cache_dir().join("youtube")
}

fn index_path() -> PathBuf {
    cache_dir().join("cache_index.json")
}

impl StreamCache {
    pub fn new(max_size_mb: u64) -> Self {
        let cache_dir = cache_dir();
        let index_path = index_path();
        let _ = std::fs::create_dir_all(&cache_dir);

        let (index, current_total) = std::fs::read_to_string(&index_path)
            .ok()
            .and_then(|s| serde_json::from_str::<CacheIndex>(&s).ok())
            .map(|idx| {
                let total: u64 = idx.entries.values().map(|e| e.size_bytes).sum();
                (idx, total)
            })
            .unwrap_or_default();

        Self {
            max_size_bytes: max_size_mb * 1024 * 1024,
            cache_dir,
            index_path,
            index,
            current_total,
        }
    }

    pub fn path_for(&self, id: &str) -> PathBuf {
        self.cache_dir.join(format!("{id}.cache"))
    }

    pub fn contains(&self, id: &str) -> bool {
        self.index.entries.contains_key(id) && self.path_for(id).exists()
    }

    /// In-memory check of whether `id` has a completed cache entry in the
    /// index. Unlike `contains`, this does NOT touch the filesystem — the
    /// index is loaded into memory at startup and updated as streams finish,
    /// so it's safe to call on every redraw.
    pub fn index_contains(&self, id: &str) -> bool {
        self.index.entries.contains_key(id)
    }

    pub fn insert(&mut self, id: &str) -> bool {
        let path = self.path_for(id);
        if !path.exists() {
            return false;
        }
        let size = path.metadata().map_or(0, |m| m.len());
        if size < 4096 {
            let _ = std::fs::remove_file(&path);
            return false;
        }
        let now = now_secs();
        if let Some(entry) = self.index.entries.get_mut(id) {
            entry.last_accessed = now;
            self.save();
            return true;
        }
        self.index.entries.insert(
            id.to_string(),
            CacheEntry {
                size_bytes: size,
                last_accessed: now,
            },
        );
        self.current_total += size;
        self.evict();
        self.save();
        true
    }

    fn evict(&mut self) {
        if self.current_total <= self.max_size_bytes {
            return;
        }

        let mut sorted: Vec<(String, CacheEntry)> = self.index.entries.drain().collect();
        sorted.sort_by_key(|(_, e)| e.last_accessed);

        let mut kept: HashMap<String, CacheEntry> = HashMap::new();
        for (id, entry) in sorted {
            if self.current_total > self.max_size_bytes {
                let path = self.path_for(&id);
                let _ = std::fs::remove_file(&path);
                self.current_total = self.current_total.saturating_sub(entry.size_bytes);
            } else {
                kept.insert(id, entry);
            }
        }
        self.index.entries = kept;
    }

    fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(&self.index) {
            let _ = std::fs::write(&self.index_path, s);
        }
    }
}
