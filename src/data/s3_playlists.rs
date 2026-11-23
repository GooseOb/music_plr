//! Cache of the playlists that live in an S3-compatible bucket.
//!
//! The bucket's `playlists/` objects are the truth; this store is the local
//! copy that makes the sidebar, the playlist page and offline editing work
//! without a round trip. An entry whose cache differs from the bucket is
//! marked dirty and flushed by the next write/list/browse.

use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
use crate::{providers::ProviderId, types::Track};

/// One bucket playlist and the last state the app saw (or wrote).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct S3PlaylistEntry {
    pub provider: ProviderId,
    pub id: String,
    pub name: String,
    pub tracks: Vec<Track>,
    /// Entity tag of the object the cached state came from; empty when the
    /// object has not been read yet.
    #[serde(default)]
    pub etag: String,
    /// Unix seconds when the bucket's contents were last read.
    #[serde(default)]
    pub fetched_at: u64,
    /// The cache holds an edit that has not reached the bucket.
    #[serde(default)]
    pub dirty: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct S3PlaylistStore {
    pub entries: Vec<S3PlaylistEntry>,
}

impl JsonStore for S3PlaylistStore {
    const FILE: &'static str = "s3_playlists.json";
    const LOCATION: StoreLocation = StoreLocation::Data;

    fn after_load(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.entries
            .retain(|entry| seen.insert((entry.provider, entry.id.clone())));
    }
}

impl S3PlaylistStore {
    pub fn get(&self, provider: ProviderId, id: &str) -> Option<&S3PlaylistEntry> {
        self.entries
            .iter()
            .find(|entry| entry.provider == provider && entry.id == id)
    }

    pub fn upsert(&mut self, entry: S3PlaylistEntry) {
        self.upsert_in_place(entry);
        self.save();
    }

    /// Replace or append `entry` without touching the file, so a batch of
    /// writes saves once.
    fn upsert_in_place(&mut self, entry: S3PlaylistEntry) {
        if let Some(slot) = self
            .entries
            .iter_mut()
            .find(|existing| existing.provider == entry.provider && existing.id == entry.id)
        {
            *slot = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn remove(&mut self, provider: ProviderId, id: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|entry| !(entry.provider == provider && entry.id == id));
        let changed = before != self.entries.len();
        if changed {
            self.save();
        }
        changed
    }

    pub fn forget_provider(&mut self, provider: ProviderId) {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.provider != provider);
        if before != self.entries.len() {
            self.save();
        }
    }

    pub fn for_provider(&self, provider: ProviderId) -> impl Iterator<Item = &S3PlaylistEntry> {
        self.entries.iter().filter(move |e| e.provider == provider)
    }
}

static STORE: OnceLock<RwLock<S3PlaylistStore>> = OnceLock::new();

fn store() -> &'static RwLock<S3PlaylistStore> {
    STORE.get_or_init(|| RwLock::new(S3PlaylistStore::load()))
}

fn read() -> std::sync::RwLockReadGuard<'static, S3PlaylistStore> {
    store()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write() -> std::sync::RwLockWriteGuard<'static, S3PlaylistStore> {
    store()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn get(provider: ProviderId, id: &str) -> Option<S3PlaylistEntry> {
    read().get(provider, id).cloned()
}

pub fn put(entry: S3PlaylistEntry) {
    write().upsert(entry);
}

/// Store several entries with one file write (an offline flush).
pub fn put_all(entries: Vec<S3PlaylistEntry>) {
    if entries.is_empty() {
        return;
    }
    let mut guard = write();
    for entry in entries {
        guard.upsert_in_place(entry);
    }
    guard.save();
}

pub fn remove(provider: ProviderId, id: &str) -> bool {
    write().remove(provider, id)
}

pub fn set_fetched(provider: ProviderId, id: &str, fetched_at: u64, etag: String) {
    let mut guard = write();
    if let Some(entry) = guard
        .entries
        .iter_mut()
        .find(|entry| entry.provider == provider && entry.id == id)
    {
        entry.etag = etag;
        entry.fetched_at = fetched_at;
        entry.dirty = false;
        guard.save();
    }
}

pub fn dirty(provider: ProviderId) -> Vec<S3PlaylistEntry> {
    read()
        .for_provider(provider)
        .filter(|entry| entry.dirty)
        .cloned()
        .collect()
}

pub fn for_provider(provider: ProviderId) -> Vec<S3PlaylistEntry> {
    read().for_provider(provider).cloned().collect()
}

/// Drop cached playlists of `provider` that are neither in `keep` nor waiting
/// to be written.
pub fn retain(provider: ProviderId, keep: &std::collections::HashSet<String>) {
    let mut guard = write();
    let before = guard.entries.len();
    guard
        .entries
        .retain(|entry| entry.provider != provider || entry.dirty || keep.contains(&entry.id));
    if guard.entries.len() != before {
        guard.save();
    }
}

pub fn forget_provider(provider: ProviderId) {
    write().forget_provider(provider);
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::custom_providers::CustomProviderKind;

    fn provider() -> ProviderId {
        ProviderId::Custom(CustomProviderKind::S3, 3)
    }

    fn entry(id: &str, name: &str) -> S3PlaylistEntry {
        S3PlaylistEntry {
            provider: provider(),
            id: id.into(),
            name: name.into(),
            tracks: Vec::new(),
            etag: String::new(),
            fetched_at: 0,
            dirty: false,
        }
    }

    #[test]
    fn upsert_replaces_by_provider_and_id() {
        let mut store = S3PlaylistStore::default();
        store.upsert(entry("a", "One"));
        store.upsert(entry("a", "Renamed"));
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].name, "Renamed");
    }

    #[test]
    fn removing_and_forgetting_only_touch_their_scope() {
        let mut store = S3PlaylistStore::default();
        store.upsert(entry("a", "A"));
        store.upsert(entry("b", "B"));
        assert!(store.remove(provider(), "a"));
        assert!(!store.remove(provider(), "a"));
        store.upsert(entry("c", "C"));

        let other = ProviderId::Custom(CustomProviderKind::S3, 4);
        let mut other_entry = entry("a", "Other");
        other_entry.provider = other;
        store.upsert(other_entry);

        store.forget_provider(provider());
        assert_eq!(store.for_provider(provider()).count(), 0);
        assert_eq!(store.for_provider(other).count(), 1);
    }

    #[test]
    fn batched_upserts_land_before_one_save() {
        let mut store = S3PlaylistStore::default();
        store.upsert_in_place(entry("a", "A"));
        store.upsert_in_place(entry("b", "B"));
        store.upsert_in_place(entry("a", "A again"));
        assert_eq!(store.entries.len(), 2);
        assert_eq!(store.get(provider(), "a").unwrap().name, "A again");
        assert_eq!(store.get(provider(), "b").unwrap().name, "B");
    }

    #[test]
    fn duplicate_ids_are_dropped_on_load() {
        let mut store = S3PlaylistStore {
            entries: vec![entry("a", "A"), entry("a", "A again")],
        };
        store.after_load();
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].name, "A");
    }

    #[test]
    fn entries_round_trip_through_json() {
        let mut original = entry("road-trip", "Road trip");
        original.tracks.push(crate::types::Track {
            title: "Song".into(),
            artists: vec!["Artist".into()],
            source: ProviderId::YouTube,
            providers: std::collections::HashMap::new(),
        });
        let json = serde_json::to_string(&original).unwrap();
        let restored: S3PlaylistEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, original);
    }
}
