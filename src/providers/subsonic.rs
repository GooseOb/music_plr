//! Subsonic / `OpenSubsonic` backend for user-configured servers
//! (`Navidrome`, `Airsonic`, `Gonic`, …). Every custom provider is one
//! `ProviderId::Custom(CustomProviderKind::Subsonic, id)` entry backed by
//! [`custom_providers`].

use std::{io::Write as _, time::Duration};

use anyhow::{Context, Result};
use opensubsonic::{
    AlbumListOptions, AlbumListType, Auth, Client, RandomSongsOptions, Search3Options,
    StreamOptions,
};

use crate::{
    data::custom_providers::{self, CustomProvider, CustomProviderKind},
    providers::{
        download_file_path, shared_runtime, AlbumMeta, ArtistAlbumCard, ArtistDataKind,
        ArtistHeader, ArtistPage, CardData, ErrorKind, ProviderId, RelatedArtistCard, SearchScope,
        SearchTab,
    },
    types::{Track, TrackAlbum},
};

const PAGE_SIZE: i32 = 10;
const THUMB_SIZE: i32 = 300;
const RADIO_SIZE: i32 = 50;

fn configured(provider: ProviderId) -> Result<CustomProvider> {
    let ProviderId::Custom(CustomProviderKind::Subsonic, id) = provider else {
        anyhow::bail!("not a custom Subsonic provider");
    };
    custom_providers::get(id)
        .filter(|config| config.kind == CustomProviderKind::Subsonic)
        .with_context(|| format!("Subsonic provider {id} is not configured"))
}

fn make_client(provider: &CustomProvider) -> Result<Client> {
    Client::new(
        &provider.server_url,
        Auth::token(&provider.username, &provider.password),
    )
    .map(|client| client.with_client_name("goosemusic"))
    .map_err(anyhow::Error::from)
}

fn block_on<T>(
    future: impl std::future::Future<Output = Result<T, opensubsonic::Error>>,
) -> Result<T> {
    shared_runtime()
        .block_on(async move { tokio::time::timeout(Duration::from_mins(1), future).await })
        .map_err(|_| anyhow::anyhow!("Subsonic request timed out"))?
        .map_err(anyhow::Error::from)
}

fn stream_url(client: &Client, id: &str) -> String {
    client
        .stream_url(id, &StreamOptions::new())
        .map(|url| url.to_string())
        .unwrap_or_default()
}

fn cover_url(client: &Client, cover_art: Option<&str>) -> String {
    cover_art
        .and_then(|id| client.cover_art_url(id, Some(THUMB_SIZE)).ok())
        .map(|url| url.to_string())
        .unwrap_or_default()
}

fn track_from_child(
    provider: ProviderId,
    client: &Client,
    child: &opensubsonic::data::Child,
) -> Track {
    let album = child.album.as_ref().map(|name| TrackAlbum {
        name: name.clone(),
        id: child.album_id.clone().unwrap_or_default(),
    });
    Track::from_provider(
        provider,
        child.id.clone(),
        stream_url(client, &child.id),
        child.title.clone(),
        child.artist.clone().unwrap_or_default(),
        child.duration.unwrap_or(0).max(0) as u32,
        cover_url(client, child.cover_art.as_deref()),
        album,
        child.artist_id.clone(),
    )
}

fn artist_card(client: &Client, artist: &opensubsonic::data::ArtistId3) -> CardData {
    CardData {
        id: artist.id.clone(),
        title: artist.name.clone(),
        subtitle: artist
            .album_count
            .map(|count| format!("{count} albums"))
            .unwrap_or_default(),
        thumbnail: cover_url(client, artist.cover_art.as_deref()),
        url: String::new(),
    }
}

fn album_card(client: &Client, album: &opensubsonic::data::AlbumId3) -> CardData {
    CardData {
        id: album.id.clone(),
        title: album.name.clone(),
        subtitle: album.artist.clone().unwrap_or_default(),
        thumbnail: cover_url(client, album.cover_art.as_deref()),
        url: String::new(),
    }
}

fn playlist_card(client: &Client, playlist: &opensubsonic::data::Playlist) -> CardData {
    CardData {
        id: playlist.id.clone(),
        title: playlist.name.clone(),
        subtitle: playlist
            .song_count
            .map(|count| format!("{count} songs"))
            .unwrap_or_default(),
        thumbnail: cover_url(client, playlist.cover_art.as_deref()),
        url: String::new(),
    }
}

