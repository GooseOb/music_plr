//! `Audius` provider. Search, browsing, artist pages and home rows all go
//! through the public `api.audius.co` v1 API, which needs no key. Tracks play
//! and download over plain HTTP: every track stores the provider's stable
//! `/tracks/{id}/stream` endpoint, which redirects to a signed CDN URL, so the
//! `yt-dlp` pipeline is never involved.

use std::fmt::Write as _;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::{
    data::library::LibraryKind,
    providers::{
        AlbumMeta, ArtistAlbumCard, ArtistDataKind, ArtistHeader, ArtistPage, CardData, ProviderId,
        RelatedArtistCard, SearchScope, SearchTab,
    },
    theme::SEARCH_PAGE_SIZE,
    types::{Track, TrackAlbum},
};

const API_BASE: &str = "https://api.audius.co/v1";
const APP_NAME: &str = "goosemusic";
const USER_AGENT: &str = "goosemusic/0.1 (https://github.com/GooseOb/music_plr)";
const BROWSE_LIMIT: usize = 50;

fn get(path: &str, params: &[(&str, String)]) -> Result<Value> {
    let mut url = format!("{API_BASE}{path}?app_name={APP_NAME}");
    for (key, value) in params {
        let _ = write!(url, "&{key}={}", crate::util::urlencode(value));
    }
    let body = super::http_agent()
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("Audius {path} request failed"))?
        .body_mut()
        .read_to_string()
        .with_context(|| format!("Audius {path} response unreadable"))?;
    serde_json::from_str(&body).with_context(|| format!("Audius {path} response invalid"))
}

fn data(path: &str, params: &[(&str, String)]) -> Result<Value> {
    get(path, params)?
        .get("data")
        .cloned()
        .with_context(|| format!("Audius {path} response has no data"))
}

fn object(path: &str, params: &[(&str, String)]) -> Result<Value> {
    match data(path, params)? {
        value @ Value::Object(_) => Ok(value),
        _ => anyhow::bail!("Audius {path} returned no object"),
    }
}

fn items(path: &str, params: &[(&str, String)]) -> Result<Vec<Value>> {
    match data(path, params)? {
        Value::Array(items) => Ok(items),
        _ => Ok(Vec::new()),
    }
}

