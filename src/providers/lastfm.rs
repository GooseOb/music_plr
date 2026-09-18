use std::fmt::Write as _;

use anyhow::{Context, Result};

use crate::{
    providers::{
        AlbumMeta, ArtistAlbumCard, ArtistHeader, ArtistPage, CardData, ProviderId,
        RelatedArtistCard, SearchScope, SearchTab,
    },
    theme::SEARCH_PAGE_SIZE,
    types::{Track, TrackAlbum},
};

const API_URL: &str = "https://ws.audioscrobbler.com/2.0/";

const BUNDLED_API_KEY: &str = "14a3619d2a81b7cd3e2e0a9adaebeecf";

/// Last.fm's generic "no artwork" star placeholder, served identically for
/// every track/artist/album without real art. Treated as "no image" so rows
/// fall back to the music-note icon instead of showing the same star picture
/// on every row.
const DEFAULT_ART_HASH: &str = "2a96cbd8b46e442fc41c2b86b821562f";

fn api_key() -> &'static str {
    BUNDLED_API_KEY
}

fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_global(Some(std::time::Duration::from_secs(20)))
            .build()
            .new_agent()
    })
}

fn get(method: &str, params: &[(&str, &str)]) -> Result<serde_json::Value> {
    let key = api_key();
    if key.trim().is_empty() {
        anyhow::bail!("Last.fm API key is not set (Settings)");
    }
    let mut url = format!(
        "{API_URL}?method={method}&api_key={}&format=json",
        crate::util::urlencode(key.trim())
    );
    for (k, v) in params {
        let _ = write!(url, "&{k}={}", crate::util::urlencode(v));
    }
    let mut body = agent()
        .get(&url)
        .header(
            "User-Agent",
            "goosemusic/0.1 (https://github.com/gooseob/music_plr)",
        )
        .call()
        .with_context(|| format!("Last.fm {method} request failed"))?
        .body_mut()
        .read_to_string()
        .with_context(|| format!("Last.fm {method} response unreadable"))?;
    let value: serde_json::Value = serde_json::from_str(&body)
        .with_context(|| format!("Last.fm {method} response invalid"))?;
    body.clear();
    if let Some(message) = value.get("message").and_then(|m| m.as_str()) {
        if value.get("error").is_some() {
            anyhow::bail!("Last.fm: {message}");
        }
    }
    Ok(value)
}

