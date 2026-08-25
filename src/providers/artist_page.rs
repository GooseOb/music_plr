//! Provider-agnostic artist page model. Each provider backend fills the
//! sections it can (`YouTube` and `SoundCloud` fill everything,
//! `MusicBrainz` only the header and albums); the view renders whatever
//! arrived. The whole [`ArtistPageState`] lives inside
//! `ViewKind::Artist`, so selected providers and loaded content survive
//! navigation.

use super::CardData;
use crate::load_state::LoadState;
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

impl ArtistHeader {
    /// Append `incoming`'s stats whose label isn't already present.
    fn merge_stats_from(&mut self, incoming: &Self) {
        for stat in &incoming.stats {
            if !self.stats.iter().any(|(label, _)| label == &stat.0) {
                self.stats.push(stat.clone());
            }
        }
    }
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

/// A page section: which provider currently backs it and its load state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtistSection {
    pub provider: Option<ProviderId>,
    pub state: crate::load_state::LoadState<SectionContent>,
}

/// Per-section payload. All four sections share one representation so the
/// rest of the code can iterate over them instead of matching on named
/// fields everywhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SectionContent {
    Tracks(Vec<Track>),
    Albums(Vec<ArtistAlbumCard>),
    Playlists(Vec<CardData>),
    Related(Vec<RelatedArtistCard>),
}

impl Default for SectionContent {
    fn default() -> Self {
        Self::Tracks(Vec::new())
    }
}

impl SectionContent {
    fn as_tracks(&self) -> Option<&Vec<Track>> {
        match self {
            Self::Tracks(tracks) => Some(tracks),
            _ => None,
        }
    }
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
                existing.merge_stats_from(&header);
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtistPageState {
    /// Known per-provider artist ids (the source id arrives pre-filled;
    /// others are resolved lazily by name).
    pub provider_ids: BTreeMap<ProviderId, String>,
    /// Pages already fetched per provider, so switching a section's provider
    /// back and forth is instant and request-free.
    pub pages: BTreeMap<ProviderId, CachedArtistPage>,
    pub header_provider: Option<ProviderId>,
    pub header: Option<ArtistHeader>,
    /// Indexed by `ArtistSectionKind` (see [`Self::section`]).
    pub sections: [ArtistSection; 4],
}

impl Default for ArtistPageState {
    fn default() -> Self {
        Self {
            sections: std::array::from_fn(|_| ArtistSection::default()),
            provider_ids: BTreeMap::new(),
            pages: BTreeMap::new(),
            header_provider: None,
            header: None,
        }
    }
}

impl ArtistPageState {
    /// Seed the source provider's id and default every section to it.
    pub fn new(source: ProviderId, id: &str) -> Self {
        let mut state = Self::loading_for(source);
        state.provider_ids.insert(source, id.to_string());
        state
    }

    /// Like [`ArtistPageState::new`] but without a known source id; the id
    /// is resolved by artist name when the page loads.
    pub fn loading_for(source: ProviderId) -> Self {
        let mut state = Self::default();
        // The source provider owns the header until explicitly switched;
        // without this, whichever companion answers first would claim it.
        // Its page fetch starts immediately, so show loading indicators
        // rather than empty states until it lands.
        for kind in ArtistSectionKind::ALL {
            state.start_section_load(kind, source);
        }
        state.header_provider = Some(source);
        state
    }

    pub fn section(&self, kind: ArtistSectionKind) -> &ArtistSection {
        &self.sections[kind as usize]
    }

    pub fn section_mut(&mut self, kind: ArtistSectionKind) -> &mut ArtistSection {
        &mut self.sections[kind as usize]
    }

    /// Point `kind` at `provider` and reset it to Loading.
    pub fn start_section_load(&mut self, kind: ArtistSectionKind, provider: ProviderId) {
        *self.section_mut(kind) = ArtistSection {
            provider: Some(provider),
            state: crate::load_state::LoadState::Loading,
        };
    }

    /// `(id, thumbnail)` of every non-empty card across the card sections.
    pub fn card_thumbs(&self) -> impl Iterator<Item = (&String, &String)> {
        self.sections.iter().flat_map(|s| match &s.state {
            LoadState::Ready(SectionContent::Albums(v)) => {
                v.iter().map(|c| (&c.id, &c.thumbnail)).collect()
            }
            LoadState::Ready(SectionContent::Playlists(v)) => {
                v.iter().map(|c| (&c.id, &c.thumbnail)).collect()
            }
            LoadState::Ready(SectionContent::Related(v)) => {
                v.iter().map(|r| (&r.id, &r.thumbnail)).collect()
            }
            _ => Vec::new(),
        })
    }

