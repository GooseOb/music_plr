use super::{JsonStore, StoreLocation};
use crate::providers::ProviderId;
use serde::{Deserialize, Serialize};

/// What kind of thing a [`LibraryItem`] represents. Distinguishes the three
/// saveable entity types so the saved list and lookups can be grouped/filtered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryKind {
    Album,
    Artist,
    Playlist,
}

/// A single saved library entry. Mirrors the fields of a `CardData` plus the
/// kind, so a saved item can be dragged (onto the playlist list or library)
/// or opened via the card-press mechanism. `provider` records which provider
/// supplied `id`, so reopening the item hits the right backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryItem {
    pub kind: LibraryKind,
    pub id: String,
    pub title: String,
    pub thumbnail: String,
    #[serde(default)]
    pub provider: ProviderId,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryStore {
    pub items: Vec<LibraryItem>,
}

impl JsonStore for LibraryStore {
    const FILE: &'static str = "library.json";
    const LOCATION: StoreLocation = StoreLocation::Data;
}

impl LibraryStore {
    /// Whether an item with the given `kind` and `id` is already saved.
    pub fn contains(&self, kind: LibraryKind, id: &str) -> bool {
        self.items.iter().any(|it| it.kind == kind && it.id == id)
    }

    /// Save an item, replacing any existing one with the same kind+id.
    /// No-op if it is already present (so repeated saves are idempotent).
    pub fn add(&mut self, item: LibraryItem) {
        if !self.contains(item.kind, &item.id) {
            self.items.push(item);
            self.save_to_disk();
        }
    }

    /// Remove the item with the given kind+id. Returns true if something was
    /// removed.
    pub fn remove(&mut self, kind: LibraryKind, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|it| !(it.kind == kind && it.id == id));
        let removed = self.items.len() != before;
        if removed {
            self.save_to_disk();
        }
        removed
    }

    /// Insert an item at `pos` (clamped), skipping it if an identical kind+id
    /// already exists. Persists to disk.
    pub fn insert(&mut self, item: LibraryItem, pos: usize) {
        if self.contains(item.kind, &item.id) {
            return;
        }
        let pos = pos.min(self.items.len());
        self.items.insert(pos, item);
        self.save_to_disk();
    }

    /// Move the item at `from` to `to` (clamped), persisting the result.
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.items.len() {
            return;
        }
        let item = self.items.remove(from);
        let to = to.min(self.items.len());
        self.items.insert(to, item);
        self.save_to_disk();
    }

    fn save_to_disk(&self) {
        JsonStore::save(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: LibraryKind, id: &str) -> LibraryItem {
        LibraryItem {
            kind,
            id: id.to_string(),
            title: format!("Title {id}"),
            thumbnail: String::new(),
            provider: ProviderId::default(),
        }
    }

    #[test]
    fn save_is_idempotent() {
        let mut s = LibraryStore::default();
        s.add(item(LibraryKind::Album, "a1"));
        s.add(item(LibraryKind::Album, "a1"));
        assert_eq!(s.items.len(), 1);
    }

    #[test]
    fn save_keeps_distinct_kinds_same_id() {
        let mut s = LibraryStore::default();
        s.add(item(LibraryKind::Album, "x"));
        s.add(item(LibraryKind::Artist, "x"));
        assert_eq!(s.items.len(), 2);
    }

    #[test]
    fn remove_uses_kind_and_id() {
        let mut s = LibraryStore::default();
        s.add(item(LibraryKind::Album, "x"));
        s.add(item(LibraryKind::Artist, "x"));
        assert!(s.remove(LibraryKind::Album, "x"));
        assert!(!s.remove(LibraryKind::Album, "x"));
        assert_eq!(s.items.len(), 1);
    }

    #[test]
    fn insert_and_move_items() {
        let mut s = LibraryStore::default();
        s.add(item(LibraryKind::Artist, "a"));
        s.add(item(LibraryKind::Album, "b"));
        s.add(item(LibraryKind::Playlist, "c"));

        // Insert at an explicit position.
        s.insert(item(LibraryKind::Artist, "d"), 1);
        assert_eq!(s.items.len(), 4);
        assert_eq!(s.items[1].id, "d");

        // Duplicate insert is a no-op.
        s.insert(item(LibraryKind::Artist, "a"), 0);
        assert_eq!(s.items.len(), 4);

        // Move the last item up to position 1.
        s.move_item(3, 1);
        assert_eq!(s.items.len(), 4);
        assert_eq!(s.items[1].id, "c");
    }
}