fn query_page(query: &str, offset: usize) -> [(&'static str, String); 3] {
    [
        ("query", query.to_string()),
        ("limit", SEARCH_PAGE_SIZE.to_string()),
        ("offset", offset.to_string()),
    ]
}

fn limit(limit: usize) -> [(&'static str, String); 1] {
    [("limit", limit.to_string())]
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn count(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn flag(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

/// Artwork URL at a size matching the largest display (120px cards, with
/// margin for high-density screens).
fn image(value: &Value, key: &str) -> String {
    let Some(image) = value.get(key) else {
        return String::new();
    };
    ["480x480", "150x150", "1000x1000"]
        .iter()
        .find_map(|size| image.get(*size).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn artist_name(user: &Value) -> String {
    let name = text(user, "name");
    if name.trim().is_empty() {
        text(user, "handle")
    } else {
        name
    }
}

fn handle_label(user: &Value) -> String {
    let handle = text(user, "handle");
    if handle.is_empty() {
        String::new()
    } else {
        format!("@{handle}")
    }
}

fn artist_page_url(handle: &str) -> String {
    if handle.is_empty() {
        String::new()
    } else {
        format!("https://audius.co/{handle}")
    }
}

fn permalink_url(permalink: &str) -> String {
    if permalink.is_empty() {
        String::new()
    } else {
        format!("https://audius.co{permalink}")
    }
}

fn year(value: &Value) -> String {
    text(value, "release_date")
        .get(..4)
        .unwrap_or_default()
        .to_string()
}

fn stream_endpoint(id: &str) -> String {
    format!("{API_BASE}/tracks/{id}/stream?app_name={APP_NAME}")
}

fn album_badge(playlist: &Value) -> &'static str {
    if count(playlist, "track_count") <= 1 {
        "Single"
    } else {
        "Album"
    }
}

fn is_playable(track: &Value) -> bool {
    if flag(track, "is_delete") == Some(true) || flag(track, "is_available") == Some(false) {
        return false;
    }
    flag(track, "is_streamable") == Some(true)
        || track.get("stream").is_some_and(|stream| {
            stream
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|url| !url.is_empty())
        })
}

fn album_from_backlink(track: &Value) -> Option<TrackAlbum> {
    let album = track.get("album_backlink")?;
    let name = text(album, "playlist_name");
    let id = count(album, "playlist_id");
    (!name.is_empty() && id > 0).then(|| TrackAlbum {
        name,
        id: id.to_string(),
    })
}

fn track_from_json(track: &Value) -> Option<Track> {
    if !is_playable(track) {
        return None;
    }
    let id = text(track, "id");
    let title = text(track, "title");
    if id.is_empty() || title.is_empty() {
        return None;
    }
    let user = track.get("user").unwrap_or(&Value::Null);
    Some(Track::from_provider_artists_with_count(
        ProviderId::Audius,
        id.clone(),
        stream_endpoint(&id),
        title,
        vec![artist_name(user)],
        count(track, "duration") as u32,
        image(track, "artwork"),
        album_from_backlink(track),
        vec![text(user, "id")],
        count(track, "play_count"),
    ))
}

fn artist_card(user: &Value) -> CardData {
    CardData::new(
        LibraryKind::Artist,
        ProviderId::Audius,
        text(user, "id"),
        artist_name(user),
        handle_label(user),
        image(user, "profile_picture"),
        artist_page_url(&text(user, "handle")),
    )
}

fn playlist_card(playlist: &Value, kind: LibraryKind) -> CardData {
    CardData::new(
        kind,
        ProviderId::Audius,
        text(playlist, "id"),
        text(playlist, "playlist_name"),
        artist_name(playlist.get("user").unwrap_or(&Value::Null)),
        image(playlist, "artwork"),
        permalink_url(&text(playlist, "permalink")),
    )
}

fn album_card(playlist: &Value) -> ArtistAlbumCard {
    ArtistAlbumCard::new(
        ProviderId::Audius,
        text(playlist, "id"),
        text(playlist, "playlist_name"),
        album_badge(playlist),
        year(playlist),
        image(playlist, "artwork"),
        permalink_url(&text(playlist, "permalink")),
    )
}

fn related_card(user: &Value) -> RelatedArtistCard {
    RelatedArtistCard::new(
        ProviderId::Audius,
        text(user, "id"),
        artist_name(user),
        crate::util::format_count(count(user, "follower_count")),
        image(user, "profile_picture"),
        artist_page_url(&text(user, "handle")),
    )
}

fn search_tracks(query: &str, offset: usize) -> Result<Vec<Track>> {
    Ok(items("/tracks/search", &query_page(query, offset))?
        .iter()
        .filter_map(track_from_json)
        .collect())
}

fn trending_tracks(limit: usize) -> Result<Vec<Track>> {
    Ok(items(
        "/tracks/trending",
        &[("limit", limit.to_string()), ("time", "week".to_string())],
    )?
    .iter()
    .filter_map(track_from_json)
    .collect())
}

fn playlist_results(query: &str, offset: usize, album: bool) -> Result<Vec<CardData>> {
    let (key, kind) = if album {
        ("albums", LibraryKind::Album)
    } else {
        ("playlists", LibraryKind::Playlist)
    };
    Ok(card_section(query, offset, key)?
        .iter()
        .map(|playlist| playlist_card(playlist, kind))
        .collect())
}

fn artist_results(query: &str, offset: usize) -> Result<Vec<CardData>> {
    Ok(card_section(query, offset, "users")?
        .iter()
        .map(artist_card)
        .collect())
}

/// One card category of Audius' combined search (users, albums, playlists).
fn card_section(query: &str, offset: usize, key: &str) -> Result<Vec<Value>> {
    let data = data("/full/search/full", &query_page(query, offset))?;
    Ok(data
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

pub fn search(query: &str, scope: SearchScope, offset: usize) -> Result<(Vec<Track>, SearchTab)> {
    let blank = query.trim().is_empty();
    match scope {
        SearchScope::Songs | SearchScope::Videos => {
            let tracks = if blank {
                trending_tracks(SEARCH_PAGE_SIZE)?
            } else {
                search_tracks(query, offset)?
            };
            Ok((tracks, SearchTab::Songs))
        }
        SearchScope::Artists => {
            let cards = if blank {
                Vec::new()
            } else {
                artist_results(query, offset)?
            };
            Ok((Vec::new(), SearchTab::Artists(cards)))
        }
        SearchScope::Albums => {
            let cards = if blank {
                Vec::new()
            } else {
                playlist_results(query, offset, true)?
            };
            Ok((Vec::new(), SearchTab::Albums(cards)))
        }
        SearchScope::Playlists => {
            let cards = if blank {
                Vec::new()
            } else {
                playlist_results(query, offset, false)?
            };
            Ok((Vec::new(), SearchTab::Playlists(cards)))
        }
    }
}

pub fn search_more(query: &str, offset: usize) -> Result<Vec<Track>> {
    search_tracks(query, offset)
}

pub fn browse(id: &str, kind: &str) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    let path = if kind == "artist" {
        format!("/users/{id}/tracks")
    } else {
        format!("/playlists/{id}/tracks")
    };
    let tracks = items(&path, &limit(BROWSE_LIMIT))?
        .iter()
        .filter_map(track_from_json)
        .collect();
    let meta = if kind == "artist" {
        None
    } else {
        album_meta(id)?
    };
    Ok((tracks, meta))
}

fn album_meta(id: &str) -> Result<Option<AlbumMeta>> {
    let Some(playlist) = items(&format!("/playlists/{id}"), &[])?.into_iter().next() else {
        return Ok(None);
    };
    let badge = if flag(&playlist, "is_album") == Some(true) {
        album_badge(&playlist).to_string()
    } else {
        String::new()
    };
    Ok(Some(AlbumMeta {
        badge,
        date: year(&playlist),
        thumbnail: image(&playlist, "artwork"),
    }))
}

fn artist_header(id: &str) -> Result<ArtistHeader> {
    let user = object(&format!("/users/{id}"), &[])?;
    let mut stats = vec![
        (
            "Followers".to_string(),
            crate::util::format_count(count(&user, "follower_count")),
        ),
        (
            "Tracks".to_string(),
            crate::util::format_count(count(&user, "track_count")),
        ),
    ];
    let location = text(&user, "location");
    if !location.is_empty() {
        stats.push(("Location".to_string(), location));
    }
    Ok(ArtistHeader {
        image: image(&user, "profile_picture"),
        stats,
        description: text(&user, "bio"),
    })
}

fn artist_tracks(id: &str) -> Result<Vec<Track>> {
    Ok(items(
        &format!("/users/{id}/tracks"),
        &[
            ("limit", BROWSE_LIMIT.to_string()),
            ("sort", "plays".to_string()),
        ],
    )?
    .iter()
    .filter_map(track_from_json)
    .collect())
}

fn artist_albums(id: &str) -> Result<Vec<ArtistAlbumCard>> {
    Ok(items(&format!("/users/{id}/albums"), &limit(BROWSE_LIMIT))?
        .iter()
        .map(album_card)
        .collect())
}

fn artist_playlists(id: &str) -> Result<Vec<CardData>> {
    Ok(
        items(&format!("/users/{id}/playlists"), &limit(BROWSE_LIMIT))?
            .iter()
            .map(|playlist| playlist_card(playlist, LibraryKind::Playlist))
            .collect(),
    )
}

fn related_artists(id: &str) -> Result<Vec<RelatedArtistCard>> {
    Ok(items(&format!("/users/{id}/related"), &limit(10))?
        .iter()
        .map(related_card)
        .collect())
}

pub fn fetch_artist_page(id: &str, kinds: &[ArtistDataKind]) -> Result<ArtistPage> {
    use ArtistDataKind as Kind;

    let mut page = ArtistPage::default();
    if Kind::Header.wanted(kinds) {
        page.header = Some(artist_header(id)?);
    }
    if Kind::Popular.wanted(kinds) {
        page.popular = artist_tracks(id)?;
    }
    if Kind::Albums.wanted(kinds) {
        page.albums = artist_albums(id)?;
    }
    if Kind::Playlists.wanted(kinds) {
        page.playlists = artist_playlists(id)?;
    }
    if Kind::Related.wanted(kinds) {
        page.related = related_artists(id)?;
    }
    Ok(page)
}

pub fn resolve_artist_id(name: &str) -> Result<Option<String>> {
    Ok(items("/users/search", &query_page(name, 0))?
        .first()
        .map(|user| text(user, "id")))
}

pub fn resolve_id(track: &Track) -> Result<Option<Track>> {
    Ok(search_tracks(&track.search_query(), 0)?.into_iter().next())
}

/// Home row: Audius' weekly trending chart.
pub fn home_sections() -> Result<Vec<crate::providers::HomeSection>> {
    let tracks = trending_tracks(20)?;
    Ok(super::home::provider_sections(
        ProviderId::Audius,
        [(
            super::home::HomeSectionKind::Trending,
            super::home::HomeSectionData::Tracks(tracks),
        )],
    ))
}

pub fn download(track: &Track, download_dir: &std::path::Path) -> Result<String> {
    let id = track.provider_id(ProviderId::Audius).unwrap_or("download");
    let output_path = super::download_file_path(download_dir, id);
    // A plain (default-config) request, not the shared JSON agent: that agent's
    // 30-second global timeout aborts long tracks mid-download.
    super::download_to_file(
        ureq::get(&stream_endpoint(id)).header("User-Agent", USER_AGENT),
        &output_path,
        "Audius",
    )?;
    Ok(output_path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn track_json(id: &str, streamable: bool) -> Value {
        json!({
            "id": id,
            "title": "Song",
            "duration": 123,
            "play_count": 42,
            "is_streamable": streamable,
            "user": {"id": "u1", "name": "Artist", "handle": "artist"},
            "artwork": {"480x480": "https://img/480.jpg"},
            "album_backlink": {"playlist_id": 99, "playlist_name": "Album"},
        })
    }

    #[test]
    fn a_streamable_track_keeps_its_metadata() {
        let track = track_from_json(&track_json("t1", true)).unwrap();
        assert_eq!(track.primary_id(), "t1");
        assert_eq!(
            track.primary_url(),
            "https://api.audius.co/v1/tracks/t1/stream?app_name=goosemusic"
        );
        assert_eq!(track.artist_text(), "Artist");
        assert_eq!(track.duration(), 123);
        assert_eq!(track.play_count(), 42);
        assert_eq!(track.thumbnail(), "https://img/480.jpg");
        assert_eq!(
            track.album().map(|album| album.name.as_str()),
            Some("Album")
        );
        assert_eq!(track.album().map(|album| album.id.as_str()), Some("99"));
        assert_eq!(track.artist_target(0), Some(("u1", ProviderId::Audius)));
    }

    #[test]
    fn unstreamable_and_deleted_tracks_are_dropped() {
        assert!(track_from_json(&track_json("t1", false)).is_none());
        let mut deleted = track_json("t2", true);
        deleted["is_delete"] = json!(true);
        assert!(track_from_json(&deleted).is_none());
    }

    #[test]
    fn cards_carry_their_public_page() {
        let user = json!({
            "id": "u1",
            "name": "Artist",
            "handle": "artist",
            "profile_picture": {"480x480": "https://img/p.jpg"},
            "follower_count": 1234,
        });
        let card = artist_card(&user);
        assert_eq!(card.card.id, "u1");
        assert_eq!(card.subtitle, "@artist");
        assert_eq!(card.card.url, "https://audius.co/artist");

        let playlist = json!({
            "id": "p1",
            "playlist_name": "Mix",
            "permalink": "/artist/playlist/mix-1",
            "track_count": 5,
            "artwork": {"480x480": "https://img/a.jpg"},
            "user": {"name": "Artist"},
        });
        let card = playlist_card(&playlist, LibraryKind::Playlist);
        assert_eq!(card.card.id, "p1");
        assert_eq!(card.subtitle, "Artist");
        assert_eq!(card.card.url, "https://audius.co/artist/playlist/mix-1");
    }

    #[test]
    fn an_album_card_names_its_release_year() {
        let playlist = json!({
            "id": "p1",
            "playlist_name": "Record",
            "track_count": 8,
            "release_date": "2026-09-29T00:30:00Z",
            "permalink": "/artist/album/record",
        });
        let card = album_card(&playlist);
        assert_eq!(card.badge, "Album");
        assert_eq!(card.date, "2026");
    }
}