fn search_artists(
    client: &Client,
    query: &str,
    offset: i32,
) -> Result<Vec<opensubsonic::data::ArtistId3>> {
    if query.is_empty() {
        return Ok(block_on(client.get_artists(None))?
            .index
            .into_iter()
            .flat_map(|index| index.artist)
            .skip(offset.max(0) as usize)
            .take(PAGE_SIZE as usize)
            .collect());
    }
    Ok(block_on(
        client.search3(
            query,
            &Search3Options::new()
                .artist_count(PAGE_SIZE)
                .artist_offset(offset),
        ),
    )?
    .artist)
}

/// Search songs, artists, albums or playlists. A blank query browses a random
/// sample of the server's library instead.
pub fn search(
    provider: ProviderId,
    query: &str,
    scope: SearchScope,
    offset: usize,
) -> Result<(Vec<Track>, SearchTab)> {
    let config = configured(provider)?;
    let client = make_client(&config)?;
    let query = query.trim();
    let offset = i32::try_from(offset).unwrap_or(i32::MAX);
    let blank = query.is_empty();

    match scope {
        SearchScope::Songs => {
            let songs = if blank {
                block_on(client.get_random_songs(&RandomSongsOptions::new().size(PAGE_SIZE)))?
            } else {
                block_on(
                    client.search3(
                        query,
                        &Search3Options::new()
                            .song_count(PAGE_SIZE)
                            .song_offset(offset),
                    ),
                )?
                .song
            };
            Ok((
                songs
                    .iter()
                    .map(|song| track_from_child(provider, &client, song))
                    .collect(),
                SearchTab::Songs,
            ))
        }
        SearchScope::Artists => {
            let artists = search_artists(&client, query, offset)?;
            Ok((
                Vec::new(),
                SearchTab::Artists(
                    artists
                        .iter()
                        .map(|artist| artist_card(&client, artist))
                        .collect(),
                ),
            ))
        }
        SearchScope::Albums => {
            let albums = if blank {
                block_on(client.get_album_list2(
                    AlbumListType::Newest,
                    &AlbumListOptions::new().size(PAGE_SIZE).offset(offset),
                ))?
            } else {
                block_on(
                    client.search3(
                        query,
                        &Search3Options::new()
                            .album_count(PAGE_SIZE)
                            .album_offset(offset),
                    ),
                )?
                .album
            };
            Ok((
                Vec::new(),
                SearchTab::Albums(
                    albums
                        .iter()
                        .map(|album| album_card(&client, album))
                        .collect(),
                ),
            ))
        }
        SearchScope::Playlists => {
            let playlists = block_on(client.get_playlists(None))?;
            let needle = query.to_lowercase();
            Ok((
                Vec::new(),
                SearchTab::Playlists(
                    playlists
                        .iter()
                        .filter(|playlist| blank || playlist.name.to_lowercase().contains(&needle))
                        .skip(offset.max(0) as usize)
                        .take(PAGE_SIZE as usize)
                        .map(|playlist| playlist_card(&client, playlist))
                        .collect(),
                ),
            ))
        }
        SearchScope::Videos => Ok((Vec::new(), SearchTab::Videos)),
    }
}

/// Next page of song results ("Load More").
pub fn search_more(provider: ProviderId, query: &str, offset: usize) -> Result<Vec<Track>> {
    let config = configured(provider)?;
    let client = make_client(&config)?;
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let result = block_on(
        client.search3(
            query,
            &Search3Options::new()
                .song_count(PAGE_SIZE)
                .song_offset(i32::try_from(offset).unwrap_or(i32::MAX)),
        ),
    )?;
    Ok(result
        .song
        .iter()
        .map(|song| track_from_child(provider, &client, song))
        .collect())
}

/// Drill into an album (`kind == "album"`) or a playlist.
pub fn browse(
    provider: ProviderId,
    id: &str,
    kind: &str,
) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    let config = configured(provider)?;
    let client = make_client(&config)?;
    match kind {
        "album" => {
            let album = block_on(client.get_album(id))?;
            let meta = AlbumMeta {
                badge: "Album".to_string(),
                date: album.year.map(|year| year.to_string()).unwrap_or_default(),
                thumbnail: cover_url(&client, album.cover_art.as_deref()),
            };
            let tracks = album
                .song
                .iter()
                .map(|song| track_from_child(provider, &client, song))
                .collect();
            Ok((tracks, Some(meta)))
        }
        "playlist" => {
            let playlist = block_on(client.get_playlist(id))?;
            let tracks = playlist
                .entry
                .iter()
                .map(|song| track_from_child(provider, &client, song))
                .collect();
            Ok((tracks, None))
        }
        _ => Ok((Vec::new(), None)),
    }
}

