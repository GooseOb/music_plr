use crate::providers::{ProviderId, ProviderMap};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::collections::HashMap;
use std::collections::VecDeque;

/// Per-provider identifier/url for a track. Re-exported from the provider
/// module for convenience.
pub use crate::providers::ProviderTrack;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackAlbum {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackArtist {
    pub name: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Track {
    pub title: String,
    pub artist: TrackArtist,
    pub duration: u32,
    #[serde(default)]
    pub thumbnail: String,
    /// Absolute path to the downloaded audio file on disk, if this track has
    /// been downloaded. `None` for streamed/cached-only or local tracks.
    #[serde(default)]
    pub download_path: Option<String>,
    #[serde(default)]
    pub album: Option<TrackAlbum>,
    /// The provider that produced this track (its display source and the
    /// default provider to stream/download from when multiple are present).
    #[serde(default)]
    pub origin: ProviderId,
    /// Per-provider identifiers/urls. Keyed by [`ProviderId`]; at least the
    /// `origin` provider is always present. A single logical track may carry
    /// several (e.g. a `YouTube` result later resolved on `SoundCloud`).
    #[serde(default)]
    pub providers: ProviderMap,
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
    /// `origin` to the first non-`Local` provider seen.
    pub fn set_provider(&mut self, provider: ProviderId, pt: ProviderTrack) {
        self.providers.insert(provider, pt);
        if self.origin == ProviderId::Local && provider != ProviderId::Local {
            self.origin = provider;
        }
    }

    /// Whether this track can be downloaded from `provider`.
    pub fn can_download_from(&self, provider: ProviderId) -> bool {
        provider.capabilities().download && self.providers.contains_key(&provider)
    }

    /// Pick the best stream+download provider for playback, preferring
    /// `preferred` (e.g. the default provider) then the origin, then any
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
        if candidates.contains(&self.origin) {
            return Some(self.origin);
        }
        candidates.first().copied()
    }

    /// A stable identity key used to de-duplicate recently-played entries and
    /// MPRIS metadata. Built from title + artist so the same song played from
    /// different providers collapses to one history entry.
    pub fn dedup_key(&self) -> String {
        format!("{}|{}", self.title, self.artist.name)
    }

    /// The search query used to resolve this track on a provider: the title
    /// alone when there is no artist, otherwise `title artist`.
    pub fn search_query(&self) -> String {
        if self.artist.name.is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.title, self.artist.name)
        }
    }

    /// The id for this track's origin provider (display/identity key).
    pub fn primary_id(&self) -> &str {
        self.provider_id(self.origin).unwrap_or("")
    }

    /// The url for this track's origin provider.
    pub fn primary_url(&self) -> &str {
        self.provider_url(self.origin).unwrap_or("")
    }

    /// A stable cache key namespacing the origin provider with its id, used to
    /// key the on-disk stream cache and download registry.
    pub fn cache_key(&self) -> String {
        let id = self.primary_id();
        format!("{:?}:{}", self.origin, id)
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
            artist: TrackArtist {
                name: "Artist".into(),
                id: None,
            },
            duration: 10,
            thumbnail: String::new(),
            download_path: None,
            album: None,
            origin: ProviderId::YouTube,
            providers: HashMap::new(),
        };
        t.set_provider(
            ProviderId::YouTube,
            ProviderTrack {
                id: id.into(),
                url: url.into(),
                artist_id: None,
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
    fn queue_tab_default_is_queue() {
        let q = PlayQueue::new();
        assert_eq!(q.queue_tab, QueueTab::Queue);
    }
}

#[cfg(test)]
mod queue_tab_tests {
    use super::*;

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
