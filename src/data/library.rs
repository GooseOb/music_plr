use crate::app::Message;

use super::{JsonStore, StoreLocation};
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
/// kind, so a saved item can be re-opened via the same `Open*` messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryItem {
    pub kind: LibraryKind,
    /// `browse_id` for artists/albums, `playlist_id` for playlists.
    pub id: String,
    pub title: String,
    pub thumbnail: String,
}

impl LibraryItem {
    pub fn open_message(&self) -> Message {
        match self.kind {
            LibraryKind::Album => Message::OpenAlbum(self.id.clone(), self.title.clone()),
            LibraryKind::Artist => Message::OpenArtist(self.id.clone(), self.title.clone()),
            LibraryKind::Playlist => Message::OpenPlaylist(self.id.clone(), self.title.clone()),
        }
    }
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
}
