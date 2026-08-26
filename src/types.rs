#[cfg(test)]
use std::collections::HashMap;
use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Per-provider identifier/url for a track. Re-exported from the provider
/// module for convenience.
pub use crate::providers::ProviderTrack;
use crate::providers::{ProviderId, ProviderMap};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackAlbum {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub download_path: Option<String>,
    pub source: ProviderId,
    pub providers: ProviderMap,
}

impl Track {
    /// Source-provider metadata, used for display. Falls back to the first
    /// carrier when the source entry is missing (e.g. a `Local` track).
    fn source_provider(&self) -> Option<&ProviderTrack> {
        if let Some(pt) = self.providers.get(&self.source) {
            return Some(pt);
        }
        self.providers.values().next()
    }

    /// The track duration in seconds, taken from the source provider's data.
    pub fn duration(&self) -> u32 {
        self.source_provider()
            .map(|p| p.duration)
            .or_else(|| {
                self.providers
                    .values()
                    .find_map(|p| (p.duration > 0).then_some(p.duration))
            })
            .unwrap_or(0)
    }

    /// The thumbnail URL, taken from the source provider's data.
    pub fn thumbnail(&self) -> &str {
        self.source_provider()
            .and_then(|p| (!p.thumbnail.is_empty()).then_some(p.thumbnail.as_str()))
            .or_else(|| {
                self.providers
                    .values()
                    .find_map(|p| (!p.thumbnail.is_empty()).then_some(p.thumbnail.as_str()))
            })
            .unwrap_or("")
    }

    /// The play count, taken from the source provider's data when nonzero,
    /// falling back to any carrier that reports one.
    pub fn play_count(&self) -> u64 {
        self.source_provider().map_or(0, |p| p.play_count).max(
            self.providers
                .values()
                .map(|p| p.play_count)
                .max()
                .unwrap_or(0),
        )
    }

    /// The album metadata, taken from the source provider's data.
    pub fn album(&self) -> Option<&TrackAlbum> {
        self.source_provider()
            .and_then(|p| p.album.as_ref())
            .or_else(|| self.providers.values().find_map(|p| p.album.as_ref()))
    }
}

impl Track {
    /// The provider id for `provider`, if known.
    pub fn provider_id(&self, provider: ProviderId) -> Option<&str> {
        self.providers.get(&provider).map(|t| t.id.as_str())
    }

    /// The provider URL for `provider`, if known.
    pub fn provider_url(&self, provider: ProviderId) -> Option<&str> {
        self.providers.get(&provider).map(|t| t.url.as_str())
    }

    /// The provider artist id for `provider`, if known.
    pub fn provider_artist_id(&self, provider: ProviderId) -> Option<&str> {
        self.providers
            .get(&provider)
            .and_then(|t| t.artist_id.as_deref())
    }

    /// Whether this track carries an identity for `provider`.
    pub fn has_provider(&self, provider: ProviderId) -> bool {
        self.providers.contains_key(&provider)
    }

    /// Insert or replace the provider-specific data on this track. Updates
    /// `source` to the first non-`Local` provider seen.
    pub fn set_provider(&mut self, provider: ProviderId, pt: ProviderTrack) {
        self.providers.insert(provider, pt);
        if self.source == ProviderId::Local && provider != ProviderId::Local {
            self.source = provider;
        }
    }

    /// Whether this track can be downloaded from `provider`.
    pub fn can_download_from(&self, provider: ProviderId) -> bool {
        provider.capabilities().download && self.providers.contains_key(&provider)
    }

    /// Pick the best stream+download provider for playback, preferring
    /// `preferred` (e.g. the default provider) then the source, then any
    /// stream-capable provider.
    pub fn best_stream_provider(&self, preferred: ProviderId) -> Option<ProviderId> {
        let candidates: Vec<ProviderId> = self
            .providers
            .keys()
            .copied()
            .filter(|p| p.capabilities().stream && p.capabilities().download)
            .collect();
        if candidates.contains(&preferred) {
            return Some(preferred);
        }
        if candidates.contains(&self.source) {
            return Some(self.source);
        }
        candidates.first().copied()
    }