pub fn resolve_artist_id(provider: ProviderId, name: &str) -> Result<Option<String>> {
    let config = configured(provider)?;
    let client = make_client(&config)?;
    let result = block_on(client.search3(name, &Search3Options::new().artist_count(1)))?;
    Ok(result.artist.first().map(|artist| artist.id.clone()))
}

pub fn resolve_id(provider: ProviderId, track: &Track) -> Result<Option<Track>> {
    let config = configured(provider)?;
    let client = make_client(&config)?;
    let result =
        block_on(client.search3(&track.search_query(), &Search3Options::new().song_count(5)))?;
    Ok(result
        .song
        .first()
        .map(|song| track_from_child(provider, &client, song)))
}

fn similar_songs(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    let config = configured(provider)?;
    let client = make_client(&config)?;
    Ok(block_on(client.get_similar_songs2(id, Some(RADIO_SIZE)))?
        .iter()
        .map(|song| track_from_child(provider, &client, song))
        .collect())
}

pub fn radio_song(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    similar_songs(provider, id)
}

pub fn radio_artist(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    similar_songs(provider, id)
}

pub fn download(
    provider: ProviderId,
    track: &Track,
    download_dir: &std::path::Path,
) -> Result<String> {
    let config = configured(provider)?;
    let client = make_client(&config)?;
    let id = track
        .provider_id(provider)
        .context("Subsonic track has no server id")?;
    // The saved file is named `.mp3`, so ask the server to transcode instead
    // of storing the original (possibly FLAC/Opus) bytes under that name.
    let url = client
        .stream_url(id, &StreamOptions::new().format("mp3"))
        .map(|url| url.to_string())
        .context("Subsonic track has no stream URL")?;
    let output_path = download_file_path(download_dir, id);
    let mut response = ureq::get(&url).call().context("Subsonic download failed")?;
    let mut file = std::fs::File::create(&output_path).context("download file not writable")?;
    std::io::copy(&mut response.body_mut().as_reader(), &mut file)
        .context("Subsonic download failed")?;
    file.flush().context("download flush failed")?;
    if file.metadata().is_ok_and(|meta| meta.len() == 0) {
        let _ = std::fs::remove_file(&output_path);
        anyhow::bail!("Subsonic download was empty");
    }
    Ok(output_path.to_string_lossy().into_owned())
}

/// Fetch the requested artist-page sections from one Subsonic server.
pub fn fetch_artist_page(
    provider: ProviderId,
    id: &str,
    kinds: &[ArtistDataKind],
) -> Result<ArtistPage> {
    let config = configured(provider)?;
    let client = make_client(&config)?;
    let mut page = ArtistPage::default();

    let needs_artist = ArtistDataKind::Header.wanted(kinds)
        || ArtistDataKind::Popular.wanted(kinds)
        || ArtistDataKind::Albums.wanted(kinds);
    let artist = if needs_artist {
        Some(block_on(client.get_artist(id))?)
    } else {
        None
    };
    let info = if ArtistDataKind::Header.wanted(kinds) || ArtistDataKind::Related.wanted(kinds) {
        block_on(client.get_artist_info2(id, None, Some(false))).ok()
    } else {
        None
    };

    if let Some(artist) = &artist {
        if ArtistDataKind::Header.wanted(kinds) {
            let image = if artist.cover_art.is_some() {
                cover_url(&client, artist.cover_art.as_deref())
            } else {
                info.as_ref()
                    .and_then(|info| {
                        info.large_image_url
                            .clone()
                            .or_else(|| info.medium_image_url.clone())
                            .or_else(|| info.small_image_url.clone())
                    })
                    .unwrap_or_default()
            };
            page.header = Some(ArtistHeader {
                image,
                stats: Vec::new(),
                description: info
                    .as_ref()
                    .and_then(|info| info.biography.clone())
                    .unwrap_or_default(),
            });
        }
        if ArtistDataKind::Popular.wanted(kinds) {
            page.popular = block_on(client.get_top_songs(&artist.name, Some(50)))?
                .iter()
                .map(|song| track_from_child(provider, &client, song))
                .collect();
        }
        if ArtistDataKind::Albums.wanted(kinds) {
            page.albums = artist
                .album
                .iter()
                .map(|album| ArtistAlbumCard {
                    id: album.id.clone(),
                    title: album.name.clone(),
                    date: album.year.map(|year| year.to_string()).unwrap_or_default(),
                    badge: "Album".to_string(),
                    thumbnail: cover_url(&client, album.cover_art.as_deref()),
                    url: String::new(),
                })
                .collect();
        }
    }

    if ArtistDataKind::Playlists.wanted(kinds) {
        page.playlists = block_on(client.get_playlists(None))?
            .iter()
            .map(|playlist| playlist_card(&client, playlist))
            .collect();
    }
    if ArtistDataKind::Related.wanted(kinds) {
        page.related = info
            .as_ref()
            .map_or(&[][..], |info| info.similar_artist.as_slice())
            .iter()
            .take(10)
            .map(|artist| RelatedArtistCard {
                id: artist.id.clone(),
                name: artist.name.clone(),
                stat: String::new(),
                thumbnail: cover_url(&client, artist.cover_art.as_deref()),
                url: String::new(),
            })
            .collect();
    }

    Ok(page)
}

