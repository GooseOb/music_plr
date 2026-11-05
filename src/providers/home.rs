//! Home-feed sections: per-provider discovery rows for the Home view.
//!
//! Every provider contributes a small, independently-failing list of rows.
//! `fetch_home` runs one provider at a time (the caller spawns a thread per
//! provider), so a slow or broken source never blocks the local rows.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{CardData, ProviderId};
use crate::{
    data::{custom_providers::CustomProviderKind, library::LibraryKind},
    i18n::Strings,
    types::Track,
};

/// One kind of home row. The (provider, kind) pair identifies a row across
/// refreshes and in the on-disk cache.
///
/// The declaration order is the row order inside a provider's block, so
/// [`sort_sections`] can sort on the value itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HomeSectionKind {
    /// The pane's single track list (always the provider's own picks).
    QuickPicks,
    ForYou,
    NewReleases,
    Trending,
    TopArtists,
    RecentlyAdded,
    RecentlyPlayed,
    MostPlayed,
    Starred,
    /// A shuffled sample of the library (random albums).
    Discover,
    /// The library's artists as cards, so artist pages are reachable from Home.
    Artists,
}

impl HomeSectionKind {
    /// Stable key for the on-disk cache and the python bridge.
    pub fn key(self) -> &'static str {
        match self {
            HomeSectionKind::QuickPicks => "quick_picks",
            HomeSectionKind::NewReleases => "new_releases",
            HomeSectionKind::Trending => "trending",
            HomeSectionKind::TopArtists => "top_artists",
            HomeSectionKind::ForYou => "for_you",
            HomeSectionKind::RecentlyAdded => "recently_added",
            HomeSectionKind::RecentlyPlayed => "recently_played",
            HomeSectionKind::MostPlayed => "most_played",
            HomeSectionKind::Starred => "starred",
            HomeSectionKind::Discover => "discover",
            HomeSectionKind::Artists => "artists",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "quick_picks" => HomeSectionKind::QuickPicks,
            "new_releases" => HomeSectionKind::NewReleases,
            "trending" => HomeSectionKind::Trending,
            "top_artists" => HomeSectionKind::TopArtists,
            "for_you" => HomeSectionKind::ForYou,
            "recently_added" => HomeSectionKind::RecentlyAdded,
            "recently_played" => HomeSectionKind::RecentlyPlayed,
            "most_played" => HomeSectionKind::MostPlayed,
            "starred" => HomeSectionKind::Starred,
            "discover" => HomeSectionKind::Discover,
            "artists" => HomeSectionKind::Artists,
            _ => return None,
        })
    }

    /// The localized row title used when the provider supplies none.
    pub fn default_title(self, tr: &'static Strings) -> &'static str {
        match self {
            HomeSectionKind::QuickPicks => tr.quick_picks,
            HomeSectionKind::NewReleases => tr.new_releases,
            HomeSectionKind::Trending => tr.trending_now,
            HomeSectionKind::TopArtists => tr.top_artists,
            HomeSectionKind::ForYou => tr.for_you,
            HomeSectionKind::RecentlyAdded => tr.recently_added,
            HomeSectionKind::RecentlyPlayed => tr.recently_played,
            HomeSectionKind::MostPlayed => tr.most_played,
            HomeSectionKind::Starred => tr.starred,
            HomeSectionKind::Discover => tr.discover,
            HomeSectionKind::Artists => tr.artists,
        }
    }
}

/// Identity of a home row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HomeSectionKey {
    pub provider: ProviderId,
    pub kind: HomeSectionKind,
}

impl HomeSectionKey {
    pub fn new(provider: ProviderId, kind: HomeSectionKind) -> Self {
        Self { provider, kind }
    }

    /// Key in `home_cache.json`: the provider slug (which already disambiguates
    /// custom servers) plus the row kind.
    pub fn cache_key(self) -> String {
        format!("{}:{}", self.provider.slug(), self.kind.key())
    }
}

/// A navigable card on the home feed (artist, album or playlist).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeCard {
    pub kind: LibraryKind,
    pub provider: ProviderId,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: String,
    pub url: String,
}

impl HomeCard {
    pub fn new(kind: LibraryKind, provider: ProviderId, card: CardData) -> Self {
        Self {
            kind,
            provider,
            id: card.id,
            title: card.title,
            subtitle: card.subtitle,
            thumbnail: card.thumbnail,
            url: card.url,
        }
    }
}

impl From<&HomeCard> for crate::data::library::LibraryItem {
    fn from(card: &HomeCard) -> Self {
        Self {
            kind: card.kind,
            id: card.id.clone(),
            title: card.title.clone(),
            thumbnail: card.thumbnail.clone(),
            provider: card.provider,
            url: card.url.clone(),
        }
    }
}

/// What a home row renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HomeSectionData {
    Tracks(Vec<Track>),
    Cards(Vec<HomeCard>),
}

