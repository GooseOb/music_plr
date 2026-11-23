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
    S3,
}

impl CustomProviderKind {
    pub const ALL: &'static [Self] = &[Self::Subsonic, Self::Jellyfin, Self::S3];

    /// Untranslated backend name; the API type is a brand, like provider names.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Subsonic => "Subsonic",
            Self::Jellyfin => "Jellyfin",
            Self::S3 => "S3",
        }
    }

    /// Lowercase key used in provider slugs, cache keys and saved files.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Subsonic => "subsonic",
            Self::Jellyfin => "jellyfin",
            Self::S3 => "s3",
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

/// A user-added provider and the configuration its backend needs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomProvider {
    pub id: u64,
    pub name: String,
    #[serde(flatten)]
    pub connection: CustomProviderConnection,
}

/// Per-backend connection settings, tagged by `kind` in saved files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum CustomProviderConnection {
    Subsonic(HttpConnection),
    Jellyfin(HttpConnection),
    S3(S3Settings),
}

impl Default for CustomProviderConnection {
    fn default() -> Self {
        Self::Subsonic(HttpConnection::default())
    }
}

impl CustomProvider {
    pub fn kind(&self) -> CustomProviderKind {
        match &self.connection {
            CustomProviderConnection::Subsonic(_) => CustomProviderKind::Subsonic,
            CustomProviderConnection::Jellyfin(_) => CustomProviderKind::Jellyfin,
            CustomProviderConnection::S3(_) => CustomProviderKind::S3,
        }
    }

    pub fn http(&self) -> Option<&HttpConnection> {
        match &self.connection {
            CustomProviderConnection::Subsonic(http) | CustomProviderConnection::Jellyfin(http) => {
                Some(http)
            }
            CustomProviderConnection::S3(_) => None,
        }
    }

    pub fn http_mut(&mut self) -> Option<&mut HttpConnection> {
        match &mut self.connection {
            CustomProviderConnection::Subsonic(http) | CustomProviderConnection::Jellyfin(http) => {
                Some(http)
            }
            CustomProviderConnection::S3(_) => None,
        }
    }

    pub fn s3(&self) -> Option<&S3Settings> {
        match &self.connection {
            CustomProviderConnection::S3(settings) => Some(settings),
            CustomProviderConnection::Subsonic(_) | CustomProviderConnection::Jellyfin(_) => None,
        }
    }

    pub fn s3_mut(&mut self) -> Option<&mut S3Settings> {
        match &mut self.connection {
            CustomProviderConnection::S3(settings) => Some(settings),
            CustomProviderConnection::Subsonic(_) | CustomProviderConnection::Jellyfin(_) => None,
        }
    }

    /// The address shown next to the provider in Settings.
    pub fn location(&self) -> &str {
        match &self.connection {
            CustomProviderConnection::Subsonic(http) | CustomProviderConnection::Jellyfin(http) => {
                &http.server_url
            }
            CustomProviderConnection::S3(settings) => &settings.bucket,
        }
    }
}

/// Subsonic and Jellyfin both authenticate with a server URL and basic
/// credentials.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpConnection {
    pub server_url: String,
    pub username: String,
    pub password: String,
}

/// An S3-compatible bucket. Access and secret keys may stay blank on a public
/// bucket.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct S3Settings {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub path_style: bool,
    pub public_bucket: bool,
}

/// Persisted custom-provider list. Ids are never reused: `next_id` survives
/// deletions so a stale track identity can never resolve to a new server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomProviderStore {
    pub next_id: u64,
    pub providers: Vec<CustomProvider>,
}

#[derive(Deserialize)]
struct CustomProviderStoreFile {
    next_id: u64,
    providers: Vec<serde_json::Value>,
}

impl CustomProviderStore {
    fn from_file(text: &str) -> Self {
        let Ok(file) = serde_json::from_str::<CustomProviderStoreFile>(text) else {
            return Self::default();
        };
        let mut store = Self {
            next_id: file.next_id,
            providers: file
                .providers
                .into_iter()
                .filter_map(|value| match serde_json::from_value(value) {
                    Ok(provider) => Some(provider),
                    Err(error) => {
                        tracing::warn!(%error, "skipping invalid custom provider");
                        None
                    }
                })
                .collect(),
        };
        store.after_load();
        store
    }
}

impl JsonStore for CustomProviderStore {
    const FILE: &'static str = "custom_providers.json";

    fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .map_or_else(Self::default, |text| Self::from_file(&text))
    }

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
    // The custom lists are cached with the disabled ones; a save or delete of
    // a server changes them as much as a Settings toggle does.
    crate::providers::refresh_provider_lists();
}

pub fn all_provider_ids() -> Vec<ProviderId> {
    read()
        .iter()
        .map(|p| ProviderId::Custom(p.kind(), p.id))
        .collect()
}

/// The registered servers, whole, so tests restoring the registry can put
/// back everything that was registered before them.
#[cfg(test)]
pub fn all() -> Vec<CustomProvider> {
    read().clone()
}

