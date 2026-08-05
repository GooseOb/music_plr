use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackSource {
    #[default]
    YouTube,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: u32,
    pub url: String,
    pub source: TrackSource,
    #[serde(default)]
    pub thumbnail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QueueTab {
    #[default]
    Queue,
    RecentlyPlayed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum View {
    #[default]
    Search,
    SongRadio,
    ArtistRadio,
    Playlist,
    Downloads,
}

impl View {
    // True for the text-list views (search/radio) whose scroll bounds and
    // track data are keyed off the live `search_results` / `radio_tracks`
    // fields rather than the playlist store.
    pub const fn is_search_like(&self) -> bool {
        matches!(self, Self::Search | Self::SongRadio | Self::ArtistRadio)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayQueue {
    pub tracks: Vec<Track>,
    pub current_index: usize,
    /// Tracks that have been played, most recent first. Deduped by url.
    pub recently_played: Vec<Track>,
    /// Which tab is currently shown in the queue panel.
    pub queue_tab: QueueTab,
}

impl PlayQueue {
    pub const fn new() -> Self {
        Self {
            tracks: Vec::new(),
            current_index: 0,
            recently_played: Vec::new(),
            queue_tab: QueueTab::Queue,
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.tracks.get(self.current_index)
    }

    #[allow(dead_code)]
    pub const fn next(&mut self) -> Option<usize> {
        if self.current_index + 1 < self.tracks.len() {
            self.current_index += 1;
            Some(self.current_index)
        } else {
            None
        }
    }

    /// Record a track as just-played and push it onto `recently_played`.
    /// Deduplicates by url, keeping most-recent-first. Trims to `max_len`.
    pub fn record_played(&mut self, track: &Track, max_len: usize) {
        self.recently_played.retain(|t| t.url != track.url);
        self.recently_played.insert(0, track.clone());
        if self.recently_played.len() > max_len {
            self.recently_played.truncate(max_len);
        }
    }

    pub const fn previous(&mut self) -> Option<usize> {
        if self.current_index > 0 {
            self.current_index -= 1;
            Some(self.current_index)
        } else {
            None
        }
    }

    pub fn enqueue(&mut self, track: Track) {
        self.tracks.push(track);
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
        self.current_index = 0;
    }
}

impl From<crate::youtube::YouTubeVideo> for Track {
    fn from(v: crate::youtube::YouTubeVideo) -> Self {
        Self {
            id: v.id,
            title: v.title,
            artist: v.channel,
            duration: v.duration as u32,
            url: v.url,
            source: TrackSource::YouTube,
            thumbnail: v.thumbnail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_queue_next_and_previous() {
        let mut q = PlayQueue::new();
        q.tracks = vec![
            Track {
                id: "1".into(),
                title: "A".into(),
                artist: "X".into(),
                duration: 10,
                url: String::new(),
                source: TrackSource::YouTube,
                thumbnail: String::new(),
            },
            Track {
                id: "2".into(),
                title: "B".into(),
                artist: "X".into(),
                duration: 10,
                url: String::new(),
                source: TrackSource::YouTube,
                thumbnail: String::new(),
            },
            Track {
                id: "3".into(),
                title: "C".into(),
                artist: "X".into(),
                duration: 10,
                url: String::new(),
                source: TrackSource::YouTube,
                thumbnail: String::new(),
            },
        ];
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("1"));

        assert_eq!(q.next(), Some(1));
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("2"));

        assert_eq!(q.previous(), Some(0));
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("1"));

        assert_eq!(q.previous(), None);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("1"));

        assert_eq!(q.next(), Some(1));
        assert_eq!(q.next(), Some(2));
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("3"));
        assert_eq!(q.next(), None);
    }

    #[test]
    fn play_queue_empty() {
        let q = PlayQueue::new();
        assert!(q.current().is_none());
    }

    fn make_track(id: &str, url: &str) -> Track {
        Track {
            id: id.into(),
            title: format!("Track {id}"),
            artist: "Artist".into(),
            duration: 10,
            url: url.into(),
            source: TrackSource::YouTube,
            thumbnail: String::new(),
        }
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
        assert_eq!(q.recently_played[0].id, "3");
        assert_eq!(q.recently_played[2].id, "1");

        // Re-recording dedupes: t2 moves to front
        q.record_played(&t2, 50);
        assert_eq!(q.recently_played.len(), 3);
        assert_eq!(q.recently_played[0].id, "2");
        assert_eq!(q.recently_played[1].id, "3");
        assert_eq!(q.recently_played[2].id, "1");
    }

    #[test]
    fn record_played_truncates_to_max() {
        let mut q = PlayQueue::new();
        for i in 1..=60 {
            q.record_played(&make_track(&i.to_string(), &format!("url{i}")), 50);
        }
        assert_eq!(q.recently_played.len(), 50);
        assert_eq!(q.recently_played[0].id, "60");
        assert_eq!(q.recently_played[49].id, "11");
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
        let json =
            r#"{"tracks":[],"current_index":0,"recently_played":[],"queue_tab":"RecentlyPlayed"}"#;
        let q: PlayQueue = serde_json::from_str(json).unwrap();
        assert_eq!(q.queue_tab, QueueTab::RecentlyPlayed);

        let serialized = serde_json::to_string(&q).unwrap();
        let restored: PlayQueue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.queue_tab, QueueTab::RecentlyPlayed);
    }
}