impl HomeSectionData {
    pub fn len(&self) -> usize {
        match self {
            HomeSectionData::Tracks(tracks) => tracks.len(),
            HomeSectionData::Cards(cards) => cards.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Seed every image this row shows into the thumbnail cache.
    pub fn seed_thumbnails(&self, index: &mut crate::data::thumbnails::ThumbnailIndex) {
        match self {
            HomeSectionData::Tracks(tracks) => {
                for track in tracks {
                    if !track.thumbnail().is_empty() {
                        index.ensure(track.source, track.primary_id(), track.thumbnail());
                    }
                }
            }
            HomeSectionData::Cards(cards) => {
                for card in cards {
                    index.ensure(card.provider, &card.id, &card.thumbnail);
                }
            }
        }
    }
}

/// A fetched row, before it is merged into a pane's home state.
#[derive(Debug, Clone, PartialEq)]
pub struct HomeSection {
    pub key: HomeSectionKey,
    pub title: String,
    pub data: HomeSectionData,
}

impl HomeSection {
    pub fn new(key: HomeSectionKey, title: String, data: HomeSectionData) -> Self {
        Self { key, title, data }
    }

    /// The title to draw: the provider's own shelf title when it supplied one
    /// (a shelf like "Take it easy" means nothing without knowing whose it
    /// is), else the localized kind title. The provider itself is named once
    /// per block by the view, not on every row.
    pub fn display_title(&self, tr: &'static Strings) -> String {
        let title = self.title.trim();
        if title.is_empty() {
            self.key.kind.default_title(tr).to_string()
        } else {
            title.to_string()
        }
    }
}

/// A row label with its source appended, for a row that renders outside its
/// provider's block (the pane's quick picks).
pub fn titled(tr: &'static Strings, label: &str, provider: ProviderId) -> String {
    format!("{} · {}", label, tr.provider(provider))
}

/// Providers that contribute home rows, in display order: the user's own
/// servers first (their library is what they came for), then the configured
/// default provider, then the remaining discovery sources.
pub fn home_providers(default: ProviderId) -> Vec<ProviderId> {
    let enabled: Vec<ProviderId> = ProviderId::all()
        .into_iter()
        .filter(|provider| home_capable(*provider))
        .collect();
    let mut providers: Vec<ProviderId> = enabled
        .iter()
        .copied()
        .filter(|provider| provider.is_custom())
        .collect();
    if enabled.contains(&default) && !providers.contains(&default) {
        providers.push(default);
    }
    let sources: Vec<ProviderId> = enabled
        .into_iter()
        .filter(|provider| !provider.is_custom() && !providers.contains(provider))
        .collect();
    providers.extend(sources);
    providers
}

/// Whether `provider` can serve a home feed right now.
fn home_capable(provider: ProviderId) -> bool {
    if !provider.enabled() {
        return false;
    }
    match provider {
        ProviderId::YouTube => crate::deps::availability().ytmusicapi,
        ProviderId::SoundCloud | ProviderId::Bandcamp | ProviderId::LastFm => true,
        ProviderId::MusicBrainz | ProviderId::Local => false,
        ProviderId::Custom(_, _) => provider.capabilities().search,
    }
}

/// Sort fetched rows into display order: provider block first, then the
/// provider's own row order, keeping quick picks out of the row list.
pub fn sort_sections(sections: &mut [HomeSection], providers: &[ProviderId]) {
    sections.sort_by_key(|section| {
        let provider_rank = providers
            .iter()
            .position(|&p| p == section.key.provider)
            .unwrap_or(usize::MAX);
        (provider_rank, section.key.kind)
    });
}

/// Row kinds `provider` may contribute (used for cache lookups and staleness).
pub fn planned_kinds(provider: ProviderId) -> &'static [HomeSectionKind] {
    use HomeSectionKind::{
        Artists, Discover, ForYou, MostPlayed, NewReleases, QuickPicks, RecentlyAdded,
        RecentlyPlayed, Starred, TopArtists, Trending,
    };
    match provider {
        ProviderId::YouTube => &[QuickPicks, ForYou, NewReleases, Trending],
        ProviderId::SoundCloud => &[Trending],
        ProviderId::Bandcamp => &[NewReleases],
        ProviderId::LastFm => &[TopArtists],
        ProviderId::Custom(_, _) => &[
            RecentlyAdded,
            RecentlyPlayed,
            MostPlayed,
            Starred,
            Discover,
            Artists,
        ],
        ProviderId::MusicBrainz | ProviderId::Local => &[],
    }
}

/// Fetch one provider's home rows. The caller runs this on a worker thread.
pub fn fetch_home(provider: ProviderId) -> Result<Vec<HomeSection>> {
    match provider {
        ProviderId::YouTube => super::youtube::home_sections(),
        ProviderId::SoundCloud => super::soundcloud::home_sections(),
        ProviderId::Bandcamp => super::bandcamp::home_sections(),
        ProviderId::LastFm => super::lastfm::home_sections(),
        ProviderId::Custom(CustomProviderKind::Subsonic, _) => {
            super::subsonic::home_sections(provider)
        }
        ProviderId::Custom(CustomProviderKind::Jellyfin, _) => {
            super::jellyfin::home_sections(provider)
        }
        ProviderId::MusicBrainz | ProviderId::Local => Ok(Vec::new()),
    }
}

/// Finish a provider block: wrap `(kind, data)` pairs into [`HomeSection`]s
/// with the provider's identity and no provider-specific title.
pub fn provider_sections(
    provider: ProviderId,
    rows: impl IntoIterator<Item = (HomeSectionKind, HomeSectionData)>,
) -> Vec<HomeSection> {
    rows.into_iter()
        .filter(|(_, data)| !data.is_empty())
        .map(|(kind, data)| {
            HomeSection::new(HomeSectionKey::new(provider, kind), String::new(), data)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str) -> HomeCard {
        HomeCard {
            kind: LibraryKind::Album,
            provider: ProviderId::Bandcamp,
            id: id.to_string(),
            title: id.to_string(),
            subtitle: String::new(),
            thumbnail: String::new(),
            url: String::new(),
        }
    }

    fn section(provider: ProviderId, kind: HomeSectionKind, title: &str) -> HomeSection {
        HomeSection::new(
            HomeSectionKey::new(provider, kind),
            title.to_string(),
            HomeSectionData::Cards(vec![card("a")]),
        )
    }

    #[test]
    fn provider_sections_drop_empty_rows() {
        let rows = provider_sections(
            ProviderId::Bandcamp,
            [
                (
                    HomeSectionKind::NewReleases,
                    HomeSectionData::Cards(vec![card("a")]),
                ),
                (
                    HomeSectionKind::Trending,
                    HomeSectionData::Cards(Vec::new()),
                ),
            ],
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key.kind, HomeSectionKind::NewReleases);
        assert_eq!(rows[0].key.provider, ProviderId::Bandcamp);
    }

    #[test]
    fn sections_sort_by_provider_block_then_kind() {
        let providers = [ProviderId::YouTube, ProviderId::Bandcamp];
        let mut sections = vec![
            section(ProviderId::Bandcamp, HomeSectionKind::NewReleases, "bc"),
            section(ProviderId::YouTube, HomeSectionKind::Trending, "yt trend"),
            section(ProviderId::YouTube, HomeSectionKind::NewReleases, "yt new"),
        ];
        sort_sections(&mut sections, &providers);
        let keys: Vec<_> = sections
            .iter()
            .map(|s| (s.key.provider, s.key.kind))
            .collect();
        assert_eq!(
            keys,
            vec![
                (ProviderId::YouTube, HomeSectionKind::NewReleases),
                (ProviderId::YouTube, HomeSectionKind::Trending),
                (ProviderId::Bandcamp, HomeSectionKind::NewReleases),
            ]
        );
    }

    #[test]
    fn row_labels_keep_the_providers_own_title() {
        let strings = crate::i18n::Language::En.strings();
        assert_eq!(
            section(
                ProviderId::YouTube,
                HomeSectionKind::Trending,
                "Trending songs for you"
            )
            .display_title(strings),
            "Trending songs for you"
        );
        assert_eq!(
            section(ProviderId::Bandcamp, HomeSectionKind::NewReleases, " ").display_title(strings),
            strings.new_releases
        );
    }

    #[test]
    fn quick_picks_label_names_its_provider() {
        let strings = crate::i18n::Language::En.strings();
        assert_eq!(
            titled(strings, strings.quick_picks, ProviderId::YouTube),
            "Quick picks · YouTube"
        );
    }

    #[test]
    fn custom_servers_plan_discovery_rows() {
        let provider = ProviderId::Custom(CustomProviderKind::Subsonic, 1);
        let kinds = planned_kinds(provider);
        for kind in [
            HomeSectionKind::RecentlyAdded,
            HomeSectionKind::Discover,
            HomeSectionKind::Artists,
        ] {
            assert!(kinds.contains(&kind), "{kind:?} missing");
        }
    }

    #[test]
    fn new_row_keys_round_trip_through_the_cache() {
        for kind in [HomeSectionKind::Discover, HomeSectionKind::Artists] {
            assert_eq!(HomeSectionKind::from_key(kind.key()), Some(kind));
        }
    }

    #[test]
    fn cache_keys_are_provider_and_kind() {
        assert_eq!(
            HomeSectionKey::new(ProviderId::YouTube, HomeSectionKind::QuickPicks).cache_key(),
            "youtube:quick_picks"
        );
    }
}