/// Probe a candidate server configuration (used by the Settings form before
/// saving).
pub fn test_connection(server_url: &str, username: &str, password: &str) -> Result<String> {
    let client = Client::new(server_url, Auth::token(username, password))
        .map(|client| client.with_client_name("goosemusic"))
        .map_err(anyhow::Error::from)?;
    let info = block_on(client.server_info())?;
    let name = match (info.server_type.as_deref(), info.server_version.as_deref()) {
        (Some(kind), Some(version)) => format!("{kind} {version}"),
        (Some(kind), None) => kind.to_string(),
        (None, _) => "Subsonic".to_string(),
    };
    if info.open_subsonic {
        Ok(format!("{name} (OpenSubsonic)"))
    } else {
        Ok(name)
    }
}

/// Classify a Subsonic client failure for the localized error sentence.
pub fn error_kind(error: &opensubsonic::Error) -> ErrorKind {
    match error {
        opensubsonic::Error::Http(_) => ErrorKind::Network,
        opensubsonic::Error::Parse(_) => ErrorKind::Parse,
        opensubsonic::Error::Api(api) => match api.error_code() {
            Some(opensubsonic::SubsonicErrorCode::NotFound) => ErrorKind::Unavailable,
            Some(
                opensubsonic::SubsonicErrorCode::WrongCredentials
                | opensubsonic::SubsonicErrorCode::TokenAuthNotSupported
                | opensubsonic::SubsonicErrorCode::AuthMechanismNotSupported
                | opensubsonic::SubsonicErrorCode::ConflictingAuthentication
                | opensubsonic::SubsonicErrorCode::InvalidApiKey
                | opensubsonic::SubsonicErrorCode::NotAuthorized
                | opensubsonic::SubsonicErrorCode::TrialExpired,
            ) => ErrorKind::Denied,
            _ => ErrorKind::Other,
        },
        _ => ErrorKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_tracks_keep_the_subsonic_identity() {
        // The conversion itself is exercised by the server-facing paths; this
        // pins the provider ids that make cache keys unique per server.
        let provider = ProviderId::Custom(CustomProviderKind::Subsonic, 9);
        assert_eq!(provider.slug().as_ref(), "subsonic-9");
        assert_eq!(ProviderId::from_slug("subsonic-9"), Some(provider));
    }

    #[test]
    fn provider_ids_round_trip_through_their_serde_keys() {
        for &provider in ProviderId::builtin_all() {
            let json = serde_json::to_string(&provider).unwrap();
            assert_eq!(serde_json::from_str::<ProviderId>(&json).unwrap(), provider);
        }
        let provider = ProviderId::Custom(CustomProviderKind::Subsonic, 42);
        assert_eq!(serde_json::to_string(&provider).unwrap(), "\"subsonic-42\"");
        assert_eq!(
            serde_json::from_str::<ProviderId>("\"subsonic-42\"").unwrap(),
            provider
        );
        // Built-ins keep their historical names so existing files parse.
        for (key, provider) in [
            ("\"YouTube\"", ProviderId::YouTube),
            ("\"LastFm\"", ProviderId::LastFm),
            ("\"Local\"", ProviderId::Local),
        ] {
            assert_eq!(serde_json::from_str::<ProviderId>(key).unwrap(), provider);
        }
    }

    #[test]
    fn timeout_is_built_inside_the_provider_runtime() {
        let value = block_on(async { Ok::<_, opensubsonic::Error>(42) }).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn api_error_codes_map_to_user_facing_kinds() {
        let error = opensubsonic::Error::Api(opensubsonic::SubsonicApiError {
            code: 40,
            message: "Wrong username or password".into(),
            help_url: None,
        });
        assert_eq!(error_kind(&error), ErrorKind::Denied);
    }
}