    /// A stable identity key used to de-duplicate recently-played entries and
    /// MPRIS metadata. Built from title + artist so the same song played from
    /// different providers collapses to one history entry.
    pub fn dedup_key(&self) -> String {
        format!("{}|{}", self.title, self.artist)
    }

    /// The search query used to resolve this track on a provider: the title
    /// alone when there is no artist, otherwise `title artist`.
    pub fn search_query(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.title, self.artist)
        }
    }

    /// The id for this track's source provider (display/identity key).
    pub fn primary_id(&self) -> &str {
        self.provider_id(self.source).unwrap_or("")
    }

    /// The url for this track's source provider.
    pub fn primary_url(&self) -> &str {
        self.provider_url(self.source).unwrap_or("")
    }

    /// A stable cache key namespacing the source provider with its id, used to
    /// key the on-disk stream cache and download registry.
    pub fn cache_key(&self) -> String {
        let id = self.primary_id();
        format!("{:?}:{}", self.source, id)
    }

    /// Build a `Track` owned by `provider`, carrying that provider's id/url in
    /// `providers` and set as `source`. Centralizes the invariant that the
    /// `source` provider is always present in `providers`. Provider-specific
    /// display metadata (`duration`/`thumbnail`/`album`) lives on the
    /// `ProviderTrack`.
    #[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
    pub fn from_provider(
        provider: ProviderId,
        id: String,
        url: String,
        title: impl Into<String>,
        artist_name: impl Into<String>,
        duration: u32,
        thumbnail: impl Into<String>,
        album: Option<TrackAlbum>,
        artist_id: Option<String>,
    ) -> Self {
        let mut providers = ProviderMap::new();
        providers.insert(
            provider,
            ProviderTrack {
                id,
                url,
                artist_id,
                duration,
                thumbnail: thumbnail.into(),
                album,
                play_count: 0,
            },
        );
        Self {
            title: title.into(),
            artist: artist_name.into(),
            download_path: None,
            source: provider,
            providers,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QueueTab {
    #[default]
    Queue,
    RecentlyPlayed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayQueue {
    pub tracks: Vec<Track>,
    pub recently_played: VecDeque<Track>,
    pub queue_tab: QueueTab,
}

impl PlayQueue {
    pub const fn new() -> Self {
        Self {
            tracks: Vec::new(),
            recently_played: VecDeque::new(),
            queue_tab: QueueTab::Queue,
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.tracks.first()
    }

    /// Pop the current (first) track off the front of the queue after it has
    /// finished playing, making the next track the new current.
    pub fn advance(&mut self) -> bool {
        if self.tracks.is_empty() {
            false
        } else {
            self.tracks.remove(0);
            true
        }
    }

    /// Record a track as just-played and push it onto `recently_played`.
    /// Deduplicates by dedup key, keeping most-recent-first. Trims to `max_len`.
    pub fn record_played(&mut self, track: &Track, max_len: usize) {
        let key = track.dedup_key();
        self.recently_played.retain(|t| t.dedup_key() != key);
        self.recently_played.push_front(track.clone());
        while self.recently_played.len() > max_len {
            self.recently_played.pop_back();
        }
    }

    /// Restore the most recently played track to the front of the queue
    /// (becomes the new current track).
    pub fn restore_previous(&mut self) -> bool {
        if let Some(track) = self.recently_played.pop_front() {
            self.tracks.insert(0, track);
            true
        } else {
            false
        }
    }

    pub fn enqueue(&mut self, track: Track) {
        self.tracks.push(track);
    }

    pub fn set_queue(&mut self, tracks: Vec<Track>, max_len: usize) {
        if let Some(old) = self.current().cloned() {
            self.record_played(&old, max_len);
        }
        self.tracks = tracks;
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_track(id: &str, url: &str) -> Track {
        let mut t = Track {
            title: format!("Track {id}"),
            artist: "Artist".into(),
            download_path: None,
            source: ProviderId::YouTube,
            providers: HashMap::new(),
        };
        t.set_provider(
            ProviderId::YouTube,
            ProviderTrack {
                id: id.into(),
                url: url.into(),
                artist_id: None,
                duration: 10,
                thumbnail: String::new(),
                album: None,
                play_count: 0,
            },
        );
        t
    }

    #[test]
    fn play_queue_advance_and_restore_previous() {
        let mut q = PlayQueue::new();
        q.tracks = vec![
            make_track("1", "url1"),
            make_track("2", "url2"),
            make_track("3", "url3"),
        ];
        assert_eq!(
            q.current().map(|t| t.provider_id(ProviderId::YouTube)),
            Some(Some("1"))
        );

        assert!(q.advance());
        assert_eq!(
            q.current().map(|t| t.provider_id(ProviderId::YouTube)),
            Some(Some("2"))
        );

        let t1 = make_track("1", "url1");
        q.record_played(&t1, 50);
        assert!(q.restore_previous());
        assert_eq!(
            q.current().map(|t| t.provider_id(ProviderId::YouTube)),
            Some(Some("1"))
        );

        assert!(q.advance());
        assert_eq!(
            q.current().map(|t| t.provider_id(ProviderId::YouTube)),
            Some(Some("2"))
        );
        assert!(q.advance());
        assert_eq!(
            q.current().map(|t| t.provider_id(ProviderId::YouTube)),
            Some(Some("3"))
        );
        assert!(q.advance());
        assert!(q.current().is_none());
        assert!(!q.advance());
    }

    #[test]
    fn play_queue_empty() {
        let mut q = PlayQueue::new();
        assert!(q.current().is_none());
        assert!(!q.advance());
        assert!(!q.restore_previous());
    }

    #[test]
    fn record_played_order_and_dedup() {
        let mut q = PlayQueue::new();
        let t1 = make_track("1", "url1");
        let t2 = make_track("2", "url2");
        let t3 = make_track("3", "url3");

        q.record_played(&t1, 50);
        q.record_played(&t2, 50);
        q.record_played(&t3, 50);

        assert_eq!(q.recently_played.len(), 3);
        assert_eq!(
            q.recently_played[0].provider_id(ProviderId::YouTube),
            Some("3")
        );
        assert_eq!(
            q.recently_played[2].provider_id(ProviderId::YouTube),
            Some("1")
        );

        q.record_played(&t2, 50);
        assert_eq!(q.recently_played.len(), 3);
        assert_eq!(
            q.recently_played[0].provider_id(ProviderId::YouTube),
            Some("2")
        );
        assert_eq!(
            q.recently_played[1].provider_id(ProviderId::YouTube),
            Some("3")
        );
        assert_eq!(
            q.recently_played[2].provider_id(ProviderId::YouTube),
            Some("1")
        );
    }

    #[test]
    fn record_played_truncates_to_max() {
        let mut q = PlayQueue::new();
        for i in 1..=60 {
            q.record_played(&make_track(&i.to_string(), &format!("url{i}")), 50);
        }
        assert_eq!(q.recently_played.len(), 50);
        assert_eq!(
            q.recently_played[0].provider_id(ProviderId::YouTube),
            Some("60")
        );
        assert_eq!(
            q.recently_played[49].provider_id(ProviderId::YouTube),
            Some("11")
        );
    }

    #[test]
    fn queue_tab_serde() {
        let json = r#"{"tracks":[],"recently_played":[],"queue_tab":"RecentlyPlayed"}"#;
        let q: PlayQueue = serde_json::from_str(json).unwrap();
        assert_eq!(q.queue_tab, QueueTab::RecentlyPlayed);

        let serialized = serde_json::to_string(&q).unwrap();
        let restored: PlayQueue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.queue_tab, QueueTab::RecentlyPlayed);
    }
}
