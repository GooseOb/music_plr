//! Cached home-feed rows.
//!
//! The feed paints from here instantly on open (and offline), then refreshes
//! in the background. Rows are keyed by provider + kind; a provider's rows all
//! refresh together.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
use crate::providers::{HomeSectionData, HomeSectionKey};

/// How long a cached row may be served before a background refresh is due.
pub const HOME_CACHE_TTL_SECS: u64 = 6 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedHomeSection {
    pub fetched_at: u64,
    pub title: String,
    pub data: HomeSectionData,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HomeCache {
    pub sections: HashMap<String, CachedHomeSection>,
}

impl JsonStore for HomeCache {
    const FILE: &'static str = "home_cache.json";
    const LOCATION: StoreLocation = StoreLocation::Cache;
}

impl HomeCache {
    pub fn insert(
        &mut self,
        key: HomeSectionKey,
        title: String,
        data: HomeSectionData,
        fetched_at: u64,
    ) {
        self.sections.insert(
            key.cache_key(),
            CachedHomeSection {
                fetched_at,
                title,
                data,
            },
        );
    }

    pub fn section(&self, key: HomeSectionKey) -> Option<&CachedHomeSection> {
        self.sections.get(&key.cache_key())
    }

    /// The oldest fetch time among `keys`, if any is cached: the age of the
    /// whole feed is the age of its stalest row.
    pub fn oldest(&self, keys: impl IntoIterator<Item = HomeSectionKey>) -> Option<u64> {
        keys.into_iter()
            .filter_map(|key| self.section(key).map(|s| s.fetched_at))
            .min()
    }

    /// Drop every cached row of `provider`, for when the server stops existing
    /// under that identity (deleted, or edited into another kind).
    pub fn forget_provider(&mut self, provider: crate::providers::ProviderId) {
        let prefix = format!("{}:", provider.slug());
        let before = self.sections.len();
        self.sections.retain(|key, _| !key.starts_with(&prefix));
        if self.sections.len() != before {
            self.save();
        }
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// True when `fetched_at` is older than [`HOME_CACHE_TTL_SECS`].
pub fn is_stale(fetched_at: u64) -> bool {
    now_secs().saturating_sub(fetched_at) > HOME_CACHE_TTL_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{HomeSectionKind, ProviderId};

    fn data() -> HomeSectionData {
        HomeSectionData::Cards(Vec::new())
    }

    #[test]
    fn a_row_is_keyed_by_provider_and_kind() {
        let mut cache = HomeCache::default();
        cache.insert(
            HomeSectionKey::new(ProviderId::YouTube, HomeSectionKind::QuickPicks),
            String::new(),
            data(),
            1_000,
        );
        assert!(cache
            .section(HomeSectionKey::new(
                ProviderId::YouTube,
                HomeSectionKind::QuickPicks
            ))
            .is_some());
        assert!(cache
            .section(HomeSectionKey::new(
                ProviderId::Bandcamp,
                HomeSectionKind::QuickPicks
            ))
            .is_none());
        assert!(cache
            .section(HomeSectionKey::new(
                ProviderId::YouTube,
                HomeSectionKind::Trending
            ))
            .is_none());
    }

    #[test]
    fn oldest_picks_the_stalest_cached_row() {
        let mut cache = HomeCache::default();
        let youtube = HomeSectionKey::new(ProviderId::YouTube, HomeSectionKind::QuickPicks);
        let bandcamp = HomeSectionKey::new(ProviderId::Bandcamp, HomeSectionKind::NewReleases);
        cache.insert(youtube, String::new(), data(), 100);
        cache.insert(bandcamp, String::new(), data(), 900);
        assert_eq!(cache.oldest([youtube, bandcamp]), Some(100));
        assert_eq!(
            cache.oldest([HomeSectionKey::new(
                ProviderId::LastFm,
                HomeSectionKind::TopArtists
            )]),
            None
        );
    }

    #[test]
    fn rows_expire_after_the_ttl() {
        let now = now_secs();
        assert!(!is_stale(now));
        assert!(is_stale(now.saturating_sub(HOME_CACHE_TTL_SECS + 1)));
    }
}
