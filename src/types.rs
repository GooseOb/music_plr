use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrackSource {
    YouTube,
    Local,
}

impl Default for TrackSource {
    fn default() -> Self {
        Self::YouTube
    }
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
