//! Jellyfin backend for user-configured servers.
//!
//! A custom provider of kind [`CustomProviderKind::Jellyfin`] authenticates
//! with a username and password, then uses the returned access token for the
//! catalog, artwork, streaming and download endpoints. The token is kept in a
//! process-wide cache so every catalog call does not create a new session.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::{
    data::custom_providers::{self, CustomProvider, CustomProviderKind},
    providers::{
        download_file_path, http_agent, AlbumMeta, ArtistAlbumCard, ArtistDataKind, ArtistHeader,
        ArtistPage, CardData, PlaylistWrite, ProviderId, RelatedArtistCard, SearchScope, SearchTab,
        ServerPlaylist,
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
    /// Playlist-entry id, present only on items returned by
    /// `/Playlists/{id}/Items`. Removing or moving a playlist entry needs this
    /// id, not the song's.
    playlist_item_id: Option<String>,
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

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct PlaylistCreationResult {
    id: Option<String>,
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

fn post_json<T: serde::de::DeserializeOwned>(
    config: &CustomProvider,
    session: &Session,
    path: &str,
) -> Result<T> {
    let url = format!("{}{path}", base_url(config));
    let mut response = http_agent()
        .post(&url)
        .header("Authorization", &token_header(session))
        .header("Accept", "application/json")
        .send_empty()
        .with_context(|| format!("request to {url} failed"))?;
    response
        .body_mut()
        .read_json()
        .with_context(|| format!("Jellyfin response from {url} was unreadable"))
}

fn post_empty(config: &CustomProvider, session: &Session, path: &str) -> Result<()> {
    let url = format!("{}{path}", base_url(config));
    http_agent()
        .post(&url)
        .header("Authorization", &token_header(session))
        .send_empty()
        .with_context(|| format!("request to {url} failed"))?;
    Ok(())
}

fn delete_resource(config: &CustomProvider, session: &Session, path: &str) -> Result<()> {
    let url = format!("{}{path}", base_url(config));
    http_agent()
        .delete(&url)
        .header("Authorization", &token_header(session))
        .call()
        .with_context(|| format!("request to {url} failed"))?;
    Ok(())
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

/// Walk a paged endpoint until a short page arrives or [`MAX_CONTENT_ITEMS`]
/// is reached. Album and playlist contents render as one scrollable list, so a
/// `Limit`-capped single request would silently drop everything past it.
fn walk_pages(mut page: impl FnMut(usize) -> Result<Vec<ItemDto>>) -> Result<Vec<ItemDto>> {
    let mut items = Vec::new();
    loop {
        let next = page(items.len())?;
        let received = next.len();
        items.extend(next);
        if received < CONTENT_PAGE || items.len() >= MAX_CONTENT_ITEMS {
            return Ok(items);
        }
    }
}

fn fetch_all_items(
    config: &CustomProvider,
    session: &Session,
    include_types: &str,
    search_term: &str,
    extra: &[(&str, &str)],
) -> Result<Vec<ItemDto>> {
    walk_pages(|start| {
        fetch_items(
            config,
            session,
            include_types,
            search_term,
            start,
            CONTENT_PAGE,
            extra,
        )
    })
}

fn fetch_playlist_items(
    config: &CustomProvider,
    session: &Session,
    playlist_id: &str,
) -> Result<Vec<ItemDto>> {
    walk_pages(|start| {
        let path = format!(
            "/Playlists/{}/Items?userId={}&Limit={CONTENT_PAGE}&StartIndex={start}&Fields={}",
            urlencode(playlist_id),
            urlencode(&session.user_id),
            urlencode(FIELDS),
        );
        Ok(get_json::<ItemQueryResult>(config, session, &path)?.items)
    })
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

/// The item's own artwork, empty when it has none. Callers that want the album
/// fallback use [`image_url`].
fn own_image(config: &CustomProvider, session: &Session, item: &ItemDto) -> String {
    if has_primary_image(item) {
        image_url_for_id(config, session, &item.id)
    } else {
        String::new()
    }
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
        thumbnail: own_image(config, session, item),
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
        thumbnail: own_image(config, session, item),
        url: String::new(),
    }
}

/// Home rows: the server's recently added, recently played, most played and
/// favourite albums, a random album sample, and its artists. A failing list
/// just drops its row.
pub fn home_sections(provider: ProviderId) -> Result<Vec<crate::providers::HomeSection>> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let mut rows = Vec::new();
    for (kind, extra) in [
        (
            crate::providers::HomeSectionKind::RecentlyAdded,
            &[("SortBy", "DateCreated"), ("SortOrder", "Descending")][..],
        ),
        (
            crate::providers::HomeSectionKind::RecentlyPlayed,
            &[
                ("Filters", "IsPlayed"),
                ("SortBy", "DatePlayed"),
                ("SortOrder", "Descending"),
            ][..],
        ),
        (
            crate::providers::HomeSectionKind::MostPlayed,
            &[
                ("SortBy", "PlayCount"),
                ("SortOrder", "Descending"),
                ("Filters", "IsPlayed"),
            ][..],
        ),
        (
            crate::providers::HomeSectionKind::Starred,
            &[("Filters", "IsFavorite")][..],
        ),
        (
            crate::providers::HomeSectionKind::Discover,
            &[("SortBy", "Random")][..],
        ),
    ] {
        let Ok(items) = fetch_items(&config, &session, "MusicAlbum", "", 0, 20, extra) else {
            continue;
        };
        let cards = items
            .iter()
            .map(|item| {
                crate::providers::HomeCard::new(
                    crate::data::library::LibraryKind::Album,
                    provider,
                    album_card(&config, &session, item),
                )
            })
            .collect();
        rows.push((kind, crate::providers::HomeSectionData::Cards(cards)));
    }

    let Ok(artists) = fetch_items(
        &config,
        &session,
        "MusicArtist",
        "",
        0,
        20,
        &[("SortBy", "Random")],
    ) else {
        return Ok(super::home::provider_sections(provider, rows));
    };
    let cards = artists
        .iter()
        .map(|item| {
            crate::providers::HomeCard::new(
                crate::data::library::LibraryKind::Artist,
                provider,
                artist_card(&config, &session, item),
            )
        })
        .collect();
    rows.push((
        crate::providers::HomeSectionKind::Artists,
        crate::providers::HomeSectionData::Cards(cards),
    ));
    Ok(super::home::provider_sections(provider, rows))
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
        thumbnail: own_image(config, session, item),
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
    // A blank query is a browse and never paginates.
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(search(provider, query, SearchScope::Songs, offset)?.0)
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
                thumbnail: own_image(&config, &session, &details),
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

/// Every playlist on the server, for the sidebar index. `ChildCount` is the
/// server's own count, so the row needs no contents fetch.
pub fn list_playlists(provider: ProviderId) -> Result<Vec<ServerPlaylist>> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let items = fetch_all_items(&config, &session, "Playlist", "", &[])?;
    Ok(items
        .iter()
        .map(|item| ServerPlaylist {
            id: item.id.clone(),
            name: item.name.clone(),
            track_count: item.child_count.unwrap_or(0) as usize,
        })
        .collect())
}

/// Append tracks to an existing playlist. Tracks that carry no id on this
/// server are reported as skipped rather than silently dropped.
pub fn add_to_playlist(
    provider: ProviderId,
    playlist_id: &str,
    tracks: &[Track],
) -> Result<PlaylistWrite> {
    let (ids, skipped) = super::playlist_track_ids(provider, tracks);
    if ids.is_empty() {
        return Ok(PlaylistWrite { added: 0, skipped });
    }
    let config = configured(provider)?;
    let session = session(&config)?;
    let path = format!(
        "/Playlists/{}/Items?ids={}&userId={}",
        urlencode(playlist_id),
        urlencode(&ids.join(",")),
        urlencode(&session.user_id),
    );
    post_empty(&config, &session, &path)?;
    Ok(PlaylistWrite {
        added: ids.len(),
        skipped,
    })
}

/// Remove the entries at `positions` (0-based, as the caller last saw them).
/// The playlist is refetched first: removal addresses playlist *entry* ids,
/// which only that response carries.
pub fn remove_from_playlist(
    provider: ProviderId,
    playlist_id: &str,
    positions: &[usize],
) -> Result<usize> {
    let config = configured(provider)?;
    let session = session(&config)?;
    let items = fetch_playlist_items(&config, &session, playlist_id)?;
    let mut entry_ids: Vec<&str> = positions
        .iter()
        .filter_map(|&position| items.get(position))
        .filter_map(|item| item.playlist_item_id.as_deref())
        .collect();
    entry_ids.sort_unstable();
    entry_ids.dedup();
    if entry_ids.is_empty() {
        return Ok(0);
    }
    let removed = entry_ids.len();
    let path = format!(
        "/Playlists/{}/Items?entryIds={}",
        urlencode(playlist_id),
        urlencode(&entry_ids.join(",")),
    );
    delete_resource(&config, &session, &path)?;
    Ok(removed)
}

/// Rewrite the playlist's entry order. Jellyfin moves one entry per call, so
/// the entries already in place are left alone and only the ones that have to
/// change positions are moved (see [`move_plan`]).
pub fn reorder_playlist(provider: ProviderId, playlist_id: &str, tracks: &[Track]) -> Result<()> {
    let (ids, skipped) = super::playlist_track_ids(provider, tracks);
    if skipped > 0 {
        anyhow::bail!("the playlist has tracks that are not on this server");
    }
    let config = configured(provider)?;
    let session = session(&config)?;
    let items = fetch_playlist_items(&config, &session, playlist_id)?;
    let entries: Vec<(&str, &str)> = items
        .iter()
        .filter_map(|item| {
            let entry = item.playlist_item_id.as_deref()?;
            Some((entry, item.id.as_str()))
        })
        .collect();
    anyhow::ensure!(
        entries.len() == ids.len(),
        "the playlist changed on the server"
    );
    // A track can appear twice in a playlist, so each wanted id claims a
    // distinct entry: the earliest one holding it that is still free.
    let mut claimed = vec![false; entries.len()];
    let mut order: Vec<&str> = Vec::with_capacity(ids.len());
    for id in &ids {
        let position = entries
            .iter()
            .enumerate()
            .find(|(index, (_, track))| !claimed[*index] && *track == id)
            .map(|(index, _)| index)
            .context("the playlist changed on the server")?;
        claimed[position] = true;
        order.push(entries[position].0);
    }
    let current: Vec<&str> = entries.iter().map(|(entry, _)| *entry).collect();
    for index in move_plan(&current, &order) {
        move_playlist_entry(&config, &session, playlist_id, order[index], index)?;
    }
    Ok(())
}

/// The moves that turn `current` into `order`, as indices into `order`: each
/// listed entry is moved to that index. Walking left to right settles every
/// position before touching the next (the server removes then inserts, which
/// leaves the already-settled rows untouched), so only entries whose position
/// is really wrong cost a call.
fn move_plan(current: &[&str], order: &[&str]) -> Vec<usize> {
    let mut simulated: Vec<&str> = current.to_vec();
    let mut moves = Vec::new();
    for (index, entry) in order.iter().enumerate() {
        if simulated.get(index) == Some(entry) {
            continue;
        }
        let Some(position) = simulated.iter().position(|e| e == entry) else {
            continue;
        };
        moves.push(index);
        let entry = simulated.remove(position);
        simulated.insert(index, entry);
    }
    moves
}

fn move_playlist_entry(
    config: &CustomProvider,
    session: &Session,
    playlist_id: &str,
    entry_id: &str,
    index: usize,
) -> Result<()> {
    post_empty(
        config,
        session,
        &format!(
            "/Playlists/{}/Items/{}/Move/{index}",
            urlencode(playlist_id),
            urlencode(entry_id),
        ),
    )
}

pub fn create_playlist(
    provider: ProviderId,
    name: &str,
    tracks: &[Track],
) -> Result<ServerPlaylist> {
    let (ids, _skipped) = super::playlist_track_ids(provider, tracks);
    let config = configured(provider)?;
    let session = session(&config)?;
    let mut path = format!(
        "/Playlists?name={}&userId={}&mediaType=Audio",
        urlencode(name),
        urlencode(&session.user_id),
    );
    if !ids.is_empty() {
        path.push_str("&ids=");
        path.push_str(&urlencode(&ids.join(",")));
    }
    let created: PlaylistCreationResult = post_json(&config, &session, &path)?;
    let id = created
        .id
        .filter(|id| !id.is_empty())
        .context("Jellyfin did not return the new playlist id")?;
    Ok(ServerPlaylist {
        id,
        name: name.to_string(),
        track_count: ids.len(),
    })
}

pub fn rename_playlist(provider: ProviderId, playlist_id: &str, name: &str) -> Result<()> {
    let config = configured(provider)?;
    let session = session(&config)?;
    // The item update endpoint replaces the whole DTO, so read the current one
    // and change just the name instead of posting our partial view of it.
    let mut item: serde_json::Value = get_json(
        &config,
        &session,
        &format!(
            "/Items/{}?userId={}&Fields={}",
            urlencode(playlist_id),
            urlencode(&session.user_id),
            urlencode(FIELDS),
        ),
    )?;
    let object = item
        .as_object_mut()
        .context("Jellyfin playlist details were not an object")?;
    object.insert(
        "Name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    let url = format!("{}/Items/{}", base_url(&config), urlencode(playlist_id));
    http_agent()
        .post(&url)
        .header("Authorization", &token_header(&session))
        .header("Content-Type", "application/json")
        .send_json(item)
        .with_context(|| format!("request to {url} failed"))?;
    Ok(())
}

pub fn delete_playlist(provider: ProviderId, playlist_id: &str) -> Result<()> {
    let config = configured(provider)?;
    let session = session(&config)?;
    delete_resource(
        &config,
        &session,
        &format!("/Items/{}", urlencode(playlist_id)),
    )
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
    super::download_to_file(ureq::get(&url), &output_path, "Jellyfin")?;
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
        || details
            .as_ref()
            .is_some_and(|details| !has_primary_image(details))
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
                thumbnail: own_image(&config, &session, item),
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
                thumbnail: own_image(&config, &session, item),
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

    fn applied(current: &[&str], order: &[&str]) -> Vec<String> {
        let mut simulated: Vec<&str> = current.to_vec();
        for index in move_plan(current, order) {
            let position = simulated
                .iter()
                .position(|e| *e == order[index])
                .expect("planned entry is present");
            let moved = simulated.remove(position);
            simulated.insert(index, moved);
        }
        simulated.into_iter().map(str::to_string).collect()
    }

    #[test]
    fn move_plan_reaches_the_wanted_order() {
        let cases: [(&[&str], &[&str]); 5] = [
            (&["a", "b", "c"], &["c", "b", "a"]),
            (&["a", "b", "c", "d", "e"], &["a", "c", "e", "b", "d"]),
            (&["a", "b", "c", "d", "e"], &["d", "e", "a", "b", "c"]),
            (
                &["a", "b", "c", "d", "e", "f"],
                &["a", "c", "b", "e", "d", "f"],
            ),
            (&["a", "b", "c"], &["a", "b", "c"]),
        ];
        for (current, order) in cases {
            let want: Vec<String> = order.iter().map(std::string::ToString::to_string).collect();
            assert_eq!(applied(current, order), want, "current {current:?}");
        }
    }

    #[test]
    fn move_plan_leaves_matching_entries_alone() {
        assert!(move_plan(&["a", "b", "c"], &["a", "b", "c"]).is_empty());
        assert_eq!(move_plan(&["a", "b", "c"], &["b", "c", "a"]), [0, 1]);
        assert_eq!(move_plan(&["a", "b", "c"], &["b", "a", "c"]), [0]);
    }
}
