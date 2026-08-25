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

/// The data one [`ArtistDataKind`] resolves to — what a fetch actually
/// delivers, without wrapping it in a page.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtistKindData {
    Header(ArtistHeader),
    Popular(Vec<Track>),
    Albums(Vec<ArtistAlbumCard>),
    Playlists(Vec<CardData>),
    Related(Vec<RelatedArtistCard>),
}

impl ArtistKindData {
    /// View as section content (for rendering a section directly).
    pub fn to_section_content(&self) -> SectionContent {
        match self {
            Self::Popular(tracks) => SectionContent::Tracks(tracks.clone()),
            Self::Albums(albums) => SectionContent::Albums(albums.clone()),
            Self::Playlists(playlists) => SectionContent::Playlists(playlists.clone()),
            Self::Related(related) => SectionContent::Related(related.clone()),
            Self::Header(_) => SectionContent::default(),
        }
    }

    /// Store into an [`ArtistPage`] under `kind`.
    fn store(&self, page: &mut ArtistPage) {
        match self {
            Self::Header(_) => {}
            Self::Popular(v) => page.popular.clone_from(v),
            Self::Albums(v) => page.albums.clone_from(v),
            Self::Playlists(v) => page.playlists.clone_from(v),
            Self::Related(v) => page.related.clone_from(v),
        }
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

    /// Merge one fetched kind's data and record it (the kind was actually
    /// requested, so an empty list is a valid result). Header merges
    /// additively instead of overwriting.
    pub fn merge_kind(&mut self, kind: ArtistDataKind, data: &ArtistKindData) {
        if let ArtistKindData::Header(header) = data {
            if let Some(existing) = &mut self.page.header {
                existing.merge_stats_from(header);
                if existing.image.is_empty() {
                    existing.image.clone_from(&header.image);
                }
                if existing.description.is_empty() {
                    existing.description.clone_from(&header.description);
                }
            } else {
                self.page.header = Some(header.clone());
            }
        } else {
            data.store(&mut self.page);
        }
        if !self.fetched.contains(&kind) {
            self.fetched.push(kind);
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
    /// `None` means a genuine cache miss (caller starts a load); `Some`
    /// carries the section's content, already stored as `Ready`.
    pub fn serve_cached_section(
        &mut self,
        kind: ArtistSectionKind,
        provider: ProviderId,
    ) -> Option<SectionContent> {
        let cached = self.pages.get(&provider)?;
        if !cached.covers(kind.data_kinds()) {
            return None;
        }
        let content = cached.page.content(kind.data_kind())?;
        let section = self.section_mut(kind);
        section.provider = Some(provider);
        section.state = crate::load_state::LoadState::Ready(content.clone());
        Some(content)
    }

    /// Fail the loading section for `data_kind` if `provider` owns it
    /// (already-loaded sections keep their content).
    pub fn fail_section(&mut self, provider: ProviderId, data_kind: ArtistDataKind, msg: &str) {
        for kind in ArtistSectionKind::ALL {
            let section = self.section_mut(kind);
            if section.provider == Some(provider)
                && kind.data_kind() == data_kind
                && section.state.is_loading()
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn yt_page(popular: bool) -> ArtistPage {
        ArtistPage {
            header: Some(ArtistHeader::default()),
            popular: if popular {
                vec![crate::types::Track::default()]
            } else {
                Vec::new()
            },
            albums: vec![ArtistAlbumCard::default()],
            playlists: vec![CardData::default()],
            related: vec![RelatedArtistCard::default()],
        }
    }

    #[test]
    fn switching_provider_back_serves_from_cache() {
        let mut state = ArtistPageState::new(ProviderId::YouTube, "yt1");

        // Open: YouTube answers everything.
        let page = yt_page(true);
        state.pages.entry(ProviderId::YouTube).or_default();
        for &kind in ArtistDataKind::ALL {
            let data = match kind {
                ArtistDataKind::Header => ArtistKindData::Header(page.header.clone().unwrap()),
                ArtistDataKind::Popular => ArtistKindData::Popular(page.popular.clone()),
                ArtistDataKind::Albums => ArtistKindData::Albums(page.albums.clone()),
                ArtistDataKind::Playlists => ArtistKindData::Playlists(page.playlists.clone()),
                ArtistDataKind::Related => ArtistKindData::Related(page.related.clone()),
            };
            state
                .pages
                .get_mut(&ProviderId::YouTube)
                .unwrap()
                .merge_kind(kind, &data);
            if kind != ArtistDataKind::Header {
                let sk = ArtistSectionKind::ALL
                    .into_iter()
                    .find(|k| k.data_kind() == kind)
                    .unwrap();
                state.section_mut(sk).state = LoadState::Ready(data.to_section_content());
            }
        }

        // Sanity: YouTube cache already covers Albums before any switching.
        assert!(state
            .serve_cached_section(ArtistSectionKind::Albums, ProviderId::YouTube)
            .is_some());

        // Switch Albums to SoundCloud, fetch arrives.
        assert!(state
            .serve_cached_section(ArtistSectionKind::Albums, ProviderId::SoundCloud)
            .is_none());
        state.start_section_load(ArtistSectionKind::Albums, ProviderId::SoundCloud);
        state
            .pages
            .entry(ProviderId::SoundCloud)
            .or_default()
            .merge_kind(
                ArtistDataKind::Albums,
                &ArtistKindData::Albums(vec![ArtistAlbumCard::default()]),
            );
        state.section_mut(ArtistSectionKind::Albums).state = LoadState::Ready(
            ArtistKindData::Albums(vec![ArtistAlbumCard::default()]).to_section_content(),
        );

        // Back to YouTube: must be a cache hit, no reload.
        assert!(
            state
                .serve_cached_section(ArtistSectionKind::Albums, ProviderId::YouTube)
                .is_some(),
            "YouTube albums should be served from cache"
        );
        // And SoundCloud again too.
        assert!(state
            .serve_cached_section(ArtistSectionKind::Albums, ProviderId::SoundCloud)
            .is_some());
    }
}