/// Serializes the tests that swap the process-wide registry, so one test's
/// restore cannot land in the middle of another's body.
#[cfg(test)]
fn registry_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Registers the two fabricated servers the app-layer tests address by id — a
/// Subsonic server (9) and a keyless S3 bucket (8) whose empty bucket keeps
/// the backend from ever reaching the network — for the guard's lifetime, then
/// puts the previous registry back.
#[cfg(test)]
pub(crate) fn test_registry() -> RegistryRestore {
    let lock = registry_guard();
    let remembered = all();
    set_registry(&[
        CustomProvider {
            id: 9,
            name: "Test".into(),
            connection: CustomProviderConnection::Subsonic(HttpConnection {
                server_url: "https://example.com".into(),
                ..Default::default()
            }),
        },
        CustomProvider {
            id: 8,
            name: "Bucket".into(),
            connection: CustomProviderConnection::S3(S3Settings::default()),
        },
    ]);
    RegistryRestore {
        _lock: lock,
        remembered,
    }
}

#[cfg(test)]
pub(crate) struct RegistryRestore {
    _lock: std::sync::MutexGuard<'static, ()>,
    remembered: Vec<CustomProvider>,
}

#[cfg(test)]
impl Drop for RegistryRestore {
    fn drop(&mut self) {
        set_registry(&self.remembered);
    }
}

pub fn contains(kind: CustomProviderKind, id: u64) -> bool {
    read().iter().any(|p| p.kind() == kind && p.id == id)
}

pub fn display_name(kind: CustomProviderKind, id: u64) -> Option<String> {
    read()
        .iter()
        .find(|p| p.kind() == kind && p.id == id)
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
                connection: CustomProviderConnection::Subsonic(HttpConnection {
                    server_url: "http://localhost:4533".into(),
                    username: "admin".into(),
                    password: "secret".into(),
                }),
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
            connection: CustomProviderConnection::Subsonic(HttpConnection {
                server_url: "http://localhost:4533".into(),
                username: "admin".into(),
                password: String::new(),
            }),
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
            connection: CustomProviderConnection::Subsonic(HttpConnection {
                server_url: "http://localhost:4533".into(),
                username: "admin".into(),
                password: String::new(),
            }),
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

    #[test]
    fn invalid_entries_do_not_hide_valid_providers() {
        let text = r#"{
          "next_id": 4,
          "providers": [
            {
              "id": 1,
              "name": "my navidrome",
              "kind": "Subsonic",
              "server_url": "http://localhost:4533",
              "username": "admin",
              "password": "subsonic",
              "bucket": "",
              "region": "",
              "path_style": false,
              "public_bucket": false
            },
            {
              "id": 3,
              "name": "my S3",
              "kind": "S3",
              "server_url": "http://127.0.0.1:9000",
              "username": "goosemusic",
              "password": "goosemusic-demo-secret",
              "bucket": "music-demo",
              "region": "us-east-1",
              "path_style": true,
              "public_bucket": false
            }
          ]
        }"#;
        let store = CustomProviderStore::from_file(text);
        assert_eq!(store.providers.len(), 1);
        assert_eq!(store.providers[0].name, "my navidrome");
        assert_eq!(store.providers[0].kind(), CustomProviderKind::Subsonic);
        assert_eq!(store.next_id, 4);
    }

    #[test]
    fn connections_keep_the_saved_shape_and_s3_has_no_http_credentials() {
        let http = CustomProvider {
            id: 1,
            name: "Home".into(),
            connection: CustomProviderConnection::Subsonic(HttpConnection {
                server_url: "http://localhost:4533".into(),
                username: "admin".into(),
                password: "secret".into(),
            }),
        };
        let json = serde_json::to_value(&http).unwrap();
        assert_eq!(json["kind"], "Subsonic");
        assert_eq!(json["server_url"], "http://localhost:4533");
        assert_eq!(json["username"], "admin");
        assert!(json.get("s3").is_none());
        assert!(json.get("access_key").is_none());

        let legacy: CustomProvider = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, http);

        let s3 = CustomProvider {
            id: 2,
            name: "Bucket".into(),
            connection: CustomProviderConnection::S3(S3Settings {
                endpoint: "https://s3.eu-west-1.amazonaws.com".into(),
                region: "eu-west-1".into(),
                bucket: "music".into(),
                access_key: "key".into(),
                secret_key: "secret".into(),
                path_style: true,
                public_bucket: false,
            }),
        };
        let json = serde_json::to_value(&s3).unwrap();
        assert_eq!(json["kind"], "S3");
        assert_eq!(json["bucket"], "music");
        assert_eq!(json["access_key"], "key");
        assert!(json.get("server_url").is_none());
        assert!(json.get("username").is_none());
        assert!(json.get("password").is_none());

        let round_trip: CustomProvider = serde_json::from_value(json).unwrap();
        assert_eq!(round_trip, s3);
    }
}