fn track_key(artist: &str, title: &str) -> String {
    format!("{artist}\u{1f}{title}")
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

fn image_url(v: &serde_json::Value) -> String {
    let url = raw_image_url(v);
    if url.contains(DEFAULT_ART_HASH) {
        String::new()
    } else {
        url
    }
}

fn raw_image_url(v: &serde_json::Value) -> String {
    let images = match v {
        serde_json::Value::Array(items) => items.clone(),
        _ => v
            .get("image")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default(),
    };
    for want in ["extralarge", "large", "medium", "small", "mega"] {
        if let Some(url) = images.iter().find_map(|i| {
            let size_ok = i.get("size").and_then(|s| s.as_str()) == Some(want);
            let text = i.get("#text").and_then(|t| t.as_str()).unwrap_or_default();
            (size_ok && !text.is_empty()).then(|| text.to_string())
        }) {
            return url;
        }
    }
    images
        .iter()
        .filter_map(|i| i.get("#text").and_then(|t| t.as_str()))
        .find(|t| !t.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn listeners(v: &serde_json::Value) -> u64 {
    v.get("listeners")
        .or_else(|| v.get("stats").and_then(|s| s.get("listeners")))
        .and_then(|l| {
            l.as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| l.as_u64())
        })
        .unwrap_or(0)
}

fn track_from_search(t: &serde_json::Value) -> Track {
    let title = str_field(t, "name");
    let artist = str_field(t, "artist");
    let mut track = Track::from_provider(
        ProviderId::LastFm,
        track_key(&artist, &title),
        str_field(t, "url"),
        title,
        artist.clone(),
        0,
        image_url(t),
        None,
        Some(artist),
    );
    if let Some(pt) = track.providers.get_mut(&ProviderId::LastFm) {
        pt.play_count = listeners(t);
    }
    track
}

/// Real artwork for a track lives on its album, which `track.search` does
/// not return (it serves the generic star placeholder for everything). One
/// `track.getInfo` call per track recovers the album image.
fn album_art(artist: &str, title: &str) -> Option<String> {
    let info = get(
        "track.getInfo",
        &[("artist", artist), ("track", title), ("autocorrect", "1")],
    )
    .ok()?;
    let album = info.get("track")?.get("album")?;
    let url = image_url(album);
    (!url.is_empty()).then_some(url)
}

fn enrich_songs_with_album_art(tracks: Vec<Track>) -> Vec<Track> {
    let tracks = enrich_parallel(tracks, album_art_for);
    let mut artists: Vec<String> = Vec::new();
    for track in &tracks {
        if missing_lastfm_thumbnail(track) && !artists.contains(&track.artist) {
            artists.push(track.artist.clone());
        }
    }
    if artists.is_empty() {
        return tracks;
    }
    let arts: Vec<Option<String>> = std::thread::scope(|s| {
        artists
            .iter()
            .map(|artist| s.spawn(move || artist_art(artist)))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    });
    let by_artist: std::collections::HashMap<&str, &str> = artists
        .iter()
        .zip(arts.iter())
        .filter_map(|(artist, art)| art.as_deref().map(|url| (artist.as_str(), url)))
        .collect();
    tracks
        .into_iter()
        .map(|mut track| {
            if missing_lastfm_thumbnail(&track) {
                if let Some(art) = by_artist.get(track.artist.as_str()) {
                    set_lastfm_thumbnail(&mut track, (*art).to_string());
                }
            }
            track
        })
        .collect()
}

fn set_lastfm_thumbnail(track: &mut Track, art: String) {
    if let Some(pt) = track.providers.get_mut(&ProviderId::LastFm) {
        pt.thumbnail = art;
    }
}

fn missing_lastfm_thumbnail(track: &Track) -> bool {
    track
        .providers
        .get(&ProviderId::LastFm)
        .is_none_or(|pt| pt.thumbnail.is_empty())
}

fn album_art_for(track: &Track) -> Option<String> {
    album_art(&track.artist, &track.title)
}

/// Artist-level fallback for tracks with no album art (covers, remixes):
/// the top album's artwork. Still the right artist, far better than the
/// generic icon, and fetched once per unique artist.
fn artist_art(artist: &str) -> Option<String> {
    let top = get(
        "artist.gettopalbums",
        &[("artist", artist), ("limit", "5"), ("autocorrect", "1")],
    )
    .ok()?;
    collect(&["topalbums", "album"], &top)
        .iter()
        .map(image_url)
        .find(|url| !url.is_empty())
}

fn enrich_parallel(
    tracks: Vec<Track>,
    art_for: impl Fn(&Track) -> Option<String> + Sync,
) -> Vec<Track> {
    let art_for = &art_for;
    std::thread::scope(|s| {
        tracks
            .into_iter()
            .map(|track| {
                s.spawn(move || {
                    let mut track = track;
                    if missing_lastfm_thumbnail(&track) {
                        if let Some(art) = art_for(&track) {
                            set_lastfm_thumbnail(&mut track, art);
                        }
                    }
                    track
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    })
}

fn collect(path: &[&str], value: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut node = value;
    for key in path {
        match node.get(key) {
            Some(next) => node = next,
            None => return Vec::new(),
        }
    }
    match node {
        serde_json::Value::Array(items) => items.clone(),
        single if single.is_object() => vec![single.clone()],
        _ => Vec::new(),
    }
}

pub fn search(query: &str, scope: SearchScope, offset: usize) -> Result<(Vec<Track>, SearchTab)> {
    let query = query.trim();
    if query.is_empty() {
        return Ok((Vec::new(), SearchTab::from_scope(scope)));
    }
    let page = (offset / SEARCH_PAGE_SIZE + 1).to_string();
    let limit = SEARCH_PAGE_SIZE.to_string();
    match scope {
        SearchScope::Artists => {
            let v = get(
                "artist.search",
                &[("artist", query), ("limit", &limit), ("page", &page)],
            )?;
            let cards = collect(&["results", "artistmatches", "artist"], &v)
                .iter()
                .map(|a| CardData {
                    id: str_field(a, "name"),
                    title: str_field(a, "name"),
                    subtitle: String::new(),
                    thumbnail: image_url(a),
                })
                .collect();
            Ok((Vec::new(), SearchTab::Artists(cards)))
        }
        SearchScope::Albums => {
            let v = get(
                "album.search",
                &[("album", query), ("limit", &limit), ("page", &page)],
            )?;
            let cards = collect(&["results", "albummatches", "album"], &v)
                .iter()
                .map(|a| CardData {
                    id: format!("{}\u{1f}{}", str_field(a, "artist"), str_field(a, "name")),
                    title: str_field(a, "name"),
                    subtitle: str_field(a, "artist"),
                    thumbnail: image_url(a),
                })
                .collect();
            Ok((Vec::new(), SearchTab::Albums(cards)))
        }
        _ => {
            let v = get(
                "track.search",
                &[("track", query), ("limit", &limit), ("page", &page)],
            )?;
            let tracks: Vec<Track> = collect(&["results", "trackmatches", "track"], &v)
                .iter()
                .map(track_from_search)
                .collect();
            Ok((enrich_songs_with_album_art(tracks), SearchTab::Songs))
        }
    }
}

pub fn search_more(query: &str, offset: usize) -> Result<Vec<Track>> {
    Ok(search(query, SearchScope::Songs, offset)?.0)
}

fn split_album_id(id: &str) -> Option<(String, String)> {
    let mut parts = id.splitn(2, '\u{1f}');
    Some((parts.next()?.to_string(), parts.next()?.to_string()))
}

fn album_tracks(artist: &str, album: &str, thumbnail: &str) -> Result<Vec<Track>> {
    let info = get(
        "album.getinfo",
        &[("artist", artist), ("album", album), ("autocorrect", "1")],
    )?;
    let tracks = collect(&["album", "tracks", "track"], &info);
    let cover = info
        .pointer("/album/image")
        .map(image_url)
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| thumbnail.to_string());
    Ok(tracks
        .iter()
        .map(|t| {
            let title = str_field(t, "name");
            let track_artist = t
                .get("artist")
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(artist)
                .to_string();
            let duration = t
                .get("duration")
                .and_then(|d| {
                    d.as_str()
                        .and_then(|s| s.parse().ok())
                        .or_else(|| d.as_u64())
                })
                .unwrap_or(0) as u32;
            let page = format!(
                "https://www.last.fm/music/{}/_/{}",
                crate::util::urlencode(&track_artist),
                crate::util::urlencode(&title)
            );
            Track::from_provider(
                ProviderId::LastFm,
                track_key(&track_artist, &title),
                page,
                title,
                track_artist.clone(),
                duration,
                cover.clone(),
                Some(TrackAlbum {
                    name: album.to_string(),
                    id: format!("{artist}\u{1f}{album}"),
                }),
                Some(track_artist),
            )
        })
        .collect())
}

pub fn browse(id: &str, kind: &str) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    if kind == "artist" {
        let top = get(
            "artist.gettoptracks",
            &[("artist", id), ("limit", "30"), ("autocorrect", "1")],
        )?;
        let tracks = collect(&["toptracks", "track"], &top)
            .iter()
            .map(|t| {
                let title = str_field(t, "name");
                let page = t
                    .get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or_default()
                    .to_string();
                let page = if page.is_empty() {
                    format!(
                        "https://www.last.fm/music/{}/_/{}",
                        crate::util::urlencode(id),
                        crate::util::urlencode(&title)
                    )
                } else {
                    page
                };
                let mut track = Track::from_provider(
                    ProviderId::LastFm,
                    track_key(id, &title),
                    page,
                    title,
                    id.to_string(),
                    0,
                    image_url(t),
                    None,
                    Some(id.to_string()),
                );
                if let Some(pt) = track.providers.get_mut(&ProviderId::LastFm) {
                    pt.play_count = t
                        .get("playcount")
                        .and_then(|p| {
                            p.as_str()
                                .and_then(|s| s.parse().ok())
                                .or_else(|| p.as_u64())
                        })
                        .unwrap_or(0);
                }
                track
            })
            .collect();
        return Ok((tracks, None));
    }
    let (artist, album) = split_album_id(id).context("bad Last.fm album id")?;
    let tracks = album_tracks(&artist, &album, "")?;
    Ok((tracks, None))
}

fn header_from_info(artist: &serde_json::Value) -> ArtistHeader {
    let mut stats = Vec::new();
    let listener_count = listeners(artist);
    if listener_count > 0 {
        stats.push((
            "Listeners".to_string(),
            crate::util::format_count(listener_count),
        ));
    }
    if let Some(plays) = artist
        .get("stats")
        .and_then(|s| s.get("playcount"))
        .and_then(|p| p.as_str())
    {
        if let Ok(plays) = plays.parse::<u64>() {
            stats.push(("Plays".to_string(), crate::util::format_count(plays)));
        }
    }
    if let Some(tags) = artist
        .get("tags")
        .and_then(|t| t.get("tag"))
        .and_then(|t| t.as_array())
    {
        let names: Vec<String> = tags
            .iter()
            .take(5)
            .map(|t| str_field(t, "name"))
            .filter(|n| !n.is_empty())
            .collect();
        if !names.is_empty() {
            stats.push(("Tags".to_string(), names.join(", ")));
        }
    }
    let bio = artist
        .get("bio")
        .and_then(|b| b.get("summary"))
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    ArtistHeader {
        image: image_url(artist),
        stats,
        description: bio,
    }
}

fn popular_from_top(id: &str) -> Vec<Track> {
    let Ok(top) = get(
        "artist.gettoptracks",
        &[("artist", id), ("limit", "10"), ("autocorrect", "1")],
    ) else {
        return Vec::new();
    };
    collect(&["toptracks", "track"], &top)
        .iter()
        .map(|t| {
            let title = str_field(t, "name");
            let page_url = t
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or_default()
                .to_string();
            Track::from_provider(
                ProviderId::LastFm,
                track_key(id, &title),
                page_url,
                title,
                id.to_string(),
                0,
                image_url(t),
                None,
                Some(id.to_string()),
            )
        })
        .collect()
}

pub fn fetch_artist_page(
    id: &str,
    kinds: &[crate::providers::ArtistDataKind],
) -> Result<ArtistPage> {
    use crate::providers::ArtistDataKind as K;

    let mut page = ArtistPage::default();
    if K::Header.wanted(kinds) || K::Related.wanted(kinds) {
        let info = get("artist.getinfo", &[("artist", id), ("autocorrect", "1")])?;
        let artist = info
            .get("artist")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if K::Header.wanted(kinds) {
            page.header = Some(header_from_info(&artist));
        }
        if K::Related.wanted(kinds) {
            page.related = collect(&["similar", "artist"], &artist)
                .iter()
                .take(10)
                .map(|a| RelatedArtistCard {
                    id: str_field(a, "name"),
                    name: str_field(a, "name"),
                    stat: String::new(),
                    thumbnail: image_url(a),
                })
                .collect();
        }
    }
    if K::Popular.wanted(kinds) {
        page.popular = popular_from_top(id);
    }
    if K::Albums.wanted(kinds) {
        if let Ok(top) = get(
            "artist.gettopalbums",
            &[("artist", id), ("limit", "10"), ("autocorrect", "1")],
        ) {
            page.albums = collect(&["topalbums", "album"], &top)
                .iter()
                .map(|a| ArtistAlbumCard {
                    id: format!("{}\u{1f}{}", id, str_field(a, "name")),
                    title: str_field(a, "name"),
                    date: String::new(),
                    badge: "Album".to_string(),
                    thumbnail: image_url(a),
                })
                .collect();
        }
    }
    Ok(page)
}

#[allow(clippy::unnecessary_wraps)]
pub fn resolve_artist_id(name: &str) -> Result<Option<String>> {
    let found = get("artist.search", &[("artist", name), ("limit", "1")])
        .map(|v| {
            collect(&["results", "artistmatches", "artist"], &v)
                .into_iter()
                .next()
                .map(|a| str_field(&a, "name"))
        })
        .unwrap_or_default()
        .filter(|n| !n.is_empty());
    Ok(found)
}

#[allow(clippy::unnecessary_wraps)]
pub fn resolve_id(track: &Track) -> Result<Option<Track>> {
    let found = get(
        "track.search",
        &[("track", &track.search_query()), ("limit", "1")],
    )
    .map(|v| {
        collect(&["results", "trackmatches", "track"], &v)
            .into_iter()
            .next()
            .map(|t| track_from_search(&t))
    })
    .unwrap_or_default();
    Ok(match found {
        Some(t) => enrich_songs_with_album_art(vec![t]).pop(),
        None => None,
    })
}

pub fn radio_song(id: &str) -> Result<Vec<Track>> {
    let (artist, title) = split_album_id(id).context("bad Last.fm track id")?;
    let similar = get(
        "track.getsimilar",
        &[
            ("artist", &artist),
            ("track", &title),
            ("limit", "20"),
            ("autocorrect", "1"),
        ],
    )?;
    Ok(collect(&["similartracks", "track"], &similar)
        .iter()
        .map(|t| {
            let name = str_field(t, "name");
            let track_artist = t
                .get("artist")
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or_default()
                .to_string();
            let page = t
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or_default()
                .to_string();
            Track::from_provider(
                ProviderId::LastFm,
                track_key(&track_artist, &name),
                page,
                name,
                track_artist.clone(),
                0,
                image_url(t),
                None,
                Some(track_artist),
            )
        })
        .collect())
}

pub fn radio_artist(id: &str) -> Result<Vec<Track>> {
    let info = get("artist.getinfo", &[("artist", id), ("autocorrect", "1")])?;
    let similar: Vec<String> = info
        .get("artist")
        .map(|a| collect(&["similar", "artist"], a))
        .unwrap_or_default()
        .iter()
        .take(3)
        .map(|a| str_field(a, "name"))
        .filter(|n| !n.is_empty())
        .collect();
    let mut tracks = Vec::new();
    for name in &similar {
        if let Ok(top) = get(
            "artist.gettoptracks",
            &[("artist", name), ("limit", "7"), ("autocorrect", "1")],
        ) {
            tracks.extend(
                collect(&["toptracks", "track"], &top)
                    .iter()
                    .take(7)
                    .map(|t| {
                        let title = str_field(t, "name");
                        let page = t
                            .get("url")
                            .and_then(|u| u.as_str())
                            .unwrap_or_default()
                            .to_string();
                        Track::from_provider(
                            ProviderId::LastFm,
                            track_key(name, &title),
                            page,
                            title,
                            name.clone(),
                            0,
                            image_url(t),
                            None,
                            Some(name.clone()),
                        )
                    }),
            );
        }
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_key_splits_back() {
        let key = track_key("Cher", "Believe");
        let (artist, title) = split_album_id(&key).unwrap();
        assert_eq!((artist.as_str(), title.as_str()), ("Cher", "Believe"));
    }

    #[test]
    fn image_url_prefers_large_sizes() {
        let v = serde_json::json!({
            "image": [
                {"#text": "small.jpg", "size": "small"},
                {"#text": "", "size": "large"},
                {"#text": "xl.jpg", "size": "extralarge"},
            ]
        });
        assert_eq!(image_url(&v), "xl.jpg");
        let bare = serde_json::json!([{"#text": "a.jpg", "size": "medium"}]);
        assert_eq!(image_url(&bare), "a.jpg");
    }

    #[test]
    fn image_url_filters_default_placeholder() {
        let v = serde_json::json!({
            "image": [
                {"#text": "https://lastfm-img.freetls.fastly.net/i/u/300x300/2a96cbd8b46e442fc41c2b86b821562f.png", "size": "extralarge"},
            ]
        });
        assert_eq!(image_url(&v), "");
    }

    #[test]
    fn collect_unwraps_single_objects() {
        let v = serde_json::json!({"results": {"trackmatches": {"track": {"name": "x"}}}});
        assert_eq!(collect(&["results", "trackmatches", "track"], &v).len(), 1);
        let missing = serde_json::json!({});
        assert!(collect(&["results", "trackmatches", "track"], &missing).is_empty());
    }

    #[test]
    fn listeners_reads_both_shapes() {
        assert_eq!(listeners(&serde_json::json!({"listeners": "123"})), 123);
        assert_eq!(
            listeners(&serde_json::json!({"stats": {"listeners": "456"}})),
            456
        );
        assert_eq!(listeners(&serde_json::json!({})), 0);
    }
}
