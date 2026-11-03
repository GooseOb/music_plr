//! Jellyfin backend for user-configured servers.
//!
//! A custom provider of kind [`CustomProviderKind::Jellyfin`] authenticates
//! with a username and password, then uses the returned access token for the
//! catalog, artwork, streaming and download endpoints. The token is kept in a
//! process-wide cache so every catalog call does not create a new session.

use std::{
    collections::HashMap,
    io::Write as _,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::{
    data::custom_providers::{self, CustomProvider, CustomProviderKind},
    providers::{
        download_file_path, http_agent, AlbumMeta, ArtistAlbumCard, ArtistDataKind, ArtistHeader,
        ArtistPage, CardData, ProviderId, RelatedArtistCard, SearchScope, SearchTab,
    },
    types::{Track, TrackAlbum},
    util::urlencode,
};

const PAGE_SIZE: usize = 10;
const THUMB_SIZE: u32 = 300;
/// Artwork size for the artist page header, which is drawn as a page hero
/// rather than as a card thumbnail.
const HEADER_SIZE: u32 = 800;
/// Server-side page size while walking a full album/playlist. Jellyfin caps
/// `Limit`, so a list longer than one page needs the `StartIndex` loop.
const CONTENT_PAGE: usize = 500;
/// Defensive ceiling for a content walk, so a server that ignores
/// `StartIndex` (and replays page one forever) cannot spin here.
const MAX_CONTENT_ITEMS: usize = 20_000;
const RADIO_SIZE: usize = 50;
const FIELDS: &str = "Album,AlbumId,AlbumArtist,AlbumArtists,Artists,ArtistItems,RunTimeTicks,PrimaryImageAspectRatio,ProductionYear,ChildCount,Overview,UserData,AlbumCount,SongCount,RecursiveItemCount";

#[derive(Clone)]
struct Session {
    token: String,
    user_id: String,
}

#[derive(Clone)]
struct CachedSession {
    signature: String,
    session: Session,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthResponse {
    access_token: Option<String>,
    user: Option<AuthUser>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthUser {
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PublicInfo {
    #[serde(alias = "version", alias = "Version")]
    version: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ItemDto {
    id: String,
    name: String,
    run_time_ticks: Option<i64>,
    production_year: Option<i32>,
    album: Option<String>,
    album_id: Option<String>,
    artists: Option<Vec<String>>,
    artist_items: Option<Vec<NameId>>,
    album_artist: Option<String>,
    album_artists: Option<Vec<NameId>>,
    image_tags: Option<HashMap<String, String>>,
    album_primary_image_tag: Option<String>,
    user_data: Option<UserData>,
    child_count: Option<u32>,
    song_count: Option<u32>,
    album_count: Option<u32>,
    overview: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct NameId {
    id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct UserData {
    play_count: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ItemQueryResult {
    items: Vec<ItemDto>,
}

fn configured(provider: ProviderId) -> Result<CustomProvider> {
    let ProviderId::Custom(CustomProviderKind::Jellyfin, id) = provider else {
        anyhow::bail!("not a custom Jellyfin provider");
    };
    custom_providers::get(id)
        .filter(|config| config.kind == CustomProviderKind::Jellyfin)
        .with_context(|| format!("Jellyfin provider {id} is not configured"))
}

fn base_url(config: &CustomProvider) -> String {
    config.server_url.trim_end_matches('/').to_string()
}

fn auth_header() -> String {
    format!(
        "MediaBrowser Client=\"goosemusic\", Device=\"goosemusic\", DeviceId=\"goosemusic\", Version=\"{}\"",
        env!("CARGO_PKG_VERSION")
    )
}

fn token_header(session: &Session) -> String {
    format!("MediaBrowser Token=\"{}\"", session.token)
}

fn authenticate(config: &CustomProvider) -> Result<Session> {
    let url = format!("{}/Users/AuthenticateByName", base_url(config));
    let mut response = http_agent()
        .post(&url)
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "Username": config.username,
            "Pw": config.password,
        }))
        .with_context(|| format!("request to {url} failed"))?;
    let parsed: AuthResponse = response
        .body_mut()
        .read_json()
        .context("Jellyfin authentication response was unreadable")?;
    let token = parsed
        .access_token
        .filter(|token| !token.is_empty())
        .context("Jellyfin did not return an access token")?;
    let user_id = parsed
        .user
        .and_then(|user| user.id)
        .context("Jellyfin did not return a user id")?;
    Ok(Session { token, user_id })
}

fn session(config: &CustomProvider) -> Result<Session> {
    static SESSIONS: OnceLock<Mutex<HashMap<u64, CachedSession>>> = OnceLock::new();
    let cache = SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let signature = format!(
        "{}\0{}\0{}",
        config.server_url, config.username, config.password
    );
    if let Ok(cache) = cache.lock() {
        if let Some(cached) = cache.get(&config.id) {
            if cached.signature == signature {
                return Ok(cached.session.clone());
            }
        }
    }

    let session = authenticate(config)?;
    if let Ok(mut cache) = cache.lock() {
        cache.insert(
            config.id,
            CachedSession {
                signature,
                session: session.clone(),
            },
        );
    }
    Ok(session)
}

fn get_json<T: serde::de::DeserializeOwned>(
    config: &CustomProvider,
    session: &Session,
    path: &str,
) -> Result<T> {
    let url = format!("{}{path}", base_url(config));
    let mut response = http_agent()
        .get(&url)
        .header("Authorization", &token_header(session))
        .header("Accept", "application/json")
        .call()
        .with_context(|| format!("request to {url} failed"))?;
    response
        .body_mut()
        .read_json()
        .with_context(|| format!("Jellyfin response from {url} was unreadable"))
}

fn item_query(
    session: &Session,
    include_types: &str,
    search_term: &str,
    start: usize,
    limit: usize,
    extra: &[(&str, &str)],
) -> String {
    let mut path = format!(
        "/Items?userId={}&Recursive=true&IncludeItemTypes={}&Limit={limit}&StartIndex={start}&Fields={}",
        urlencode(&session.user_id),
        urlencode(include_types),
        urlencode(FIELDS),
    );
    if !search_term.is_empty() {
        path.push_str("&SearchTerm=");
        path.push_str(&urlencode(search_term));
    }
    for (key, value) in extra {
        path.push('&');
        path.push_str(key);
        path.push('=');
        path.push_str(&urlencode(value));
    }
    path
}

fn fetch_items(
    config: &CustomProvider,
    session: &Session,
    include_types: &str,
    search_term: &str,
    start: usize,
    limit: usize,
    extra: &[(&str, &str)],
) -> Result<Vec<ItemDto>> {
    let path = item_query(session, include_types, search_term, start, limit, extra);
    Ok(get_json::<ItemQueryResult>(config, session, &path)?.items)
}

/// Every item matching a query, walked page by page. Album contents use this:
/// they render as one scrollable list, so a `Limit`-capped single request would
/// silently drop everything past it.
fn fetch_all_items(
    config: &CustomProvider,
    session: &Session,
    include_types: &str,
    search_term: &str,
    extra: &[(&str, &str)],
) -> Result<Vec<ItemDto>> {
    let mut items = Vec::new();
    loop {
        let page = fetch_items(
            config,
            session,
            include_types,
            search_term,
            items.len(),
            CONTENT_PAGE,
            extra,
        )?;
        let received = page.len();
        items.extend(page);
        if received < CONTENT_PAGE || items.len() >= MAX_CONTENT_ITEMS {
            return Ok(items);
        }
    }
}

/// Every track of a playlist, walked page by page (see [`fetch_all_items`]).
fn fetch_playlist_items(
    config: &CustomProvider,
    session: &Session,
    playlist_id: &str,
) -> Result<Vec<ItemDto>> {
    let mut items = Vec::new();
    loop {
        let path = format!(
            "/Playlists/{}/Items?userId={}&Limit={CONTENT_PAGE}&StartIndex={}&Fields={}",
            urlencode(playlist_id),
            urlencode(&session.user_id),
            items.len(),
            urlencode(FIELDS),
        );
        let page = get_json::<ItemQueryResult>(config, session, &path)?.items;
        let received = page.len();
        items.extend(page);
        if received < CONTENT_PAGE || items.len() >= MAX_CONTENT_ITEMS {
            return Ok(items);
        }
    }
}

fn image_url_for_id(config: &CustomProvider, session: &Session, id: &str) -> String {
    image_url_for_id_sized(config, session, id, THUMB_SIZE)
}

fn image_url_for_id_sized(
    config: &CustomProvider,
    session: &Session,
    id: &str,
    size: u32,
) -> String {
    let token = urlencode(&session.token);
    format!(
        "{}/Items/{}/Images/Primary?maxHeight={size}&quality=90&ApiKey={token}&api_key={token}",
        base_url(config),
        urlencode(id),
    )
}

fn has_primary_image(item: &ItemDto) -> bool {
    item.image_tags
        .as_ref()
        .is_some_and(|tags| tags.contains_key("Primary"))
}

fn image_url(config: &CustomProvider, session: &Session, item: &ItemDto) -> String {
    image_url_with_size(config, session, item, THUMB_SIZE)
}

/// The item's own artwork, or its album's when the item carries none.
fn image_url_with_size(
    config: &CustomProvider,
    session: &Session,
    item: &ItemDto,
    size: u32,
) -> String {
    let id = if has_primary_image(item) {
        Some(item.id.as_str())
    } else if item.album_primary_image_tag.is_some() {
        item.album_id.as_deref()
    } else {
        None
    };
    id.map_or_else(String::new, |id| {
        image_url_for_id_sized(config, session, id, size)
    })
}

fn stream_url(config: &CustomProvider, session: &Session, id: &str) -> String {
    let token = urlencode(&session.token);
    format!(
        "{}/Audio/{}/stream?static=true&ApiKey={token}&api_key={token}",
        base_url(config),
        urlencode(id),
    )
}

fn download_url(config: &CustomProvider, session: &Session, id: &str) -> String {
    let token = urlencode(&session.token);
    format!(
        "{}/Audio/{}/stream.mp3?static=false&ApiKey={token}&api_key={token}",
        base_url(config),
        urlencode(id),
    )
}

fn track_from_item(
    provider: ProviderId,
    config: &CustomProvider,
    session: &Session,
    item: &ItemDto,
) -> Track {
    let album = item.album.clone().map(|name| TrackAlbum {
        name,
        id: item.album_id.clone().unwrap_or_default(),
    });
    let artist = item
        .artists
        .as_ref()
        .and_then(|artists| artists.first())
        .cloned()
        .or_else(|| item.album_artist.clone())
        .unwrap_or_default();
    let artist_id = item
        .artist_items
        .as_ref()
        .and_then(|artists| artists.first())
        .and_then(|artist| artist.id.clone())
        .or_else(|| {
            item.album_artists
                .as_ref()
                .and_then(|artists| artists.first())
                .and_then(|artist| artist.id.clone())
        });
    let duration = item
        .run_time_ticks
        .unwrap_or(0)
        .max(0)
        .checked_div(10_000_000)
        .and_then(|seconds| u32::try_from(seconds).ok())
        .unwrap_or(0);
    let play_count = item
        .user_data
        .as_ref()
        .and_then(|data| data.play_count)
        .unwrap_or(0);
    Track::from_provider_with_count(
        provider,
        item.id.clone(),
        stream_url(config, session, &item.id),
        item.name.clone(),
        artist,
        duration,
        image_url(config, session, item),
        album,
        artist_id,
        play_count,
    )
}

fn artist_card(config: &CustomProvider, session: &Session, item: &ItemDto) -> CardData {
    CardData {
        id: item.id.clone(),
        title: item.name.clone(),
        subtitle: item
            .album_count
            .filter(|count| *count > 0)
            .map(|count| format!("{count} albums"))
            .unwrap_or_default(),
        thumbnail: if has_primary_image(item) {
            image_url_for_id(config, session, &item.id)
        } else {
            String::new()
        },
        url: String::new(),
    }
}

fn album_card(config: &CustomProvider, session: &Session, item: &ItemDto) -> CardData {
    CardData {
        id: item.id.clone(),
        title: item.name.clone(),
        subtitle: item
            .album_artist
            .clone()
            .or_else(|| {
                item.artists
                    .as_ref()
                    .and_then(|artists| artists.first().cloned())
            })
            .unwrap_or_default(),
        thumbnail: if has_primary_image(item) {
            image_url_for_id(config, session, &item.id)
        } else {
            String::new()
        },
        url: String::new(),
    }
}

fn playlist_card(config: &CustomProvider, session: &Session, item: &ItemDto) -> CardData {
    CardData {
        id: item.id.clone(),
        title: item.name.clone(),
        subtitle: item
            .child_count
            .filter(|count| *count > 0)
            .map(|count| format!("{count} songs"))
            .unwrap_or_default(),
        thumbnail: if has_primary_image(item) {
            image_url_for_id(config, session, &item.id)
        } else {
            String::new()
        },
        url: String::new(),
    }
}

/// Playlist cards with an artwork fallback: a Jellyfin playlist usually owns no
/// image of its own, so it borrows its first track's (which itself falls back to
/// the album's). Only the cards still missing art cost a request, and they run
/// in parallel, so a page of them adds one round-trip instead of one per card.
fn playlist_cards(config: &CustomProvider, session: &Session, items: &[ItemDto]) -> Vec<CardData> {
    let mut cards: Vec<CardData> = items
        .iter()
        .map(|item| playlist_card(config, session, item))
        .collect();
    let missing: Vec<usize> = cards
        .iter()
        .enumerate()
        .filter(|(_, card)| card.thumbnail.is_empty())
        .map(|(index, _)| index)
        .collect();
    if missing.is_empty() {
        return cards;
    }

    let found: Vec<(usize, String)> = std::thread::scope(|scope| {
        let handles: Vec<_> = missing
            .iter()
            .map(|&index| {
                let id = items[index].id.clone();
                scope.spawn(move || {
                    (
                        index,
                        first_track_image(config, session, &id).unwrap_or_default(),
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect()
    });
    for (index, thumbnail) in found {
        if thumbnail.is_empty() {
            continue;
        }
        if let Some(card) = cards.get_mut(index) {
            card.thumbnail = thumbnail;
        }
    }
    cards
}

fn first_track_image(
    config: &CustomProvider,
    session: &Session,
    playlist_id: &str,
) -> Result<String> {
    let path = format!(
        "/Playlists/{}/Items?userId={}&Limit=1&Fields={}",
        urlencode(playlist_id),
        urlencode(&session.user_id),
        urlencode(FIELDS),
    );
    let result: ItemQueryResult = get_json(config, session, &path)?;
    Ok(result
        .items
        .first()
        .map(|item| image_url(config, session, item))
        .unwrap_or_default())
}

/// Search songs, artists, albums or playlists. A blank query browses recent
/// songs or the server's newest albums, depending on the scope.
pub fn search(
    provider: ProviderId,
    query: &str,
    scope: SearchScope,
    offset: usize,
) -> Result<(Vec<Track>, SearchTab)> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let query = query.trim();
    let blank = query.is_empty();

    match scope {
        SearchScope::Songs => {
            let extra: &[(&str, &str)] = if blank {
                &[("SortBy", "DateCreated"), ("SortOrder", "Descending")]
            } else {
                &[]
            };
            let items = fetch_items(&config, &session, "Audio", query, offset, PAGE_SIZE, extra)?;
            Ok((
                items
                    .iter()
                    .map(|item| track_from_item(provider, &config, &session, item))
                    .collect(),
                SearchTab::Songs,
            ))
        }
        SearchScope::Artists => {
            let items = fetch_items(
                &config,
                &session,
                "MusicArtist",
                query,
                offset,
                PAGE_SIZE,
                &[("SortBy", "SortName")],
            )?;
            Ok((
                Vec::new(),
                SearchTab::Artists(
                    items
                        .iter()
                        .map(|item| artist_card(&config, &session, item))
                        .collect(),
                ),
            ))
        }
        SearchScope::Albums => {
            let extra: &[(&str, &str)] = if blank {
                &[("SortBy", "DateCreated"), ("SortOrder", "Descending")]
            } else {
                &[]
            };
            let items = fetch_items(
                &config,
                &session,
                "MusicAlbum",
                query,
                offset,
                PAGE_SIZE,
                extra,
            )?;
            Ok((
                Vec::new(),
                SearchTab::Albums(
                    items
                        .iter()
                        .map(|item| album_card(&config, &session, item))
                        .collect(),
                ),
            ))
        }
        SearchScope::Playlists => {
            let items = fetch_items(
                &config,
                &session,
                "Playlist",
                query,
                offset,
                PAGE_SIZE,
                &[("SortBy", "SortName")],
            )?;
            Ok((
                Vec::new(),
                SearchTab::Playlists(playlist_cards(&config, &session, &items)),
            ))
        }
        SearchScope::Videos => Ok((Vec::new(), SearchTab::Videos)),
    }
}

/// Next page of song results ("Load More").
pub fn search_more(provider: ProviderId, query: &str, offset: usize) -> Result<Vec<Track>> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let items = fetch_items(&config, &session, "Audio", query, offset, PAGE_SIZE, &[])?;
    Ok(items
        .iter()
        .map(|item| track_from_item(provider, &config, &session, item))
        .collect())
}

/// Drill into an album (`kind == "album"`) or a playlist.
pub fn browse(
    provider: ProviderId,
    id: &str,
    kind: &str,
) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    let config = configured(provider)?;
    let session = session(&config)?;
    match kind {
        "album" => {
            let details: ItemDto = get_json(
                &config,
                &session,
                &format!(
                    "/Items/{}?userId={}&Fields={}",
                    urlencode(id),
                    urlencode(&session.user_id),
                    urlencode(FIELDS),
                ),
            )?;
            let items = fetch_all_items(
                &config,
                &session,
                "Audio",
                "",
                &[
                    ("ParentId", id),
                    ("SortBy", "ParentIndexNumber,IndexNumber"),
                ],
            )?;
            let meta = AlbumMeta {
                badge: "Album".to_string(),
                date: details
                    .production_year
                    .map(|year| year.to_string())
                    .unwrap_or_default(),
                thumbnail: if has_primary_image(&details) {
                    image_url_for_id(&config, &session, &details.id)
                } else {
                    String::new()
                },
            };
            Ok((
                items
                    .iter()
                    .map(|item| track_from_item(provider, &config, &session, item))
                    .collect(),
                Some(meta),
            ))
        }
        "playlist" => {
            let items = fetch_playlist_items(&config, &session, id)?;
            Ok((
                items
                    .iter()
                    .map(|item| track_from_item(provider, &config, &session, item))
                    .collect(),
                None,
            ))
        }
        _ => Ok((Vec::new(), None)),
    }
}

pub fn resolve_artist_id(provider: ProviderId, name: &str) -> Result<Option<String>> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let items = fetch_items(
        &config,
        &session,
        "MusicArtist",
        name,
        0,
        1,
        &[("SortBy", "SortName")],
    )?;
    Ok(items.first().map(|item| item.id.clone()))
}

pub fn resolve_id(provider: ProviderId, track: &Track) -> Result<Option<Track>> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let query = if track.title.trim().is_empty() {
        track.search_query()
    } else {
        track.title.clone()
    };
    let items = fetch_items(&config, &session, "Audio", &query, 0, 5, &[])?;
    let item = items
        .iter()
        .find(|item| {
            item_matches_artist(item, &track.artist) && item.name.eq_ignore_ascii_case(&track.title)
        })
        .or_else(|| {
            items
                .iter()
                .find(|item| item_matches_artist(item, &track.artist))
        })
        .or_else(|| items.first());
    Ok(item.map(|item| track_from_item(provider, &config, &session, item)))
}

fn item_matches_artist(item: &ItemDto, artist: &str) -> bool {
    if artist.trim().is_empty() {
        return false;
    }
    item.artists
        .as_ref()
        .is_some_and(|artists| artists.iter().any(|name| name.eq_ignore_ascii_case(artist)))
        || item
            .album_artist
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(artist))
}

fn instant_mix_path(session: &Session, collection: &str, id: &str) -> String {
    format!(
        "/{collection}/{}/InstantMix?userId={}&Limit={RADIO_SIZE}&Fields={}",
        urlencode(id),
        urlencode(&session.user_id),
        urlencode(FIELDS),
    )
}

fn instant_mix(provider: ProviderId, collection: &str, id: &str) -> Result<Vec<Track>> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let path = instant_mix_path(&session, collection, id);
    let result: ItemQueryResult = get_json(&config, &session, &path)?;
    Ok(result
        .items
        .iter()
        .map(|item| track_from_item(provider, &config, &session, item))
        .collect())
}

pub fn radio_song(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    instant_mix(provider, "Songs", id)
}

pub fn radio_artist(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    instant_mix(provider, "Artists", id)
}

pub fn download(
    provider: ProviderId,
    track: &Track,
    download_dir: &std::path::Path,
) -> Result<String> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let id = track
        .provider_id(provider)
        .context("Jellyfin track has no server id")?;
    let url = download_url(&config, &session, id);
    let output_path = download_file_path(download_dir, id);
    let mut response = ureq::get(&url).call().context("Jellyfin download failed")?;
    let mut file = std::fs::File::create(&output_path).context("download file not writable")?;
    std::io::copy(&mut response.body_mut().as_reader(), &mut file)
        .context("Jellyfin download failed")?;
    file.flush().context("download flush failed")?;
    if file.metadata().is_ok_and(|meta| meta.len() == 0) {
        let _ = std::fs::remove_file(&output_path);
        anyhow::bail!("Jellyfin download was empty");
    }
    Ok(output_path.to_string_lossy().into_owned())
}

/// Fetch the requested artist-page sections from one Jellyfin server.
#[allow(clippy::too_many_lines)]
pub fn fetch_artist_page(
    provider: ProviderId,
    id: &str,
    kinds: &[ArtistDataKind],
) -> Result<ArtistPage> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let mut page = ArtistPage::default();

    let wants_header = ArtistDataKind::Header.wanted(kinds);
    let details = if wants_header {
        let path = format!(
            "/Items/{}?userId={}&Fields={}",
            urlencode(id),
            urlencode(&session.user_id),
            urlencode(FIELDS),
        );
        Some(get_json::<ItemDto>(&config, &session, &path)?)
    } else {
        None
    };

    let albums = if ArtistDataKind::Albums.wanted(kinds)
        || (wants_header
            && details
                .as_ref()
                .is_none_or(|details| !has_primary_image(details)))
    {
        let limit = if ArtistDataKind::Albums.wanted(kinds) {
            50
        } else {
            1
        };
        Some(fetch_items(
            &config,
            &session,
            "MusicAlbum",
            "",
            0,
            limit,
            &[("ArtistIds", id), ("SortBy", "ProductionYear,SortName")],
        )?)
    } else {
        None
    };

    if let Some(details) = &details {
        if wants_header {
            let mut image = image_url_with_size(&config, &session, details, HEADER_SIZE);
            if image.is_empty() {
                if let Some(album) = albums.as_ref().and_then(|albums| albums.first()) {
                    image = image_url_with_size(&config, &session, album, HEADER_SIZE);
                }
            }
            page.header = Some(ArtistHeader {
                image,
                stats: Vec::new(),
                description: details.overview.clone().unwrap_or_default(),
            });
        }
    }

    if ArtistDataKind::Popular.wanted(kinds) {
        page.popular = fetch_items(
            &config,
            &session,
            "Audio",
            "",
            0,
            50,
            &[
                ("ArtistIds", id),
                ("SortBy", "PlayCount,SortName"),
                ("SortOrder", "Descending"),
            ],
        )?
        .iter()
        .map(|item| track_from_item(provider, &config, &session, item))
        .collect();
    }
    if ArtistDataKind::Albums.wanted(kinds) {
        page.albums = albums
            .unwrap_or_default()
            .iter()
            .map(|item| ArtistAlbumCard {
                id: item.id.clone(),
                title: item.name.clone(),
                date: item
                    .production_year
                    .map(|year| year.to_string())
                    .unwrap_or_default(),
                badge: "Album".to_string(),
                thumbnail: if has_primary_image(item) {
                    image_url_for_id(&config, &session, &item.id)
                } else {
                    String::new()
                },
                url: String::new(),
            })
            .collect();
    }
    if ArtistDataKind::Playlists.wanted(kinds) {
        page.playlists = playlist_cards(
            &config,
            &session,
            &fetch_items(
                &config,
                &session,
                "Playlist",
                "",
                0,
                20,
                &[("SortBy", "SortName")],
            )?,
        );
    }
    if ArtistDataKind::Related.wanted(kinds) {
        let path = format!(
            "/Artists/{}/Similar?userId={}&Limit=10",
            urlencode(id),
            urlencode(&session.user_id),
        );
        let result: ItemQueryResult = get_json(&config, &session, &path)?;
        page.related = result
            .items
            .iter()
            .map(|item| RelatedArtistCard {
                id: item.id.clone(),
                name: item.name.clone(),
                stat: String::new(),
                thumbnail: if has_primary_image(item) {
                    image_url_for_id(&config, &session, &item.id)
                } else {
                    String::new()
                },
                url: String::new(),
            })
            .collect();
    }

    Ok(page)
}

/// Probe a candidate server configuration (used by the Settings form before
/// saving).
pub fn test_connection(server_url: &str, username: &str, password: &str) -> Result<String> {
    let config = CustomProvider {
        id: 0,
        name: String::new(),
        kind: CustomProviderKind::Jellyfin,
        server_url: server_url.to_string(),
        username: username.to_string(),
        password: password.to_string(),
    };
    authenticate(&config)?;
    let url = format!("{}/System/Info/Public", base_url(&config));
    let mut response = http_agent()
        .get(&url)
        .call()
        .with_context(|| format!("request to {url} failed"))?;
    let info: PublicInfo = response
        .body_mut()
        .read_json()
        .context("Jellyfin server info was unreadable")?;
    Ok(info
        .version
        .filter(|version| !version.is_empty())
        .map_or_else(
            || "Jellyfin".to_string(),
            |version| format!("Jellyfin {version}"),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_query_url_encodes_user_input() {
        let session = Session {
            token: "token".into(),
            user_id: "user".into(),
        };
        let query = item_query(&session, "Audio", "night drive & rain", 10, 5, &[]);
        assert!(query.contains("SearchTerm=night+drive+%26+rain"));
        assert!(query.contains("StartIndex=10"));
    }

    #[test]
    fn instant_mix_path_encodes_the_seed_and_user() {
        let session = Session {
            token: "token".into(),
            user_id: "user".into(),
        };
        let path = instant_mix_path(&session, "Artists", "artist/id");
        assert!(path.starts_with("/Artists/artist%2Fid/InstantMix?"));
        assert!(path.contains("userId=user"));
        assert!(path.contains("Limit=50"));
    }

    #[test]
    fn jellyfin_track_ids_keep_the_kind_in_their_slug() {
        let provider = ProviderId::Custom(CustomProviderKind::Jellyfin, 7);
        assert_eq!(provider.slug().as_ref(), "jellyfin-7");
        assert_eq!(ProviderId::from_slug("jellyfin-7"), Some(provider));
        assert_eq!(serde_json::to_string(&provider).unwrap(), "\"jellyfin-7\"");
    }
}
