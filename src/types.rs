use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackSource {
    #[default]
    YouTube,
    Local,
}

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
    pub id: String,
    pub title: String,
    pub artist: TrackArtist,
    pub duration: u32,
    pub url: String,
    pub source: TrackSource,
    #[serde(default)]
    pub thumbnail: String,
    /// Absolute path to the downloaded audio file on disk, if this track has
    /// been downloaded. `None` for streamed/cached-only or local tracks.
    #[serde(default)]
    pub download_path: Option<String>,
    #[serde(default)]
    pub album: Option<TrackAlbum>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QueueTab {
    #[default]
    Queue,
    RecentlyPlayed,
}

use std::collections::VecDeque;

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
    /// Deduplicates by url, keeping most-recent-first. Trims to `max_len`.
    pub fn record_played(&mut self, track: &Track, max_len: usize) {
        self.recently_played.retain(|t| t.url != track.url);
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

impl From<crate::youtube::YouTubeVideo> for Track {
    fn from(v: crate::youtube::YouTubeVideo) -> Self {
        Self {
            id: v.id,
            title: v.title,
            artist: TrackArtist {
                name: v.channel,
                id: v.artist_id,
            },
            duration: v.duration,
            url: v.url,
            source: TrackSource::YouTube,
            thumbnail: v.thumbnail,
            download_path: None,
            album: v.album,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_track(id: &str, url: &str) -> Track {
        Track {
            id: id.into(),
            title: format!("Track {id}"),
            artist: TrackArtist {
                name: "Artist".into(),
                id: None,
            },
            duration: 10,
            url: url.into(),
            source: TrackSource::YouTube,
            thumbnail: String::new(),
            download_path: None,
            album: None,
        }
    }

    #[test]
    fn play_queue_advance_and_restore_previous() {
        let mut q = PlayQueue::new();
        q.tracks = vec![
            Track {
                id: "1".into(),
                title: "A".into(),
                artist: TrackArtist {
                    name: "X".into(),
                    id: None,
                },
                duration: 10,
                url: String::new(),
                source: TrackSource::YouTube,
                thumbnail: String::new(),
                download_path: None,
                album: None,
            },
            Track {
                id: "2".into(),
                title: "B".into(),
                artist: TrackArtist {
                    name: "X".into(),
                    id: None,
                },
                duration: 10,
                url: String::new(),
                source: TrackSource::YouTube,
                thumbnail: String::new(),
                download_path: None,
                album: None,
            },
            Track {
                id: "3".into(),
                title: "C".into(),
                artist: TrackArtist {
                    name: "X".into(),
                    id: None,
                },
                duration: 10,
                url: String::new(),
                source: TrackSource::YouTube,
                thumbnail: String::new(),
                download_path: None,
                album: None,
            },
        ];
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("1"));

        // advance removes the current track, making the next one current
        assert!(q.advance());
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("2"));

        // record played and restore_previous puts it back at the front
        let t1 = make_track("1", "url1");
        q.record_played(&t1, 50);
        assert!(q.restore_previous());
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("1"));

        // advance through all tracks
        assert!(q.advance());
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("2"));
        assert!(q.advance());
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("3"));
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
        let json = r#"{"tracks":[],"recently_played":[],"queue_tab":"RecentlyPlayed"}"#;
        let q: PlayQueue = serde_json::from_str(json).unwrap();
        assert_eq!(q.queue_tab, QueueTab::RecentlyPlayed);

        let serialized = serde_json::to_string(&q).unwrap();
        let restored: PlayQueue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.queue_tab, QueueTab::RecentlyPlayed);
    }
}
