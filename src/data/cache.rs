use std::{
    collections::HashMap,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::providers::ProviderId;

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
    format!("{}:{id}", provider.slug())
}

fn parse_key(key: &str) -> (ProviderId, String) {
    if let Some((p, id)) = key.split_once(':') {
        (
            ProviderId::from_slug(p).unwrap_or(ProviderId::Local),
            id.to_string(),
        )
    } else {
        (ProviderId::YouTube, key.to_string())
    }
}

/// Pre-slug `Debug:id` index key (e.g. `YouTube:…`, `LastFm:…`) as a
/// `(provider, id)` pair. Only used by the one-time [`migrate_index_keys`];
/// new code exclusively writes `slug:id` keys parsed by [`parse_key`].
fn parse_legacy_key(key: &str) -> Option<(ProviderId, String)> {
    let (p, id) = key.split_once(':')?;
    let provider = match p {
        "YouTube" => ProviderId::YouTube,
        "SoundCloud" => ProviderId::SoundCloud,
        "MusicBrainz" => ProviderId::MusicBrainz,
        "Local" => ProviderId::Local,
        _ => return None,
    };
    Some((provider, id.to_string()))
}
/// One-time rewrite of pre-slug index keys to the `slug:id` format so old
/// entries stay reachable after the directory migration; on collision the
/// fresher entry wins. Returns the rewritten index and whether any key
/// changed (so the caller can persist the migration immediately).
fn migrate_index_keys(entries: HashMap<String, CacheEntry>) -> (HashMap<String, CacheEntry>, bool) {
    let mut remapped: HashMap<String, CacheEntry> = HashMap::new();
    let mut migrated = false;
    for (old_key, entry) in entries {
        let (provider, id) = parse_legacy_key(&old_key).unwrap_or_else(|| parse_key(&old_key));
        let new_key = key(provider, &id);
        migrated |= new_key != old_key;
        if let Some(existing) = remapped.get_mut(&new_key) {
            if entry.last_accessed > existing.last_accessed {
                *existing = entry;
            }
        } else {
            remapped.insert(new_key, entry);
        }
    }
    (remapped, migrated)
}

fn cache_dir() -> PathBuf {
    super::cache_path("streams")
}

fn provider_dir(provider: ProviderId) -> PathBuf {
    cache_dir().join(provider.slug())
}

/// One-time move from pre-slug (`Debug`-named, e.g. `YouTube/`) cache
/// directories to slug-named ones (`youtube/`). Merges when both exist and
/// ignores failures: the cache is regenerable.
fn migrate_legacy_dirs() {
    for &provider in ProviderId::all() {
        let legacy = cache_dir().join(format!("{provider:?}"));
        let current = provider_dir(provider);
        if legacy == current || !legacy.exists() {
            continue;
        }
        if !current.exists() {
            if std::fs::rename(&legacy, &current).is_ok() {
                continue;
            }
            let _ = std::fs::create_dir_all(&current);
        }
        if let Ok(read) = std::fs::read_dir(&legacy) {
            for entry in read.flatten() {
                let dest = current.join(entry.file_name());
                if dest.exists() {
                    let _ = std::fs::remove_file(entry.path());
                } else {
                    let _ = std::fs::rename(entry.path(), &dest);
                }
            }
        }
        let _ = std::fs::remove_dir(&legacy);
    }
}

fn index_path() -> PathBuf {
    cache_dir().join("cache_index.json")
}

impl StreamCache {
    pub fn new(max_size_mb: u64) -> Self {
        let index_path = index_path();
        let _ = std::fs::create_dir_all(cache_dir());
        migrate_legacy_dirs();

        let (index, current_total, index_migrated) = std::fs::read_to_string(&index_path)
            .ok()
            .and_then(|s| serde_json::from_str::<CacheIndex>(&s).ok())
            .map(|idx| {
                let (remapped, migrated) = migrate_index_keys(idx.entries);
                let total: u64 = remapped.values().map(|e| e.size_bytes).sum();
                (CacheIndex { entries: remapped }, total, migrated)
            })
            .unwrap_or_default();

        let this = Self {
            max_size_bytes: max_size_mb * 1024 * 1024,
            index_path,
            index,
            current_total,
        };
        if index_migrated {
            this.save();
        }
        this
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

    pub fn remove(&mut self, provider: ProviderId, id: &str) -> bool {
        let removed_entry = self.index.entries.remove(&key(provider, id));
        let removed_file = std::fs::remove_file(Self::path_for(provider, id)).is_ok();
        let had_entry = if let Some(entry) = removed_entry {
            self.current_total = self.current_total.saturating_sub(entry.size_bytes);
            self.save();
            true
        } else {
            false
        };
        had_entry || removed_file
    }

    pub fn cached_providers_for(&self, track: &crate::types::Track) -> Vec<ProviderId> {
        ProviderId::all()
            .iter()
            .copied()
            .filter(|p| {
                track
                    .provider_id(*p)
                    .is_some_and(|id| self.index_contains(*p, id))
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_per_provider() {
        for &provider in ProviderId::all() {
            let (parsed, id) = parse_key(&key(provider, "abc123"));
            assert_eq!(parsed, provider);
            assert_eq!(id, "abc123");
        }
    }

    #[test]
    fn keys_keep_colons_in_id() {
        let (provider, id) = parse_key(&key(ProviderId::Bandcamp, "1:2:a"));
        assert_eq!(provider, ProviderId::Bandcamp);
        assert_eq!(id, "1:2:a");
    }

    #[test]
    fn migration_rewrites_legacy_keys() {
        for (legacy, provider) in [
            ("YouTube", ProviderId::YouTube),
            ("SoundCloud", ProviderId::SoundCloud),
            ("MusicBrainz", ProviderId::MusicBrainz),
            ("Local", ProviderId::Local),
        ] {
            let (migrated, changed) = migrate_index_keys(HashMap::from([(
                format!("{legacy}:x"),
                CacheEntry {
                    size_bytes: 1,
                    last_accessed: 1,
                },
            )]));
            assert_eq!(
                migrated.keys().collect::<Vec<_>>(),
                [key(provider, "x")].iter().collect::<Vec<_>>()
            );
            assert!(changed);
        }
    }

    #[test]
    fn migration_reports_unchanged_index() {
        let entry = CacheEntry {
            size_bytes: 1,
            last_accessed: 1,
        };
        let (_, changed) =
            migrate_index_keys(HashMap::from([(key(ProviderId::YouTube, "x"), entry)]));
        assert!(!changed);
    }

    #[test]
    fn migration_collision_keeps_fresher_entry() {
        let old = CacheEntry {
            size_bytes: 10,
            last_accessed: 1,
        };
        let fresh = CacheEntry {
            size_bytes: 20,
            last_accessed: 2,
        };
        let (migrated, _) = migrate_index_keys(HashMap::from([
            ("YouTube:x".to_string(), old),
            ("youtube:x".to_string(), fresh),
        ]));
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[&key(ProviderId::YouTube, "x")].size_bytes, 20);
    }

    #[test]
    fn unknown_prefix_falls_back_to_local() {
        assert_eq!(parse_key("nope:x").0, ProviderId::Local);
    }

    #[test]
    fn provider_dirs_use_slugs() {
        for &provider in ProviderId::all() {
            let dir = provider_dir(provider);
            assert_eq!(
                dir.file_name().and_then(|n| n.to_str()),
                Some(provider.slug())
            );
        }
    }
}
