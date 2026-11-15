use std::{collections::HashSet, process::Command, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use innertube_rs::{
    endpoints::music::parse_music_watch_response, MusicAlbumItem, MusicArtistItem, MusicArtistPage,
    MusicArtistRef, MusicManager, MusicPlaylistItem, MusicSearchFilter, MusicTrackItem, Session,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{run_command_with_stdin_and_timeout, run_command_with_timeout, ytdlp};
use crate::{
    data::library::LibraryKind,
    providers::{
        AlbumMeta, ArtistAlbumCard, ArtistDataKind, ArtistHeader, ArtistPage, CardData,
        ClientEvent, HomeSection, HomeSectionData, HomeSectionKey, HomeSectionKind, ProviderId,
        RelatedArtistCard, SearchScope, SearchTab,
    },
    theme::SEARCH_PAGE_SIZE,
    types::{Track, TrackAlbum},
};

mod innertube;
mod raw;
use innertube::{post_music, raw_browse, raw_charts, raw_search, run_innertube, session};
use raw::{
    chart_video_playlist_id, collect_two_row_cards, explore_album_cards, normalize_playlist_id,
    playlist_browse_id, raw_artist_playlist_cards, raw_chart_artist_cards, raw_home_shelves,
    raw_rows_tracks, raw_tracks, raw_two_row_tracks, text_value, thumbnail_list_url,
    track_from_two_row,
};

#[derive(Deserialize)]
struct YTDLPSearchResult {
    id: String,
    title: String,
    #[serde(default, deserialize_with = "deserialize_flexible_duration")]
    duration: u32,
    channel: String,
    #[serde(default)]
    channel_id: Option<String>,
    #[serde(default)]
    view_count: Option<u64>,
}

/// Durations beyond a week are garbage (e.g. "1e30"), not tracks.
const MAX_TRACK_SECS: f64 = 7.0 * 24.0 * 3600.0;

const SEARCH_TIMEOUT: Duration = Duration::from_mins(1);
fn clamp_duration(secs: f64) -> u32 {
    if secs.is_finite() && (0.0..MAX_TRACK_SECS).contains(&secs) {
        secs as u32
    } else {
        0
    }
}

fn parse_duration_text(value: &str) -> u32 {
    let secs = value
        .trim()
        .split(':')
        .try_fold(0.0_f64, |acc, part| {
            part.trim().parse::<f64>().map(|p| acc * 60.0 + p)
        })
        .unwrap_or(0.0);
    clamp_duration(secs)
}

/// Durations arrive either as seconds (yt-dlp) or as a raw
/// "M:SS"/"H:MM:SS" string (`InnerTube` shelves); accept both.
fn deserialize_flexible_duration<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match Value::deserialize(deserializer)? {
        Value::Number(n) => clamp_duration(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => parse_duration_text(&s),
        _ => 0,
    })
}

/// Parse abbreviated view/play counts ("1.2M", "847", "3.4K", "841M plays").
fn parse_abbreviated_count(s: &str) -> u64 {
    let s = s
        .trim()
        .trim_end_matches("plays")
        .trim_end_matches("views")
        .trim()
        .replace(['\u{a0}', ','], "");
    let (num, mult) = match s.chars().last() {
        Some('K' | 'k') => (&s[..s.len() - 1], 1_000u64),
        Some('M' | 'm') => (&s[..s.len() - 1], 1_000_000),
        Some('B' | 'b') => (&s[..s.len() - 1], 1_000_000_000),
        _ => (s.as_str(), 1),
    };
    num.parse::<f64>().map_or(0, |n| (n * mult as f64) as u64)
}

fn item_duration(item: &MusicTrackItem) -> u32 {
    item.duration_ms
        .map(|ms| clamp_duration(ms as f64 / 1000.0))
        .filter(|&secs| secs > 0)
        .or_else(|| item.duration.as_deref().map(parse_duration_text))
        .unwrap_or(0)
}

fn watch_url(video_id: &str) -> String {
    format!("https://youtube.com/watch?v={video_id}")
}

fn default_thumbnail(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg")
}

fn artist_pairs(artists: &[MusicArtistRef]) -> Vec<(String, String)> {
    artists
        .iter()
        .filter(|artist| !artist.name.is_empty())
        .map(|artist| {
            (
                artist.name.clone(),
                artist.browse_id.clone().unwrap_or_default(),
            )
        })
        .collect()
}

fn build_track(
    video_id: &str,
    title: String,
    artists: Vec<(String, String)>,
    album: Option<TrackAlbum>,
    duration: u32,
    thumbnail: String,
    views: u64,
) -> Track {
    let (names, artist_ids) = artists.into_iter().unzip();
    Track::from_provider_artists_with_count(
        ProviderId::YouTube,
        video_id.to_string(),
        watch_url(video_id),
        title,
        names,
        duration,
        thumbnail,
        album,
        artist_ids,
        views,
    )
}

fn typed_tracks(items: &[MusicTrackItem]) -> Vec<Track> {
    items
        .iter()
        .filter_map(|item| track_from_item(item, None, &[]))
        .collect()
}

fn track_from_item(
    item: &MusicTrackItem,
    album: Option<TrackAlbum>,
    fallback_artists: &[MusicArtistRef],
) -> Option<Track> {
    if item.video_id.is_empty() {
        return None;
    }
    let source = if item.artists.is_empty() {
        fallback_artists
    } else {
        item.artists.as_slice()
    };
    let album = album.or_else(|| {
        item.album.as_ref().map(|album| TrackAlbum {
            name: album.title.clone(),
            id: album.browse_id.clone().unwrap_or_default(),
        })
    });
    Some(build_track(
        &item.video_id,
        item.title.clone(),
        artist_pairs(source),
        album,
        item_duration(item),
        item.thumbnail.clone().unwrap_or_default(),
        0,
    ))
}

fn fill_default_thumbnail(mut track: Track) -> Track {
    if track.thumbnail().is_empty() {
        let id = track
            .provider_id(ProviderId::YouTube)
            .unwrap_or_default()
            .to_string();
        if !id.is_empty() {
            if let Some(provider) = track.providers.get_mut(&ProviderId::YouTube) {
                provider.thumbnail = default_thumbnail(&id);
            }
        }
    }
    track
}

fn chart_video_tracks() -> Result<Vec<Track>> {
    let session = session()?;
    let charts = run_innertube(raw_charts(session.clone(), "ZZ"))?;
    let Some(playlist_id) = chart_video_playlist_id(&charts) else {
        return Ok(Vec::new());
    };
    let raw = run_innertube(raw_browse(session, playlist_browse_id(&playlist_id)))?;
    Ok(raw_tracks(&raw))
}

fn album_fallback_artists(album: &innertube_rs::MusicAlbumView) -> Vec<MusicArtistRef> {
    if !album.artists.is_empty() {
        return album.artists.clone();
    }
    album
        .artist
        .as_deref()
        .map(|line| {
            crate::util::split_artist_names(line)
                .into_iter()
                .map(|name| MusicArtistRef {
                    name,
                    browse_id: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn clean_home_subtitle(text: &str) -> String {
    for prefix in ["Album • ", "Single • ", "EP • ", "Playlist • "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    text.to_string()
}

fn artist_card(item: MusicArtistItem) -> Option<CardData> {
    if item.browse_id.is_empty() {
        return None;
    }
    Some(CardData::new(
        LibraryKind::Artist,
        ProviderId::YouTube,
        item.browse_id.clone(),
        item.name,
        item.subscribers.unwrap_or_default(),
        item.thumbnail.unwrap_or_default(),
        artist_page_url(&item.browse_id),
    ))
}

fn album_card(item: MusicAlbumItem) -> Option<CardData> {
    if item.browse_id.is_empty() {
        return None;
    }
    let names: Vec<&str> = item
        .artists
        .iter()
        .map(|artist| artist.name.as_str())
        .filter(|name| !name.is_empty())
        .collect();
    let subtitle = if names.is_empty() {
        item.artist.unwrap_or_default()
    } else {
        names.join(", ")
    };
    Some(CardData::new(
        LibraryKind::Album,
        ProviderId::YouTube,
        item.browse_id.clone(),
        item.title,
        clean_home_subtitle(&subtitle),
        item.thumbnail.unwrap_or_default(),
        album_page_url(&item.browse_id),
    ))
}

fn playlist_card(item: MusicPlaylistItem) -> Option<CardData> {
    let id = normalize_playlist_id(&item.browse_id);
    if id.is_empty() {
        return None;
    }
    Some(CardData::new(
        LibraryKind::Playlist,
        ProviderId::YouTube,
        id.to_string(),
        item.title,
        item.author.or(item.description).unwrap_or_default(),
        item.thumbnail.unwrap_or_default(),
        playlist_page_url(id),
    ))
}

/// Run a search and split the result into the playable `Track` list (for
/// Songs/Videos) and the `SearchTab` describing which tab is active (carrying
/// the concrete card lists for Artists/Albums/Playlists). yt-dlp is the
/// fallback for pagination (`search_more`) and when the `InnerTube` request
/// fails; it only yields playable tracks, so the scoped card tabs rely on
/// `YouTube Music` metadata.
pub fn search(query: &str, scope: SearchScope, offset: usize) -> Result<(Vec<Track>, SearchTab)> {
    if offset == 0 {
        match search_ytmusic(query, scope) {
            Ok(parts) => return Ok(parts),
            Err(error) => {
                tracing::warn!("YouTube Music search failed, falling back to yt-dlp: {error:#}");
            }
        }
    }
    let tracks = search_ytdlp(query, offset, SEARCH_PAGE_SIZE)?;
    Ok((tracks, SearchTab::Songs))
}

pub fn browse(id: &str, kind: &str) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    match normalize_browse_kind(id, kind) {
        "album" => browse_album(id),
        "playlist" => browse_playlist(id),
        _ => browse_artist(id),
    }
}

fn normalize_browse_kind<'a>(id: &str, kind: &'a str) -> &'a str {
    match kind {
        "album" | "playlist" | "artist" => kind,
        _ if id.starts_with("MPREb_") => "album",
        _ if id.starts_with("VL")
            || id.starts_with("PL")
            || id.starts_with("OL")
            || id.starts_with("RD") =>
        {
            "playlist"
        }
        _ => "artist",
    }
}

fn browse_album(id: &str) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    let session = session()?;
    let album_session = session.clone();
    let request_id = id.to_string();
    let album = run_innertube(async move {
        MusicManager::new(&album_session)
            .get_album(&request_id)
            .await
            .map_err(anyhow::Error::from)
    })?;
    let album_id = if album.browse_id.is_empty() {
        id.to_string()
    } else {
        album.browse_id.clone()
    };
    let album_ref = TrackAlbum {
        name: album.title.clone(),
        id: album_id.clone(),
    };
    let fallback_artists = album_fallback_artists(&album);
    let fallback_thumbnail = album.thumbnail.clone().unwrap_or_default();
    let raw_tracks = run_innertube(raw_browse(session, album_id))
        .map(|raw| {
            raw_rows_tracks(
                &raw,
                Some(&album_ref),
                &fallback_artists,
                &fallback_thumbnail,
            )
        })
        .unwrap_or_default();
    let tracks: Vec<Track> = if raw_tracks.is_empty() {
        album
            .tracks
            .iter()
            .filter_map(|item| track_from_item(item, Some(album_ref.clone()), &fallback_artists))
            .map(|mut track| {
                if track.thumbnail().is_empty() && !fallback_thumbnail.is_empty() {
                    if let Some(provider) = track.providers.get_mut(&ProviderId::YouTube) {
                        provider.thumbnail.clone_from(&fallback_thumbnail);
                    }
                }
                track
            })
            .collect()
    } else {
        raw_tracks
    };
    let meta = AlbumMeta {
        badge: album.album_type.unwrap_or_default(),
        date: album.year.unwrap_or_default(),
        thumbnail: album.thumbnail.unwrap_or_default(),
    };
    let meta = (!meta.badge.is_empty() || !meta.date.is_empty()).then_some(meta);
    Ok((tracks, meta))
}

fn browse_playlist(id: &str) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    let session = session()?;
    let mut browse_error = None;
    let tracks: Vec<Track> = match run_innertube(raw_browse(session, playlist_browse_id(id))) {
        Ok(raw) => raw_tracks(&raw),
        Err(error) => {
            browse_error = Some(error);
            Vec::new()
        }
    };
    if !tracks.is_empty() {
        return Ok((tracks, None));
    }
    if let Some(radio_id) = radio_id_from_playlist(id) {
        let tracks = radio_tracks(radio_id)?;
        if !tracks.is_empty() {
            return Ok((tracks, None));
        }
    }
    match browse_error {
        Some(error) => Err(error),
        None => Ok((Vec::new(), None)),
    }
}

fn browse_artist(id: &str) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    match load_artist_and_songs(id) {
        Ok((_, songs)) => Ok((songs, None)),
        Err(error) => match user_channel_page(id) {
            Ok(Some(page)) => Ok((page.popular, None)),
            _ => Err(error),
        },
    }
}

fn radio_id_from_playlist(id: &str) -> Option<&str> {
    let id = normalize_playlist_id(id);
    id.starts_with("RD").then_some(id)
}

fn load_artist(id: &str) -> Result<MusicArtistPage> {
    let artist_session = session()?;
    let request_id = id.to_string();
    run_innertube(async move {
        MusicManager::new(&artist_session)
            .get_artist(&request_id)
            .await
            .map_err(anyhow::Error::from)
    })
}

fn artist_songs(artist: &MusicArtistPage) -> Vec<Track> {
    let Some(playlist_id) = artist.songs_playlist_id.as_deref() else {
        return top_song_tracks(artist);
    };
    let Ok(session) = session() else {
        return top_song_tracks(artist);
    };
    match run_innertube(raw_browse(session, playlist_browse_id(playlist_id))) {
        Ok(raw) => {
            let tracks = raw_tracks(&raw);
            if tracks.is_empty() {
                top_song_tracks(artist)
            } else {
                tracks
            }
        }
        Err(_) => top_song_tracks(artist),
    }
}

fn load_artist_and_songs(id: &str) -> Result<(MusicArtistPage, Vec<Track>)> {
    let artist = load_artist(id)?;
    let songs = artist_songs(&artist);
    Ok((artist, songs))
}

fn top_song_tracks(artist: &MusicArtistPage) -> Vec<Track> {
    typed_tracks(&artist.top_songs)
}

fn fetch_home_and_explore(session: Arc<Session>) -> Result<(Value, Value)> {
    let (home, explore) = run_innertube(async move {
        let (home, explore) = tokio::join!(
            post_music(
                session.clone(),
                "/browse",
                json!({ "browseId": "FEmusic_home" }),
            ),
            post_music(session, "/browse", json!({ "browseId": "FEmusic_explore" }),),
        );
        Ok::<_, anyhow::Error>((home, explore))
    })?;
    match (home, explore) {
        (Ok(home), Ok(explore)) => Ok((home, explore)),
        (Ok(home), Err(_)) => Ok((home, Value::Null)),
        (Err(_), Ok(explore)) => Ok((Value::Null, explore)),
        (Err(error), Err(_)) => Err(error),
    }
}

/// Fetch `YouTube Music`'s home feed rows (quick picks, recommendations, new
/// releases, trending).
pub fn home_sections() -> Result<Vec<HomeSection>> {
    let session = session()?;
    let (home, explore) = fetch_home_and_explore(session)?;
    let shelves = raw_home_shelves(&home);

    let mut sections = Vec::new();
    if let Some(shelf) = shelves.iter().find(|shelf| shelf.tracks.len() >= 5) {
        let tracks: Vec<Track> = shelf.tracks.clone();
        if !tracks.is_empty() {
            sections.push(HomeSection::new(
                HomeSectionKey::new(ProviderId::YouTube, HomeSectionKind::QuickPicks),
                String::new(),
                HomeSectionData::Tracks(tracks),
            ));
        }
    }

    let for_you = shelves
        .iter()
        .find(|shelf| shelf.tracks.len() < 5 && shelf.cards.len() >= 4);
    if let Some(shelf) = for_you {
        let cards: Vec<CardData> = shelf.cards.clone();
        if !cards.is_empty() {
            sections.push(HomeSection::new(
                HomeSectionKey::new(ProviderId::YouTube, HomeSectionKind::ForYou),
                shelf.title.clone(),
                HomeSectionData::Cards(cards),
            ));
        }
    }

    let new_releases = explore_album_cards(&explore);
    if !new_releases.is_empty() {
        sections.push(HomeSection::new(
            HomeSectionKey::new(ProviderId::YouTube, HomeSectionKind::NewReleases),
            String::new(),
            HomeSectionData::Cards(new_releases),
        ));
    }

    let trending_shelf = shelves
        .iter()
        .filter(|shelf| shelf.tracks.len() >= 5)
        .nth(1);
    let (trending, trending_title) = match trending_shelf {
        Some(shelf) => (shelf.tracks.clone(), shelf.title.clone()),
        None => (chart_video_tracks().unwrap_or_default(), String::new()),
    };
    if !trending.is_empty() {
        sections.push(HomeSection::new(
            HomeSectionKey::new(ProviderId::YouTube, HomeSectionKind::Trending),
            trending_title,
            HomeSectionData::Tracks(trending),
        ));
    }

    Ok(sections)
}

/// Fetch the full artist page (header, popular songs, album/single/playlist
/// shelves, related artists) through the `InnerTube` music endpoints.
pub fn fetch_artist_page(id: &str, kinds: &[ArtistDataKind]) -> Result<ArtistPage> {
    let artist = match load_artist(id) {
        Ok(artist) => artist,
        Err(error) => {
            return match user_channel_page(id) {
                Ok(Some(mut page)) => {
                    filter_artist_page(&mut page, kinds);
                    Ok(page)
                }
                _ => Err(error),
            };
        }
    };
    let songs = if ArtistDataKind::Popular.wanted(kinds) {
        artist_songs(&artist)
    } else {
        Vec::new()
    };
    let mut page = artist_page_from(id, &artist, songs, kinds);
    filter_artist_page(&mut page, kinds);
    Ok(page)
}

fn artist_page_from(
    id: &str,
    artist: &MusicArtistPage,
    songs: Vec<Track>,
    kinds: &[ArtistDataKind],
) -> ArtistPage {
    let mut header = ArtistHeader {
        image: artist.thumbnail.clone().unwrap_or_default(),
        stats: Vec::new(),
        description: artist.description.clone().unwrap_or_default(),
    };
    if let Some(monthly) = artist.monthly_listeners.clone() {
        header
            .stats
            .push(("YouTube Monthly listeners".to_string(), monthly));
    }
    if let Some(subscribers) = artist.subscribers.clone() {
        header
            .stats
            .push(("YouTube Subscribers".to_string(), subscribers));
    }

    let mut albums: Vec<ArtistAlbumCard> = Vec::new();
    for (items, badge) in [(&artist.albums, "Album"), (&artist.singles, "Single")] {
        for item in items {
            if item.browse_id.is_empty() {
                continue;
            }
            albums.push(ArtistAlbumCard::new(
                ProviderId::YouTube,
                item.browse_id.clone(),
                item.title.clone(),
                badge,
                item.year.clone().unwrap_or_default(),
                item.thumbnail.clone().unwrap_or_default(),
                album_page_url(&item.browse_id),
            ));
        }
    }
    // `MusicArtistPage::featured_on` is never populated by the crate's parser,
    // so the playlist shelf comes from the raw artist browse below. The same
    // response serves as the popular fallback for regular channels, whose
    // videos live in two-row shelves rather than a "Top songs" shelf.
    let wants_playlists = ArtistDataKind::Playlists.wanted(kinds);
    let wants_popular = ArtistDataKind::Popular.wanted(kinds);
    let needs_raw = wants_playlists || (wants_popular && songs.is_empty());
    let raw = needs_raw
        .then(|| {
            session()
                .ok()
                .and_then(|session| run_innertube(raw_browse(session, id.to_string())).ok())
        })
        .flatten();
    let popular = channel_popular(songs, raw.as_ref());
    let mut playlists: Vec<CardData> = Vec::new();
    if wants_playlists {
        if let Some(raw) = &raw {
            for card in raw_artist_playlist_cards(raw) {
                if !playlists
                    .iter()
                    .any(|existing| existing.card.id == card.card.id)
                {
                    playlists.push(card);
                }
            }
        }
    }
    let related: Vec<RelatedArtistCard> = artist
        .similar_artists
        .iter()
        .filter(|item| !item.browse_id.is_empty())
        .map(|item| {
            RelatedArtistCard::new(
                ProviderId::YouTube,
                item.browse_id.clone(),
                item.name.clone(),
                item.subscribers.clone().unwrap_or_default(),
                item.thumbnail.clone().unwrap_or_default(),
                artist_page_url(&item.browse_id),
            )
        })
        .collect();

    ArtistPage {
        header: Some(header),
        popular,
        albums,
        playlists,
        related,
    }
}

/// Popular rows for an artist page: the crate's parsed "Top songs" shelf when
/// there is one, otherwise the video cards of a regular channel page (those
/// pages have a `musicVisualHeaderRenderer` and no songs shelf).
fn channel_popular(songs: Vec<Track>, raw: Option<&Value>) -> Vec<Track> {
    if !songs.is_empty() {
        return songs;
    }
    raw.filter(|raw| raw.pointer("/header/musicVisualHeaderRenderer").is_some())
        .map_or_else(Vec::new, |raw| raw_two_row_tracks(raw, &[]))
}

fn user_channel_page(id: &str) -> Result<Option<ArtistPage>> {
    let session = session()?;
    let raw = run_innertube(raw_browse(session, id.to_string()))?;
    Ok(user_channel_page_from(&raw, id))
}

fn user_channel_page_from(raw: &Value, id: &str) -> Option<ArtistPage> {
    let header_renderer = raw.pointer("/header/musicVisualHeaderRenderer");
    let micro = raw.pointer("/microformat/microformatDataRenderer");
    let channel_name = header_renderer
        .and_then(|header| header.get("title"))
        .map(text_value)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            micro
                .and_then(|micro| micro.get("title"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let foreground = thumbnail_list_url(header_renderer.and_then(|header| {
        header.pointer("/foregroundThumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
    }));
    let image = if foreground.is_empty() {
        thumbnail_list_url(micro.and_then(|micro| micro.pointer("/thumbnail/thumbnails")))
    } else {
        foreground
    };
    let subscribers = header_renderer
        .and_then(|header| {
            header.pointer("/subscriptionButton/subscribeButtonRenderer/subscriberCountText")
        })
        .map(text_value)
        .filter(|subs| !subs.is_empty());
    let fallback_artist = MusicArtistRef {
        name: channel_name,
        browse_id: (!id.is_empty()).then(|| id.to_string()),
    };
    let mut popular = Vec::new();
    let mut seen = HashSet::new();
    for card in collect_two_row_cards(raw) {
        let Some(track) = track_from_two_row(card, std::slice::from_ref(&fallback_artist)) else {
            continue;
        };
        let Some(video_id) = track.provider_id(ProviderId::YouTube).map(str::to_string) else {
            continue;
        };
        if seen.insert(video_id) {
            popular.push(track);
        }
    }
    if popular.is_empty() && image.is_empty() {
        return None;
    }
    let stats = subscribers
        .map(|subs| vec![("YouTube Subscribers".to_string(), subs)])
        .unwrap_or_default();
    Some(ArtistPage {
        header: Some(ArtistHeader {
            image,
            stats,
            description: String::new(),
        }),
        popular,
        albums: Vec::new(),
        playlists: Vec::new(),
        related: Vec::new(),
    })
}

fn filter_artist_page(page: &mut ArtistPage, kinds: &[ArtistDataKind]) {
    if !ArtistDataKind::Popular.wanted(kinds) {
        page.popular.clear();
    }
    if !ArtistDataKind::Header.wanted(kinds) {
        if let Some(header) = &mut page.header {
            header.stats.clear();
            header.description.clear();
            header.image.clear();
        }
    }
    if !ArtistDataKind::Albums.wanted(kinds) {
        page.albums.clear();
    }
    if !ArtistDataKind::Playlists.wanted(kinds) {
        page.playlists.clear();
    }
    if !ArtistDataKind::Related.wanted(kinds) {
        page.related.clear();
    }
}

/// Resolve an artist name to a `YouTube` channel browseId via the Artists
/// search scope. Returns `Ok(None)` when nothing matched.
pub fn resolve_artist_id(name: &str) -> Result<Option<String>> {
    let (_, tab) = search_ytmusic(name, SearchScope::Artists)?;
    Ok(match tab {
        SearchTab::Artists(cards) => cards.into_iter().next().map(|c| c.card.id.clone()),
        _ => None,
    })
}

/// Public page for a `YouTube` Music artist/album/playlist. Card ids are
/// already addressable (channel ids, album browse ids, playlist ids), so the
/// page is a template over the id. These mirror the shapes search cards use.
fn artist_page_url(id: &str) -> String {
    // The shared rule for "artist page for this id" lives on the provider, so
    // a track's artist id resolves to the same page its cards do.
    ProviderId::YouTube.artist_page_url(id).unwrap_or_default()
}

fn album_page_url(id: &str) -> String {
    // Same id space as the ids on tracks' `TrackAlbum`, so both go through the
    // shared rule.
    ProviderId::YouTube.album_page_url(id).unwrap_or_default()
}

fn playlist_page_url(id: &str) -> String {
    let id = normalize_playlist_id(id);
    format!("https://music.youtube.com/playlist?list={id}")
}

impl From<SearchScope> for MusicSearchFilter {
    fn from(scope: SearchScope) -> Self {
        match scope {
            SearchScope::Songs => MusicSearchFilter::Songs,
            SearchScope::Videos => MusicSearchFilter::Videos,
            SearchScope::Artists => MusicSearchFilter::Artists,
            SearchScope::Albums => MusicSearchFilter::Albums,
            SearchScope::Playlists => MusicSearchFilter::Playlists,
        }
    }
}

fn search_ytmusic(query: &str, scope: SearchScope) -> Result<(Vec<Track>, SearchTab)> {
    if query.trim().is_empty() {
        return charts_search(scope);
    }
    let filter = MusicSearchFilter::from(scope);
    if matches!(scope, SearchScope::Songs | SearchScope::Videos) {
        let tracks = search_tracks_raw(query, filter)?;
        return Ok((tracks, scope.into()));
    }
    let session = session()?;
    let query = query.to_string();
    let results = run_innertube(async move {
        MusicManager::new(&session)
            .search(&query, Some(filter))
            .await
            .map_err(anyhow::Error::from)
    })?;

    let tab = match scope {
        SearchScope::Artists => SearchTab::Artists(
            results
                .artists
                .into_iter()
                .filter_map(artist_card)
                .collect(),
        ),
        SearchScope::Albums => {
            SearchTab::Albums(results.albums.into_iter().filter_map(album_card).collect())
        }
        SearchScope::Playlists => SearchTab::Playlists(
            results
                .playlists
                .into_iter()
                .filter_map(playlist_card)
                .collect(),
        ),
        SearchScope::Songs | SearchScope::Videos => {
            unreachable!("Songs and Videos return before the card search")
        }
    };
    Ok((Vec::new(), tab))
}

fn search_tracks_raw(query: &str, filter: MusicSearchFilter) -> Result<Vec<Track>> {
    let session = session()?;
    let raw = run_innertube(raw_search(
        session,
        query.to_string(),
        filter.to_param_str(),
    ))?;
    Ok(raw_tracks(&raw))
}

fn charts_search(scope: SearchScope) -> Result<(Vec<Track>, SearchTab)> {
    match scope {
        SearchScope::Playlists => Ok((Vec::new(), SearchTab::Playlists(Vec::new()))),
        SearchScope::Songs | SearchScope::Videos => {
            if let Ok(tracks) = chart_video_tracks() {
                if !tracks.is_empty() {
                    return Ok((tracks, scope.into()));
                }
            }
            let session = session()?;
            let raw = run_innertube(raw_browse(session, "FEmusic_explore".to_string()))?;
            let tracks = if matches!(scope, SearchScope::Songs) {
                raw_tracks(&raw)
            } else {
                raw_two_row_tracks(&raw, &[])
            };
            Ok((tracks, scope.into()))
        }
        SearchScope::Artists => {
            let session = session()?;
            let raw = run_innertube(raw_charts(session, "ZZ"))?;
            Ok((Vec::new(), SearchTab::Artists(raw_chart_artist_cards(&raw))))
        }
        SearchScope::Albums => {
            let session = session()?;
            let raw = run_innertube(raw_browse(session, "FEmusic_explore".to_string()))?;
            Ok((Vec::new(), SearchTab::Albums(explore_album_cards(&raw))))
        }
    }
}

fn search_ytdlp(query: &str, offset: usize, page_size: usize) -> Result<Vec<Track>> {
    // yt-dlp cannot parse an empty/blank query ("ytsearchN:" alone is an
    // unsupported url scheme); blank queries are served by the charts path,
    // so degrade to an empty page instead of erroring.
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    // flat_search returns full metadata in a single yt-dlp pass.
    flat_search(query, offset + 1, offset + page_size)
}

pub fn search_more(query: &str, offset: usize) -> Result<Vec<Track>> {
    // Pagination only works through yt-dlp, which yields playable tracks.
    // (Scoped searches get their first page from YouTube Music; further pages
    // fall back to general track results — acceptable for "Load More".)
    search_ytdlp(query, offset, SEARCH_PAGE_SIZE)
}

fn track_from_ytdlp(item: YTDLPSearchResult) -> Option<Track> {
    if !is_video_id(&item.id) {
        return None;
    }
    let artists = if item.channel.is_empty() {
        Vec::new()
    } else {
        vec![(item.channel, item.channel_id.unwrap_or_default())]
    };
    Some(build_track(
        &item.id,
        item.title,
        artists,
        None,
        item.duration,
        default_thumbnail(&item.id),
        item.view_count.unwrap_or(0),
    ))
}

// yt-dlp search pass. The `ytsearchN:` prefix requests up to N results;
// `--playlist-start/--playlist-end` then slice the requested window. (The old
// `--default-search <music-url> --flat-playlist` combo now resolves to channel
// browse ids — `UC...` — which is_video_id() rejects, so every search returned
// zero results.) Each dumped entry already carries duration/channel/views, so
// no second metadata pass is required. Playlist offsets are 1-based per
// yt-dlp's --playlist-start/--playlist-end convention; callers must convert
// 0-based offsets to 1-based before calling.
fn flat_search(query: &str, start: usize, end: usize) -> Result<Vec<Track>> {
    let start_str = start.to_string();
    let end_str = end.to_string();
    // `ytsearchN:` caps how many results yt-dlp fetches; use `end` (the upper
    // bound of the requested window) as N so the slice below is always within
    // what was actually fetched.
    let search_spec = format!("ytsearch{end}:{query}");
    let args: Vec<&str> = vec![
        "--dump-json",
        "--no-warnings",
        "--skip-download",
        "--playlist-start",
        &start_str,
        "--playlist-end",
        &end_str,
        &search_spec,
    ];

    let mut cmd =
        crate::deps::yt_dlp_command().context("Failed to run yt-dlp. Is it installed?")?;
    cmd.args(&args);
    let flat_output = run_command_with_timeout(&mut cmd, SEARCH_TIMEOUT)
        .context("Failed to run yt-dlp. Is it installed?")?;

    if !flat_output.status.success() {
        let stderr = String::from_utf8_lossy(&flat_output.stderr);
        anyhow::bail!("yt-dlp search failed: {stderr}");
    }

    let mut tracks = Vec::new();
    for line in String::from_utf8_lossy(&flat_output.stdout).lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(item) = serde_json::from_str::<YTDLPSearchResult>(line) {
            // Non-video playlist entries (mixes, channels, etc.) are skipped
            // inside `track_from_ytdlp`.
            if let Some(track) = track_from_ytdlp(item) {
                tracks.push(track);
            }
        }
    }

    Ok(tracks)
}

/// Whether any track still lacks duration or play-count data, i.e. the
/// yt-dlp [`enrich_track_metadata`] pass would have work to do.
pub fn needs_enrichment(tracks: &[Track]) -> bool {
    tracks.iter().any(|t| {
        t.providers
            .get(&ProviderId::YouTube)
            .is_some_and(|pt| pt.duration == 0 || pt.play_count == 0)
    })
}

/// Fill missing duration/view-count data on YouTube-sourced tracks with one
/// batched yt-dlp metadata pass. Used as a second phase after the artist
/// page renders, so popular rows don't wait on yt-dlp. Only tracks that are
/// actually missing data are queried.
pub fn enrich_track_metadata(tracks: &mut [Track]) {
    let ids: Vec<String> = tracks
        .iter()
        .filter(|t| {
            t.providers
                .get(&ProviderId::YouTube)
                .is_some_and(|pt| pt.duration == 0 || pt.play_count == 0)
        })
        .filter_map(|t| t.provider_id(ProviderId::YouTube).map(str::to_string))
        .collect();
    if ids.is_empty() {
        return;
    }
    let metadata = fetch_batch_metadata(&ids);
    for track in tracks.iter_mut() {
        let Some(id) = track.provider_id(ProviderId::YouTube).map(str::to_string) else {
            continue;
        };
        if let (Some(item), Some(pt)) = (
            metadata.get(&id),
            track.providers.get_mut(&ProviderId::YouTube),
        ) {
            if pt.duration == 0 {
                pt.duration = item.duration;
            }
            // The yt-dlp pass's exact view_count is the fallback for rows
            // the shelf couldn't supply counts for.
            if pt.play_count == 0 {
                pt.play_count = item.view_count.unwrap_or(0);
            }
        }
    }
}

// Single batched metadata pass over yt-dlp for a list of video ids. Returns a
// map keyed by video id; a failed yt-dlp invocation yields an empty map so
// callers gracefully fall back to the cheap flat-search stubs.
fn fetch_batch_metadata(
    valid_ids: &[String],
) -> std::collections::HashMap<String, YTDLPSearchResult> {
    use std::collections::HashMap;
    let mut results: HashMap<String, YTDLPSearchResult> = HashMap::new();

    let Some(path) = crate::deps::resolve_yt_dlp() else {
        return results;
    };
    let cookie_args = crate::deps::cookie_args();
    let mut batch_args = vec![
        "--batch-file",
        "-",
        "--dump-json",
        "--skip-download",
        "--no-warnings",
    ];
    batch_args.extend(cookie_args.iter().map(String::as_str));
    let mut cmd = Command::new(path);
    cmd.args(&batch_args);
    // `--batch-file -` reads the URLs from stdin, one per line.
    let mut stdin = Vec::new();
    for id in valid_ids {
        stdin.extend_from_slice(format!("https://youtube.com/watch?v={id}\n").as_bytes());
    }
    let Ok(output) =
        run_command_with_stdin_and_timeout(&mut cmd, Some(stdin), Duration::from_mins(1))
    else {
        return results;
    };
    if output.status.success() {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(item) = serde_json::from_str::<YTDLPSearchResult>(line) {
                results.insert(item.id.clone(), item);
            }
        }
    }
    results
}

fn song_radio(video_id: &str) -> Result<Vec<Track>> {
    let session = session()?;
    let video_id = video_id.to_string();
    let page = run_innertube(async move {
        MusicManager::new(&session)
            .get_radio_page(&video_id, None)
            .await
            .map_err(anyhow::Error::from)
    })?;
    Ok(typed_tracks(&page.tracks)
        .into_iter()
        .map(fill_default_thumbnail)
        .collect())
}

fn radio_tracks(playlist_id: &str) -> Result<Vec<Track>> {
    let session = session()?;
    let playlist_id = normalize_playlist_id(playlist_id).to_string();
    let raw = run_innertube(post_music(
        session,
        "/next",
        json!({
            "enablePersistentPlaylistPanel": true,
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
            "playlistId": playlist_id,
        }),
    ))?;
    let page = parse_music_watch_response(&raw, true).map_err(anyhow::Error::from)?;
    Ok(typed_tracks(&page.tracks)
        .into_iter()
        .map(fill_default_thumbnail)
        .collect())
}

/// Build a song radio from a real `YouTube` Music mix seeded by the track's
/// `video_id`.
pub fn radio_song(video_id: &str) -> Result<Vec<Track>> {
    if video_id.is_empty() {
        return Ok(Vec::new());
    }
    song_radio(video_id)
}

/// Build an artist radio from a real `YouTube` Music mix seeded by the artist's
/// `browse_id` (resolved from the track's provider artist id).
pub fn radio_artist(browse_id: &str) -> Result<Vec<Track>> {
    if browse_id.is_empty() {
        return Ok(Vec::new());
    }
    if !browse_id.starts_with("UC") {
        return radio_tracks(browse_id);
    }
    let session = session()?;
    let request_id = browse_id.to_string();
    let artist = run_innertube(async move {
        MusicManager::new(&session)
            .get_artist(&request_id)
            .await
            .map_err(anyhow::Error::from)
    })?;
    if let Some(radio_id) = artist.radio_id.as_deref().filter(|id| !id.is_empty()) {
        return radio_tracks(radio_id);
    }
    if let Some(first) = artist
        .top_songs
        .first()
        .filter(|item| !item.video_id.is_empty())
    {
        return song_radio(&first.video_id);
    }
    anyhow::bail!("YouTube Music has no radio for this artist");
}

/// Resolve a logical track (title + artist) to a `YouTube` video id by running a
/// yt-dlp search and returning the first result's id. Used by the "play via /
/// download from `YouTube`" flow when a track lacks a `YouTube` id.
pub fn resolve_id(track: &Track) -> Result<Option<Track>> {
    let query = track.search_query();
    Ok(flat_search(&query, 1, 1)?.into_iter().next())
}

pub fn download(
    video_url: &str,
    download_dir: &std::path::Path,
    emit: &dyn Fn(ClientEvent),
) -> Result<String> {
    let id = video_url
        .split("v=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .unwrap_or("download");
    let output_path = super::download_file_path(download_dir, id);
    download_audio(video_url, output_path.to_string_lossy().as_ref(), emit)
}

pub fn download_audio(
    video_url: &str,
    output_path: &str,
    emit: &dyn Fn(ClientEvent),
) -> Result<String> {
    // Start on yt-dlp's defaults (best quality for most videos); the
    // quality-aware race runs only if this attempt fails with a client
    // failure. Never start on the remembered winner: it was the best client
    // for a *different* video and may only offer a low-bitrate muxed MP4 here
    // while defaults serve high-bitrate audio.
    match ytdlp::download_audio(video_url, output_path, &[]) {
        Ok(path) => Ok(path),
        Err(e) if ytdlp::is_youtube_url(video_url) && ytdlp::is_client_failure(&e.to_string()) => {
            ytdlp::forget_client();
            let (winner, _) = ytdlp::resolve_player_client(video_url, ytdlp::DOWNLOAD_FORMAT, emit);
            let extractor_arg = winner
                .as_deref()
                .map(|c| format!("youtube:player_client={c}"));
            let extra_args: Vec<&str> = match &extractor_arg {
                Some(arg) => vec!["--extractor-args", arg.as_str()],
                None => Vec::new(),
            };
            match ytdlp::download_audio(video_url, output_path, &extra_args) {
                Ok(path) => Ok(path),
                Err(e) => {
                    if ytdlp::is_client_failure(&e.to_string()) {
                        ytdlp::forget_client();
                    }
                    Err(e)
                }
            }
        }
        Err(e) => {
            if ytdlp::is_client_failure(&e.to_string()) {
                ytdlp::forget_client();
            }
            Err(e)
        }
    }
}

/// Whether `id` looks like a real `YouTube` video `id` (11 chars, not a
/// mix/playlist/channel entry). yt-dlp's flat playlist also returns mixes
/// (`MPRE` prefix), channel uploads (`UC` prefix), and other non-video
/// entries whose ids are longer than the 11-character video id; those are
/// filtered out so the search results stay playable.
fn is_video_id(id: &str) -> bool {
    id.len() == 11 && !id.starts_with("MPRE") && !id.starts_with("UC")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_enrichment_only_when_data_missing() {
        fn track(id: &str, duration: u32, plays: u64) -> Track {
            Track::from_provider_with_count(
                ProviderId::YouTube,
                id.into(),
                "url".into(),
                "t",
                "a",
                duration,
                "thumb",
                None,
                None,
                plays,
            )
        }
        assert!(!needs_enrichment(&[]));
        assert!(!needs_enrichment(&[track("a", 259, 100)]));
        assert!(!needs_enrichment(&[
            track("a", 259, 100),
            track("b", 300, 50)
        ]));
        assert!(needs_enrichment(&[track("a", 0, 100)]));
        assert!(needs_enrichment(&[track("a", 259, 0)]));
        assert!(needs_enrichment(&[track("a", 259, 100), track("b", 0, 0)]));
    }

    #[test]
    fn parses_abbreviated_counts() {
        use super::parse_abbreviated_count as p;
        assert_eq!(p("847"), 847);
        assert_eq!(p("1.2K"), 1_200);
        assert_eq!(p("966K"), 966_000);
        assert_eq!(p("4.9M"), 4_900_000);
        assert_eq!(p("841M plays"), 841_000_000);
        assert_eq!(p("614M\u{a0}plays"), 614_000_000);
        assert_eq!(p("3.4B views"), 3_400_000_000);
        assert_eq!(p(""), 0);
        assert_eq!(p("garbage"), 0);
        assert_eq!(p(" \u{a0}1.2K "), 1_200);
        assert_eq!(p("-3K"), 0);
    }

    #[test]
    fn parses_duration_shapes() {
        assert_eq!(parse_duration_text("4:36"), 276);
        assert_eq!(parse_duration_text("2:00:05"), 7205);
        for malformed in ["-1:30", "1e30", "4:xx"] {
            assert_eq!(parse_duration_text(malformed), 0);
        }
        let item: YTDLPSearchResult = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "channel": "c", "duration": 321.0
        }))
        .unwrap();
        assert_eq!(item.duration, 321);
        let item: YTDLPSearchResult = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "channel": "c", "duration": null
        }))
        .unwrap();
        assert_eq!(item.duration, 0);
        let item: YTDLPSearchResult = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "channel": "c", "duration": "4:36"
        }))
        .unwrap();
        assert_eq!(item.duration, 276);
        let item: YTDLPSearchResult = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "channel": "c", "duration": 1e30
        }))
        .unwrap();
        assert_eq!(item.duration, 0);
    }

    #[test]
    fn ytdlp_results_keep_the_channel_id() {
        let item: YTDLPSearchResult = serde_json::from_value(serde_json::json!({
            "id": "dQw4w9WgXcQ",
            "title": "T",
            "channel": "Rick Astley",
            "channel_id": "UCuAXFkgsw1L7xaCfnd5JJOw"
        }))
        .unwrap();
        let track = track_from_ytdlp(item).unwrap();
        assert_eq!(track.artists, ["Rick Astley"]);
        assert_eq!(
            track.artist_target(0),
            Some(("UCuAXFkgsw1L7xaCfnd5JJOw", ProviderId::YouTube))
        );

        // Older/partial entries carry no channel id; the artist stays a name.
        let item: YTDLPSearchResult = serde_json::from_value(serde_json::json!({
            "id": "dQw4w9WgXcQ",
            "title": "T",
            "channel": "Rick Astley"
        }))
        .unwrap();
        let track = track_from_ytdlp(item).unwrap();
        assert_eq!(track.artists, ["Rick Astley"]);
        assert_eq!(track.artist_target(0), None);
    }

    #[test]
    fn normalizes_browse_kinds() {
        assert_eq!(normalize_browse_kind("MPREb_x", ""), "album");
        assert_eq!(normalize_browse_kind("VLPL123", ""), "playlist");
        assert_eq!(normalize_browse_kind("PL123", ""), "playlist");
        assert_eq!(normalize_browse_kind("RD123", ""), "playlist");
        assert_eq!(normalize_browse_kind("UC123", ""), "artist");
        assert_eq!(normalize_browse_kind("anything", "album"), "album");
    }

    #[test]
    fn extracts_radio_playlist_ids() {
        assert_eq!(radio_id_from_playlist("VLRD123"), Some("RD123"));
        assert_eq!(radio_id_from_playlist("RD123"), Some("RD123"));
        assert_eq!(radio_id_from_playlist("VLPL123"), None);
    }

    #[test]
    fn regular_channel_popular_falls_back_to_its_video_cards() {
        use serde_json::json;
        let channel = json!({
            "header": {
                "musicVisualHeaderRenderer": {
                    "title": { "runs": [{ "text": "A Channel" }] }
                }
            },
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicCarouselShelfRenderer": {
                                            "title": { "runs": [{ "text": "Videos" }] },
                                            "contents": [{
                                                "musicTwoRowItemRenderer": {
                                                    "title": { "runs": [{ "text": "A Video" }] },
                                                    "subtitle": { "runs": [
                                                        {
                                                            "text": "A Channel",
                                                            "navigationEndpoint": {
                                                                "browseEndpoint": {
                                                                    "browseId": "UC_channel",
                                                                    "browseEndpointContextSupportedConfigs": {
                                                                        "browseEndpointContextMusicConfig": {
                                                                            "pageType": "MUSIC_PAGE_TYPE_USER_CHANNEL"
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        },
                                                        { "text": " • " },
                                                        { "text": "1M views" }
                                                    ] },
                                                    "navigationEndpoint": {
                                                        "watchEndpoint": { "videoId": "dQw4w9WgXcQ" }
                                                    },
                                                    "thumbnailRenderer": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [{ "url": "thumb" }]
                                                            }
                                                        }
                                                    }
                                                }
                                            }]
                                        }
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        });
        let popular = channel_popular(Vec::new(), Some(&channel));
        assert_eq!(popular.len(), 1);
        assert_eq!(popular[0].title, "A Video");
        assert_eq!(popular[0].artists, ["A Channel"]);
        assert_eq!(
            popular[0].artist_target(0),
            Some(("UC_channel", ProviderId::YouTube))
        );

        // An immersive (music artist) page keeps the crate's parsed songs and
        // gains no video fallback.
        let artist = json!({
            "header": { "musicImmersiveHeaderRenderer": {} },
            "contents": {}
        });
        assert!(channel_popular(Vec::new(), Some(&artist)).is_empty());
        let songs = typed_tracks(&[MusicTrackItem {
            video_id: "dQw4w9WgXcQ".into(),
            title: "A Song".into(),
            ..Default::default()
        }]);
        assert_eq!(
            channel_popular(songs, Some(&channel))
                .first()
                .map(|track| track.title.as_str()),
            Some("A Song")
        );
    }
}
