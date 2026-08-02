use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum View {
    Search(String),
    SongRadio(String),
    ArtistRadio(String),
    Playlist(usize),
    Downloads,
}

impl Default for View {
    fn default() -> Self {
        View::Search(String::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayQueue {
    pub tracks: Vec<Track>,
    pub current_index: usize,
}

impl PlayQueue {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            current_index: 0,
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.tracks.get(self.current_index)
    }

    pub fn next(&mut self) -> Option<usize> {
        if self.current_index + 1 < self.tracks.len() {
            self.current_index += 1;
            Some(self.current_index)
        } else {
            None
        }
    }

    pub fn previous(&mut self) -> Option<usize> {
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
        Track {
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
                url: "".into(),
                source: TrackSource::YouTube,
                thumbnail: "".into(),
            },
            Track {
                id: "2".into(),
                title: "B".into(),
                artist: "X".into(),
                duration: 10,
                url: "".into(),
                source: TrackSource::YouTube,
                thumbnail: "".into(),
            },
            Track {
                id: "3".into(),
                title: "C".into(),
                artist: "X".into(),
                duration: 10,
                url: "".into(),
                source: TrackSource::YouTube,
                thumbnail: "".into(),
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
}
