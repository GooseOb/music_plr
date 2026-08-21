//! `MusicBrainz` provider. Search-only: it resolves canonical track/artist ids
//! (MBIDs) and rich metadata but provides no audio streaming or download. A
//! track found here carries only a `MusicBrainz` id; playing or downloading it
//! triggers a fallback resolution on the default (stream+download) provider.

use crate::providers::{ProviderId, SearchScope, SearchTab};
use crate::types::{ProviderTrack, Track, TrackAlbum, TrackArtist};
use crate::util::urlencode;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MBRecording {
    id: String,
    title: String,
    #[serde(default)]
    artist_credit: Vec<MBArtistCredit>,
    #[serde(default)]
    releases: Vec<MBRelease>,
}

#[derive(Debug, Deserialize)]
struct MBArtistCredit {
    name: String,
    #[serde(default)]
    artist: Option<MBArtist>,
}

#[derive(Debug, Deserialize)]
struct MBArtist {
    id: String,
}

#[derive(Debug, Deserialize)]
struct MBRelease {
    id: String,
    title: String,
}

#[derive(Debug, Deserialize)]
struct MBResponse {
    #[serde(default)]
    recordings: Vec<MBRecording>,
}

fn api_get_json<T: serde::de::DeserializeOwned>(url: &str) -> Option<T> {
    let resp = ureq::get(url)
        .header("User-Agent", "music_plr/0.1 (https://example.com)")
        .call()
        .ok()?;
    let body = resp.into_body().read_to_string().ok()?;
    serde_json::from_str(&body).ok()
}

fn to_track(rec: &MBRecording) -> Track {
    let artist = rec
        .artist_credit
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let artist_id = rec
        .artist_credit
        .first()
        .and_then(|a| a.artist.as_ref())
        .map(|a| a.id.clone());
    let album = rec.releases.first().map(|r| TrackAlbum {
        name: r.title.clone(),
        id: r.id.clone(),
    });
    // Cover Art Archive serves release artwork by MBID; the app's lazy
    // thumbnail downloader fetches this on demand.
    let thumbnail = rec
        .releases
        .first()
        .map(|r| format!("https://coverartarchive.org/release/{}/front", r.id))
        .unwrap_or_default();
    let mut providers = std::collections::HashMap::new();
    providers.insert(
        ProviderId::MusicBrainz,
        ProviderTrack {
            id: rec.id.clone(),
            url: String::new(),
            artist_id,
        },
    );
    Track {
        title: rec.title.clone(),
        artist: TrackArtist {
            name: artist,
            id: None,
        },
        duration: 0,
        thumbnail,
        download_path: None,
        album,
        origin: ProviderId::MusicBrainz,
        providers,
    }
}

pub fn search(query: &str, scope: SearchScope, offset: usize) -> (Vec<Track>, SearchTab) {
    let (entity, field) = match scope {
        SearchScope::Artists => ("artist", "artist"),
        SearchScope::Albums => ("release", "release"),
        _ => ("recording", "recording"),
    };
    let url = format!(
        "https://musicbrainz.org/ws/2/{entity}/?query={field}:{}&offset={}&fmt=json",
        urlencode(query),
        offset
    );
    let tracks = if entity == "recording" {
        api_get_json::<MBResponse>(&url)
            .map(|r| r.recordings.into_iter().map(|t| to_track(&t)).collect())
            .unwrap_or_default()
    } else {
        // Artist/album scopes: search by name and synthesize track stubs.
        let resp = api_get_json::<MusicBrainzGeneric>(&url);
        resp.map(|r| r.into_tracks(scope)).unwrap_or_default()
    };
    (tracks, SearchTab::Songs)
}

pub fn search_more(query: &str, offset: usize) -> Vec<Track> {
    search(query, SearchScope::Songs, offset).0
}

/// Resolve a logical track to a `MusicBrainz` recording MBID.
pub fn resolve_id(track: &Track) -> Option<(String, String)> {
    let q = track.search_query();
    let url = format!(
        "https://musicbrainz.org/ws/2/recording/?query=recording:{}&fmt=json",
        urlencode(&q)
    );
    api_get_json::<MBResponse>(&url)
        .and_then(|r| r.recordings.into_iter().next())
        .map(|rec| (rec.id, String::new()))
}

#[derive(Debug, Deserialize)]
struct MusicBrainzGeneric {
    #[serde(default)]
    artists: Vec<MBArtistName>,
    #[serde(default)]
    releases: Vec<MBReleaseName>,
}

#[derive(Debug, Deserialize)]
struct MBArtistName {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct MBReleaseName {
    id: String,
    title: String,
}

impl MusicBrainzGeneric {
    fn into_tracks(self, scope: SearchScope) -> Vec<Track> {
        match scope {
            SearchScope::Artists => self
                .artists
                .into_iter()
                .map(|a| {
                    let mut providers = std::collections::HashMap::new();
                    providers.insert(
                        ProviderId::MusicBrainz,
                        ProviderTrack {
                            id: a.id,
                            url: String::new(),
                            artist_id: None,
                        },
                    );
                    Track {
                        title: a.name.clone(),
                        artist: TrackArtist {
                            name: a.name.clone(),
                            id: None,
                        },
                        duration: 0,
                        thumbnail: String::new(),
                        download_path: None,
                        album: None,
                        origin: ProviderId::MusicBrainz,
                        providers,
                    }
                })
                .collect(),
            _ => self
                .releases
                .into_iter()
                .map(|r| {
                    let rid = r.id.clone();
                    let mut providers = std::collections::HashMap::new();
                    providers.insert(
                        ProviderId::MusicBrainz,
                        ProviderTrack {
                            id: rid.clone(),
                            url: String::new(),
                            artist_id: None,
                        },
                    );
                    Track {
                        title: r.title.clone(),
                        artist: TrackArtist {
                            name: String::new(),
                            id: None,
                        },
                        duration: 0,
                        thumbnail: String::new(),
                        download_path: None,
                        album: Some(TrackAlbum {
                            name: r.title,
                            id: rid,
                        }),
                        origin: ProviderId::MusicBrainz,
                        providers,
                    }
                })
                .collect(),
        }
    }
}
