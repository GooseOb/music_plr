//! Index of the playlists that live on a user-configured server.
//!
//! Only identity and metadata are stored here: the tracks of a server playlist
//! are always fetched through the provider `browse` path, so a playlist that
//! was edited elsewhere shows the server's contents instead of a stale copy.
//! Local playlists keep living in [`super::playlists`].

use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
use crate::providers::{ProviderId, ServerPlaylist};

/// Identity of one server-side playlist: its provider plus the server's id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServerPlaylistId {
    pub provider: ProviderId,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerPlaylistEntry {
    pub provider: ProviderId,
    pub id: String,
    pub name: String,
    pub track_count: usize,
}

impl ServerPlaylistEntry {
    pub fn target(&self) -> ServerPlaylistId {
        ServerPlaylistId {
            provider: self.provider,
            id: self.id.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerPlaylistStore {
    pub entries: Vec<ServerPlaylistEntry>,
}

impl JsonStore for ServerPlaylistStore {
    const FILE: &'static str = "server_playlists.json";
    const LOCATION: StoreLocation = StoreLocation::Data;

    fn after_load(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.entries.retain(|entry| seen.insert(entry.target()));
    }
}

impl ServerPlaylistStore {
    pub fn for_provider(&self, provider: ProviderId) -> impl Iterator<Item = &ServerPlaylistEntry> {
        self.entries.iter().filter(move |e| e.provider == provider)
    }

    pub fn get(&self, target: &ServerPlaylistId) -> Option<&ServerPlaylistEntry> {
        self.entries
            .iter()
            .find(|e| e.provider == target.provider && e.id == target.id)
    }

    fn get_mut(&mut self, target: &ServerPlaylistId) -> Option<&mut ServerPlaylistEntry> {
        self.entries
            .iter_mut()
            .find(|e| e.provider == target.provider && e.id == target.id)
    }

    /// Replace every entry of `provider` with a fresh server listing, keeping
    /// the other providers' entries where they are. Saves only when the
    /// listing actually changed, so a periodic refresh is free. Returns
    /// whether the list changed, i.e. whether the sidebar rows moved.
    pub fn replace_provider(&mut self, provider: ProviderId, playlists: &[ServerPlaylist]) -> bool {
        let fresh: Vec<ServerPlaylistEntry> = playlists
            .iter()
            .map(|playlist| ServerPlaylistEntry {
                provider,
                id: playlist.id.clone(),
                name: playlist.name.clone(),
                track_count: playlist.track_count,
            })
            .collect();
        let existing: Vec<ServerPlaylistEntry> = self.for_provider(provider).cloned().collect();
        if existing == fresh {
            return false;
        }
        let insert_at = self
            .entries
            .iter()
            .position(|e| e.provider == provider)
            .unwrap_or(self.entries.len());
        self.entries.retain(|e| e.provider != provider);
        let insert_at = insert_at.min(self.entries.len());
        self.entries.splice(insert_at..insert_at, fresh);
        self.save();
        true
    }

    pub fn insert(&mut self, entry: ServerPlaylistEntry) {
        self.entries
            .retain(|e| !(e.provider == entry.provider && e.id == entry.id));
        self.entries.push(entry);
        self.save();
    }

    pub fn remove(&mut self, target: &ServerPlaylistId) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|e| !(e.provider == target.provider && e.id == target.id));
        let changed = self.entries.len() != before;
        if changed {
            self.save();
        }
        changed
    }

    pub fn rename(&mut self, target: &ServerPlaylistId, name: &str) {
        if let Some(entry) = self.get_mut(target) {
            if entry.name != name {
                entry.name = name.to_string();
                self.save();
            }
        }
    }

    pub fn set_count(&mut self, target: &ServerPlaylistId, count: usize) {
        if let Some(entry) = self.get_mut(target) {
            if entry.track_count != count {
                entry.track_count = count;
                self.save();
            }
        }
    }

    /// Put a row back exactly where it was, for a rolled-back write.
    pub fn restore(&mut self, entry: ServerPlaylistEntry) {
        match self.get_mut(&entry.target()) {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
        self.save();
    }

    /// Shift a row's cached track count by `delta`, so the sidebar agrees with
    /// a write that just landed instead of waiting for the next listing (the
    /// open views refetch and correct it to the server's own number anyway).
    pub fn adjust_count(&mut self, target: &ServerPlaylistId, delta: isize) {
        if let Some(entry) = self.get_mut(target) {
            let count = entry.track_count.saturating_add_signed(delta);
            if entry.track_count != count {
                entry.track_count = count;
                self.save();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlist(id: &str, name: &str, count: usize) -> ServerPlaylist {
        ServerPlaylist {
            id: id.into(),
            name: name.into(),
            track_count: count,
        }
    }

    fn target(id: &str) -> ServerPlaylistId {
        ServerPlaylistId {
            provider: ProviderId::Custom(
                crate::data::custom_providers::CustomProviderKind::Subsonic,
                1,
            ),
            id: id.into(),
        }
    }

    #[test]
    fn replacing_one_provider_leaves_the_others_alone() {
        let jellyfin = ProviderId::Custom(
            crate::data::custom_providers::CustomProviderKind::Jellyfin,
            2,
        );
        let subsonic = target("any").provider;
        let mut store = ServerPlaylistStore::default();
        store.replace_provider(subsonic, &[playlist("a", "One", 1)]);
        store.replace_provider(jellyfin, &[playlist("b", "Two", 2)]);

        store.replace_provider(subsonic, &[playlist("c", "Three", 3)]);
        assert_eq!(store.for_provider(subsonic).count(), 1);
        assert_eq!(store.for_provider(jellyfin).count(), 1);
        assert!(store.get(&target("a")).is_none());
        assert_eq!(
            store.get(&target("c")).map(|e| e.name.as_str()),
            Some("Three")
        );
    }

    #[test]
    fn an_identical_listing_keeps_the_order() {
        let provider = target("any").provider;
        let mut store = ServerPlaylistStore::default();
        store.replace_provider(provider, &[playlist("a", "A", 1), playlist("b", "B", 2)]);
        store.replace_provider(provider, &[playlist("a", "A", 1), playlist("b", "B", 2)]);
        let ids: Vec<&str> = store.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn replacing_reports_whether_the_rows_moved() {
        let provider = target("any").provider;
        let mut store = ServerPlaylistStore::default();
        assert!(store.replace_provider(provider, &[playlist("a", "One", 1)]));
        assert!(!store.replace_provider(provider, &[playlist("a", "One", 1)]));
        assert!(store.replace_provider(provider, &[playlist("b", "Two", 2)]));
        assert!(store.replace_provider(provider, &[]));
    }

    #[test]
    fn rename_and_count_only_touch_the_named_entry() {
        let provider = target("any").provider;
        let mut store = ServerPlaylistStore::default();
        store.replace_provider(provider, &[playlist("a", "A", 1), playlist("b", "B", 2)]);

        store.rename(&target("a"), "Renamed");
        store.set_count(&target("a"), 9);
        assert_eq!(store.get(&target("a")).unwrap().name, "Renamed");
        assert_eq!(store.get(&target("a")).unwrap().track_count, 9);
        assert_eq!(store.get(&target("b")).unwrap().name, "B");
        assert_eq!(store.get(&target("b")).unwrap().track_count, 2);
    }

    #[test]
    fn adjusting_a_count_saturates_at_zero() {
        let provider = target("any").provider;
        let mut store = ServerPlaylistStore::default();
        store.replace_provider(provider, &[playlist("a", "A", 1)]);

        store.adjust_count(&target("a"), 2);
        assert_eq!(store.get(&target("a")).unwrap().track_count, 3);
        store.adjust_count(&target("a"), -9);
        assert_eq!(store.get(&target("a")).unwrap().track_count, 0);
    }

    #[test]
    fn duplicate_ids_are_dropped_on_load() {
        let provider = target("any").provider;
        let mut store = ServerPlaylistStore {
            entries: vec![
                ServerPlaylistEntry {
                    provider,
                    id: "a".into(),
                    name: "A".into(),
                    track_count: 1,
                },
                ServerPlaylistEntry {
                    provider,
                    id: "a".into(),
                    name: "A again".into(),
                    track_count: 1,
                },
            ],
        };
        store.after_load();
        assert_eq!(store.entries.len(), 1);
    }
}
