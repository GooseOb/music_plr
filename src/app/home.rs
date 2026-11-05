//! Home view state: the provider-supplied discovery rows of a pane.
//!
//! Local rows (recently played, playlists, library, downloads) are rendered
//! straight from their stores by the view, so they are always fresh and never
//! live here. What this type holds is the fetched remote half: the rows per
//! provider, plus which refresh produced them.

use serde::{Deserialize, Serialize};

use crate::providers::{HomeSection, HomeSectionKey, ProviderId};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HomeData {
    /// The fetched rows, one list per provider. Not serialized: `home_cache.json`
    /// is their on-disk home, and a restored pane repaints from it through
    /// `start_home`, so `session.json` (written on every volume step and seek)
    /// stays small.
    #[serde(skip)]
    pub sections: Vec<HomeSection>,
    /// How many provider fetches of the running refresh are still in flight.
    /// The feed shows its spinner until every source has answered (or failed),
    /// so one fast provider cannot make the page look complete while the rest
    /// are still coming.
    #[serde(skip)]
    pub pending: usize,
    /// Which provider supplied the quick-pick list: its tracks live in the
    /// view's `content`, so the section itself never reaches `sections` and
    /// the view needs this to label the row.
    #[serde(skip)]
    pub quick_picks_provider: Option<ProviderId>,
}

impl HomeData {
    /// Swap in one provider's freshly fetched rows, leaving every other
    /// provider's rows untouched.
    pub fn replace_provider(&mut self, provider: ProviderId, rows: Vec<HomeSection>) {
        self.sections.retain(|s| s.key.provider != provider);
        self.sections.extend(rows);
    }

    pub fn section(&self, key: HomeSectionKey) -> Option<&HomeSection> {
        self.sections.iter().find(|s| s.key == key)
    }

    pub fn is_loading(&self) -> bool {
        self.pending > 0
    }
}

/// Where a pressed home track comes from. Home tiles are not part of the
/// pane's track list, so they carry their own source instead of a `TrackPos`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeTrackSource {
    Recent,
    Downloads,
    /// The rail this tile belongs to, keyed like every other rail identity so
    /// a refresh between the click and the handler cannot play the wrong row.
    Section(crate::providers::HomeSectionKey),
}

/// Identity of one horizontal rail on the Home page. Sections are keyed by
/// provider and kind rather than by their position, so a refresh that reorders
/// or replaces rows keeps each rail's scroll offset with the rail it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HomeRailId {
    Recent,
    Playlists,
    Library,
    Downloads,
    /// A custom server's playlist index, inside that provider's block.
    ServerPlaylists(ProviderId),
    Section(HomeSectionKey),
}

impl HomeRailId {
    /// Stable fragment of the rail's scrollable id (see `ui::card_row_id`).
    pub fn slug(self) -> String {
        match self {
            HomeRailId::Recent => "recent".to_string(),
            HomeRailId::Playlists => "playlists".to_string(),
            HomeRailId::Library => "library".to_string(),
            HomeRailId::Downloads => "downloads".to_string(),
            HomeRailId::ServerPlaylists(provider) => {
                format!("server-playlists:{}", provider.slug())
            }
            HomeRailId::Section(key) => {
                format!("section:{}:{}", key.provider.slug(), key.kind.key())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::library::LibraryKind,
        providers::{HomeCard, HomeSectionData, HomeSectionKind},
    };

    fn section(provider: ProviderId, kind: HomeSectionKind) -> HomeSection {
        HomeSection::new(
            HomeSectionKey::new(provider, kind),
            String::new(),
            HomeSectionData::Cards(vec![HomeCard {
                kind: LibraryKind::Album,
                provider,
                id: "1".to_string(),
                title: "Album".to_string(),
                subtitle: String::new(),
                thumbnail: String::new(),
                url: String::new(),
            }]),
        )
    }

    #[test]
    fn replacing_a_provider_keeps_the_other_rows() {
        let mut home = HomeData {
            sections: vec![
                section(ProviderId::YouTube, HomeSectionKind::NewReleases),
                section(ProviderId::Bandcamp, HomeSectionKind::NewReleases),
            ],
            ..Default::default()
        };
        home.replace_provider(
            ProviderId::YouTube,
            vec![section(ProviderId::YouTube, HomeSectionKind::Trending)],
        );
        let providers: Vec<_> = home.sections.iter().map(|s| s.key.provider).collect();
        assert_eq!(providers, vec![ProviderId::Bandcamp, ProviderId::YouTube]);
        assert!(home
            .section(HomeSectionKey::new(
                ProviderId::YouTube,
                HomeSectionKind::NewReleases
            ))
            .is_none());
        assert!(!home.sections.is_empty());
    }

    #[test]
    fn replacing_with_nothing_drops_only_that_provider() {
        let mut home = HomeData {
            sections: vec![
                section(ProviderId::YouTube, HomeSectionKind::Trending),
                section(ProviderId::Bandcamp, HomeSectionKind::NewReleases),
            ],
            ..Default::default()
        };
        home.replace_provider(ProviderId::YouTube, Vec::new());
        assert_eq!(home.sections.len(), 1);
        assert_eq!(home.sections[0].key.provider, ProviderId::Bandcamp);
    }
}
