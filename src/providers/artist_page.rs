//! Provider-agnostic artist page model. Each provider backend fills the
//! sections it can (`YouTube` and `SoundCloud` fill everything,
//! `MusicBrainz` only the header and albums); the view renders whatever
//! arrived. The whole [`ArtistPageState`] lives inside
//! `ViewKind::Artist`, so selected providers and loaded content survive
//! navigation.

use super::CardData;
use crate::{providers::ProviderId, types::Track};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Header block: picture, name-line stats ("Monthly listeners: 280M" etc.)
/// and an optional bio.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtistHeader {
    pub image: String,
    pub stats: Vec<(String, String)>,
    pub description: String,
}

/// One entry in the Albums row (albums, EPs and singles merged; `badge`
/// carries "Single"/"EP" when known).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtistAlbumCard {
    pub id: String,
    pub title: String,
    pub date: String,
    pub badge: String,
    pub thumbnail: String,
}

/// One entry in the "Fans also like" row.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelatedArtistCard {
    pub id: String,
    pub name: String,
    pub stat: String,
    pub thumbnail: String,
}

/// Everything a provider can serve for one artist page load.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtistPage {
    pub header: Option<ArtistHeader>,
    pub popular: Vec<Track>,
    pub albums: Vec<ArtistAlbumCard>,
    pub playlists: Vec<CardData>,
    pub related: Vec<RelatedArtistCard>,
}

/// A page section: which provider currently backs it, whether a fetch is in
/// flight, and its content.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtistSection<T> {
    pub provider: Option<ProviderId>,
    pub loading: bool,
    pub content: T,
}

/// One fetchable piece of an artist page. Backends only request the data
/// for the kinds they're asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtistDataKind {
    Header,
    Popular,
    Albums,
    Playlists,
    Related,
}

impl ArtistDataKind {
    pub const ALL: &'static [Self] = &[
        Self::Header,
        Self::Popular,
        Self::Albums,
        Self::Playlists,
        Self::Related,
    ];

    pub fn wanted(self, kinds: &[Self]) -> bool {
        kinds.contains(&self)
    }
}

/// A provider page plus which kinds it already contains — the unit of the
/// per-provider cache. Sections absent from `fetched` still need fetching.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CachedArtistPage {
    pub page: ArtistPage,
    pub fetched: Vec<ArtistDataKind>,
}

impl CachedArtistPage {
    /// Whether every kind in `kinds` has been fetched into this entry.
    pub fn covers(&self, kinds: &[ArtistDataKind]) -> bool {
        kinds.iter().all(|k| self.fetched.contains(k))
    }

    /// Merge a fresh partial fetch: overwrite each part that is non-empty in
    /// the response (empty means "not requested"), and record its kinds.
    pub fn merge(&mut self, kinds: &[ArtistDataKind], page: ArtistPage) {
        if !page.popular.is_empty() {
            self.page.popular = page.popular;
        }
        if !page.albums.is_empty() {
            self.page.albums = page.albums;
        }
        if !page.playlists.is_empty() {
            self.page.playlists = page.playlists;
        }
        if !page.related.is_empty() {
            self.page.related = page.related;
        }
        if let Some(header) = page.header {
            if let Some(existing) = &mut self.page.header {
                for stat in header.stats {
                    if !existing.stats.iter().any(|(label, _)| label == &stat.0) {
                        existing.stats.push(stat);
                    }
                }
                if existing.image.is_empty() {
                    existing.image = header.image;
                }
                if existing.description.is_empty() {
                    existing.description = header.description;
                }
            } else {
                self.page.header = Some(header);
            }
        }
        for kind in kinds {
            if !self.fetched.contains(kind) {
                self.fetched.push(*kind);
            }
        }
    }
}

/// Persisted per-artist-page state stored in `ViewKind::Artist`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtistPageState {
    /// Known per-provider artist ids (the source id arrives pre-filled;
    /// others are resolved lazily by name).
    pub provider_ids: BTreeMap<ProviderId, String>,
    /// Pages already fetched per provider, so switching a section's provider
    /// back and forth is instant and request-free.
    pub pages: BTreeMap<ProviderId, CachedArtistPage>,
    pub header_provider: Option<ProviderId>,
    pub header: Option<ArtistHeader>,
    pub popular: ArtistSection<Vec<Track>>,
    pub albums: ArtistSection<Vec<ArtistAlbumCard>>,
    pub playlists: ArtistSection<Vec<CardData>>,
    pub related: ArtistSection<Vec<RelatedArtistCard>>,
}

impl ArtistPageState {
    /// Seed the source provider's id and default every section to it.
    pub fn new(source: ProviderId, id: &str) -> Self {
        let mut state = Self::default();
        state.provider_ids.insert(source, id.to_string());
        // The source provider owns the header until explicitly switched;
        // without this, whichever companion answers first would claim it.
        state.header_provider = Some(source);
        state.popular.provider = Some(source);
        state.albums.provider = Some(source);
        state.playlists.provider = Some(source);
        state.related.provider = Some(source);
        // The source provider's page fetch starts immediately; show loading
        // indicators rather than empty states until it lands.
        state.popular.loading = true;
        state.albums.loading = true;
        state.playlists.loading = true;
        state.related.loading = true;
        state
    }
}

/// Identifies one section of the artist page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistSectionKind {
    Popular,
    Albums,
    Playlists,
    Related,
}

impl ArtistSectionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Popular => "Most popular songs",
            Self::Albums => "Albums",
            Self::Playlists => "Playlists",
            Self::Related => "Fans also like",
        }
    }

    /// Providers that can serve this section, in preference order.
    pub fn providers(self) -> &'static [ProviderId] {
        match self {
            Self::Albums => &[
                ProviderId::YouTube,
                ProviderId::SoundCloud,
                ProviderId::MusicBrainz,
            ],
            _ => &[ProviderId::YouTube, ProviderId::SoundCloud],
        }
    }
}