    /// Serve `kind` from `provider`'s cache when it covers the section.
    /// Returns the popular tracks when they were served (the view's track
    /// list mirrors that section).
    pub fn serve_cached_section(
        &mut self,
        kind: ArtistSectionKind,
        provider: ProviderId,
    ) -> Option<Vec<Track>> {
        let cached = self.pages.get(&provider)?;
        if !cached.covers(kind.data_kinds()) {
            return None;
        }
        let content = cached.page.content(kind.data_kind())?;
        let tracks = content.as_tracks().cloned();
        let section = self.section_mut(kind);
        section.provider = Some(provider);
        section.state = crate::load_state::LoadState::Ready(content);
        tracks
    }

    /// Mark each requested section owned by `provider` as ready with the
    /// fetched data. Returns the popular tracks when they were among the
    /// fetched kinds.
    pub fn apply_fetch(
        &mut self,
        kinds: &[ArtistDataKind],
        provider: ProviderId,
        fetched: &ArtistPage,
    ) -> Option<Vec<Track>> {
        let mut new_tracks = None;
        for kind in ArtistSectionKind::ALL {
            let data_kind = kind.data_kind();
            if !data_kind.wanted(kinds) || self.section(kind).provider != Some(provider) {
                continue;
            }
            if let Some(content) = fetched.content(data_kind) {
                new_tracks = content.as_tracks().cloned();
                self.section_mut(kind).state = crate::load_state::LoadState::Ready(content);
            }
        }
        new_tracks
    }

    /// Fail every loading section owned by `provider` (already-loaded
    /// sections keep their content).
    pub fn fail_sections(&mut self, provider: ProviderId, msg: &str) {
        for section in &mut self.sections {
            if section.provider == Some(provider) && section.state.is_loading() {
                section.state = crate::load_state::LoadState::Failed(msg.to_string());
            }
        }
    }

    /// Apply `provider`'s header onto the page: an unowned header is taken
    /// as-is (blanked unless `provider` explicitly owns the header block),
    /// while an existing one only gains new stats and refreshed image/bio
    /// from the owner.
    pub fn merge_header(&mut self, provider: ProviderId, incoming: Option<&ArtistHeader>) {
        let Some(incoming) = incoming else {
            return;
        };
        let explicit = self.header_provider == Some(provider);
        match &mut self.header {
            None => {
                let mut header = incoming.clone();
                if !explicit {
                    header.image.clear();
                    header.description.clear();
                }
                self.header = Some(header);
            }
            Some(existing) => {
                existing.merge_stats_from(incoming);
                if explicit {
                    existing.image.clone_from(&incoming.image);
                    existing.description.clone_from(&incoming.description);
                }
            }
        }
    }
}

impl ArtistPage {
    /// Clone out the section content for `kind` (`None` for `Header`, which
    /// has no section).
    pub fn content(&self, kind: ArtistDataKind) -> Option<SectionContent> {
        match kind {
            ArtistDataKind::Popular => Some(SectionContent::Tracks(self.popular.clone())),
            ArtistDataKind::Albums => Some(SectionContent::Albums(self.albums.clone())),
            ArtistDataKind::Playlists => Some(SectionContent::Playlists(self.playlists.clone())),
            ArtistDataKind::Related => Some(SectionContent::Related(self.related.clone())),
            ArtistDataKind::Header => None,
        }
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
    pub const ALL: [Self; 4] = [Self::Popular, Self::Albums, Self::Playlists, Self::Related];

    pub fn label(self) -> &'static str {
        match self {
            Self::Popular => "Most popular songs",
            Self::Albums => "Albums",
            Self::Playlists => "Playlists",
            Self::Related => "Fans also like",
        }
    }

    pub fn data_kind(self) -> ArtistDataKind {
        match self {
            Self::Popular => ArtistDataKind::Popular,
            Self::Albums => ArtistDataKind::Albums,
            Self::Playlists => ArtistDataKind::Playlists,
            Self::Related => ArtistDataKind::Related,
        }
    }

    pub fn data_kinds(self) -> &'static [ArtistDataKind] {
        match self {
            Self::Popular => &[ArtistDataKind::Popular],
            Self::Albums => &[ArtistDataKind::Albums],
            Self::Playlists => &[ArtistDataKind::Playlists],
            Self::Related => &[ArtistDataKind::Related],
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
