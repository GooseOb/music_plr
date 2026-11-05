use std::{
    fmt,
    sync::{OnceLock, RwLock},
};

use serde::{Deserialize, Serialize};

use super::JsonStore;
use crate::providers::ProviderId;

/// Backend a custom provider talks to.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum CustomProviderKind {
    #[default]
    Subsonic,
    Jellyfin,
}

impl CustomProviderKind {
    pub const ALL: &'static [Self] = &[Self::Subsonic, Self::Jellyfin];

    /// Untranslated backend name; the API type is a brand, like provider names.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Subsonic => "Subsonic",
            Self::Jellyfin => "Jellyfin",
        }
    }

    /// Lowercase key used in provider slugs, cache keys and saved files.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Subsonic => "subsonic",
            Self::Jellyfin => "jellyfin",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.slug() == slug)
    }
}

impl fmt::Display for CustomProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A user-added provider and the credentials needed to reach it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomProvider {
    pub id: u64,
    pub name: String,
    pub kind: CustomProviderKind,
    pub server_url: String,
    pub username: String,
    pub password: String,
}

/// Persisted custom-provider list. Ids are never reused: `next_id` survives
/// deletions so a stale track identity can never resolve to a new server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomProviderStore {
    pub next_id: u64,
    pub providers: Vec<CustomProvider>,
}

impl JsonStore for CustomProviderStore {
    const FILE: &'static str = "custom_providers.json";

    fn after_load(&mut self) {
        self.next_id = self
            .next_id
            .max(self.providers.iter().map(|p| p.id + 1).max().unwrap_or(1));
    }
}

impl CustomProviderStore {
    /// A fresh id that is never reused, even after deletions.
    pub fn allocate_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn upsert(&mut self, provider: CustomProvider) {
        if let Some(slot) = self
            .providers
            .iter_mut()
            .find(|existing| existing.id == provider.id)
        {
            *slot = provider;
        } else {
            self.providers.push(provider);
        }
    }

    pub fn remove(&mut self, id: u64) -> Option<CustomProvider> {
        let index = self.providers.iter().position(|p| p.id == id)?;
        Some(self.providers.remove(index))
    }
}

static REGISTRY: OnceLock<RwLock<Vec<CustomProvider>>> = OnceLock::new();

fn registry() -> &'static RwLock<Vec<CustomProvider>> {
    REGISTRY.get_or_init(|| RwLock::new(Vec::new()))
}

fn read() -> std::sync::RwLockReadGuard<'static, Vec<CustomProvider>> {
    registry()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write() -> std::sync::RwLockWriteGuard<'static, Vec<CustomProvider>> {
    registry()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Replace the process-wide registry. Provider dispatch is made of free
/// functions without access to [`MusicPlayer`](crate::app::MusicPlayer), so the
/// current credentials live here and are synced whenever the store changes.
pub fn set_registry(providers: &[CustomProvider]) {
    *write() = providers.to_vec();
}

pub fn all_provider_ids() -> Vec<ProviderId> {
    read()
        .iter()
        .map(|p| ProviderId::Custom(p.kind, p.id))
        .collect()
}

pub fn contains(kind: CustomProviderKind, id: u64) -> bool {
    read().iter().any(|p| p.kind == kind && p.id == id)
}

pub fn display_name(kind: CustomProviderKind, id: u64) -> Option<String> {
    read()
        .iter()
        .find(|p| p.kind == kind && p.id == id)
        .map(|p| p.name.clone())
}

pub fn get(id: u64) -> Option<CustomProvider> {
    read().iter().find(|p| p.id == id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_id_follows_the_highest_stored_id() {
        let mut store = CustomProviderStore {
            next_id: 1,
            providers: vec![CustomProvider {
                id: 7,
                name: "Home".into(),
                kind: CustomProviderKind::Subsonic,
                server_url: "http://localhost:4533".into(),
                username: "admin".into(),
                password: "secret".into(),
            }],
        };
        store.after_load();
        assert_eq!(store.next_id, 8);
    }

    #[test]
    fn empty_store_starts_ids_at_one() {
        let mut store = CustomProviderStore::default();
        store.after_load();
        assert_eq!(store.next_id, 1);
    }

    #[test]
    fn allocated_ids_are_never_reused_after_a_delete() {
        let mut store = CustomProviderStore::default();
        store.after_load();
        let first = store.allocate_id();
        let second = store.allocate_id();
        assert_eq!((first, second), (1, 2));

        store.upsert(CustomProvider {
            id: first,
            name: "Home".into(),
            kind: CustomProviderKind::Subsonic,
            server_url: "http://localhost:4533".into(),
            username: "admin".into(),
            password: String::new(),
        });
        store.remove(first);
        assert_eq!(store.allocate_id(), 3);
    }

    #[test]
    fn upsert_replaces_an_existing_provider_id() {
        let mut store = CustomProviderStore::default();
        let provider = CustomProvider {
            id: 4,
            name: "Home".into(),
            kind: CustomProviderKind::Subsonic,
            server_url: "http://localhost:4533".into(),
            username: "admin".into(),
            password: String::new(),
        };
        store.upsert(provider.clone());
        let mut edited = provider;
        edited.name = "Studio".into();
        store.upsert(edited);
        assert_eq!(store.providers.len(), 1);
        assert_eq!(store.providers[0].name, "Studio");
    }

    #[test]
    fn kind_slugs_round_trip() {
        for &kind in CustomProviderKind::ALL {
            assert_eq!(CustomProviderKind::from_slug(kind.slug()), Some(kind));
        }
    }
}
