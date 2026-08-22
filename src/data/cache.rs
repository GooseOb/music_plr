use crate::providers::ProviderId;
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

fn key(provider: ProviderId, id: &str) -> String {
    format!("{provider:?}:{id}")
}

fn parse_key(key: &str) -> (ProviderId, String) {
    // Best-effort parse; provider enum debug labels are stable.
    if let Some((p, id)) = key.split_once(':') {
        let provider = match p {
            "YouTube" => ProviderId::YouTube,
            "SoundCloud" => ProviderId::SoundCloud,
            "MusicBrainz" => ProviderId::MusicBrainz,
            _ => ProviderId::Local,
        };
        (provider, id.to_string())
    } else {
        (ProviderId::YouTube, key.to_string())
    }
}

fn cache_dir() -> PathBuf {
    super::cache_path("streams")
}

fn provider_dir(provider: ProviderId) -> PathBuf {
    cache_dir().join(format!("{provider:?}"))
}

fn index_path() -> PathBuf {
    cache_dir().join("cache_index.json")
}

impl StreamCache {
    pub fn new(max_size_mb: u64) -> Self {
        let index_path = index_path();
        let _ = std::fs::create_dir_all(cache_dir());

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
            index_path,
            index,
            current_total,
        }
    }

    pub fn path_for(provider: ProviderId, id: &str) -> PathBuf {
        provider_dir(provider).join(format!("{id}.cache"))
    }

    /// Update the cache size cap (used by the Settings view). Eviction itself
    /// is deferred to the next `insert`, so this only changes the threshold.
    pub fn set_max_size_mb(&mut self, mb: u64) {
        self.max_size_bytes = mb * 1024 * 1024;
    }

    pub fn contains(&self, provider: ProviderId, id: &str) -> bool {
        self.index.entries.contains_key(&key(provider, id)) && Self::path_for(provider, id).exists()
    }

    /// In-memory check of whether `id` has a completed cache entry in the
    /// index. Unlike `contains`, this does NOT touch the filesystem — the
    /// index is loaded into memory at startup and updated as streams finish,
    /// so it's safe to call on every redraw.
    pub fn index_contains(&self, provider: ProviderId, id: &str) -> bool {
        self.index.entries.contains_key(&key(provider, id))
    }

    pub fn insert(&mut self, provider: ProviderId, id: &str) -> bool {
        let key = key(provider, id);
        let path = Self::path_for(provider, id);
        if !path.exists() {
            return false;
        }
        let size = path.metadata().map_or(0, |m| m.len());
        if size < 4096 {
            let _ = std::fs::remove_file(&path);
            return false;
        }
        let now = now_secs();
        if let Some(entry) = self.index.entries.get_mut(&key) {
            entry.last_accessed = now;
            self.save();
            return true;
        }
        self.index.entries.insert(
            key,
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
        for (key, entry) in sorted {
            if self.current_total > self.max_size_bytes {
                let (provider, id) = parse_key(&key);
                let path = Self::path_for(provider, &id);
                let _ = std::fs::remove_file(&path);
                self.current_total = self.current_total.saturating_sub(entry.size_bytes);
            } else {
                kept.insert(key, entry);
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
