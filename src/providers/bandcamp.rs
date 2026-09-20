use anyhow::{Context, Result};

use crate::{
    providers::{
        AlbumMeta, ArtistAlbumCard, ArtistHeader, ArtistPage, CardData, ProviderId, SearchScope,
        SearchTab,
    },
    theme::SEARCH_PAGE_SIZE,
    types::{Track, TrackAlbum},
};

fn block_on<F, T>(fut: F) -> Result<T>
where
    F: std::future::Future<Output = std::result::Result<T, bandcamp::Error>>,
{
    super::shared_runtime()
        .block_on(fut)
        .map_err(|e| anyhow::anyhow!("Bandcamp: {e}"))
}

fn stream_url(t: &bandcamp::AlbumTrack) -> Option<String> {
    t.streaming_url
        .get("mp3-128")
        .cloned()
        .or_else(|| t.streaming_url.values().next().cloned())
}

/// Thumbnail URL at a size matching the largest display (120px cards, with
/// margin for high-density screens). The crate default serves multi-megapixel
/// originals (~2.4MB/2559px) that stall the thumbnail pipeline and spike CPU
/// on JPEG decode for a 36px row.
macro_rules! sized_thumb {
    ($image:expr) => {
        $image
            .get_with_resolution(bandcamp::ImageResolution::Px300)
            .or_else(|| $image.get_url())
            .unwrap_or_default()
    };
}

fn track_from_album_track(album: &bandcamp::Album, t: &bandcamp::AlbumTrack) -> Option<Track> {
    if !t.is_streamable {
        return None;
    }
    let url = stream_url(t)?;
    let artist = if t.band_name.is_empty() {
        album.band.name.clone()
    } else {
        t.band_name.clone()
    };
    let album_title = t.album_title.clone().unwrap_or_else(|| album.title.clone());
    let album_meta = if album_title.is_empty() {
        None
    } else {
        Some(TrackAlbum {
            name: album_title,
            id: t.album_id.map(|id| id.to_string()).unwrap_or_default(),
        })
    };
    Some(Track::from_provider(
        ProviderId::Bandcamp,
        t.id.to_string(),
        url,
        t.title.clone(),
        artist.clone(),
        t.duration.unwrap_or(0.0) as u32,
        t.image
            .get_with_resolution(bandcamp::ImageResolution::Px300)
            .or_else(|| t.image.get_url())
            .or_else(|| {
                album
                    .image
                    .get_with_resolution(bandcamp::ImageResolution::Px300)
            })
            .or_else(|| album.image.get_url())
            .unwrap_or_default(),
        album_meta,
        Some(t.band_id.to_string()),
    ))
}

fn album_tracks(album: &bandcamp::Album) -> Vec<Track> {
    album
        .tracks
        .iter()
        .filter_map(|t| track_from_album_track(album, t))
        .collect()
}

fn split_release_id(id: &str) -> Result<(u64, u64, String)> {
    let mut parts = id.splitn(3, ':');
    let band = parts
        .next()
        .unwrap_or_default()
        .parse::<u64>()
        .context("bad Bandcamp album id")?;
    let item = parts
        .next()
        .unwrap_or_default()
        .parse::<u64>()
        .context("bad Bandcamp album id")?;
    Ok((band, item, parts.next().unwrap_or("a").to_string()))
}

fn fetch_tralbum(band: u64, item: u64, kind: &str) -> Result<bandcamp::Album> {
    block_on(async {
        if kind == "t" {
            bandcamp::fetch_track(band, item).await
        } else {
            bandcamp::fetch_album(band, item).await
        }
    })
}

fn fetch_artist(id: &str) -> Result<bandcamp::Artist> {
    if id.starts_with("http") {
        block_on(bandcamp::artist_from_url(id))
    } else {
        let band = id.parse::<u64>().context("bad Bandcamp artist id")?;
        block_on(bandcamp::fetch_artist(band))
    }
}

pub fn search(query: &str, scope: SearchScope, offset: usize) -> Result<(Vec<Track>, SearchTab)> {
    if offset > 0 {
        return Ok((Vec::new(), SearchTab::from_scope(scope)));
    }
    let items = block_on(bandcamp::search(query))?;
    match scope {
        SearchScope::Artists => {
            let cards = items
                .iter()
                .filter_map(|item| match item {
                    bandcamp::SearchResultItem::Artist(a) => Some(CardData {
                        id: a.url.clone(),
                        title: a.name.clone(),
                        subtitle: a.location.clone().unwrap_or_default(),
                        thumbnail: sized_thumb!(a.image),
                    }),
                    _ => None,
                })
                .take(SEARCH_PAGE_SIZE)
                .collect();
            Ok((Vec::new(), SearchTab::Artists(cards)))
        }
        SearchScope::Albums => {
            let cards = items
                .iter()
                .filter_map(|item| match item {
                    bandcamp::SearchResultItem::Album(a) => Some(CardData {
                        id: format!("{}:{}:a", a.band_id, a.album_id),
                        title: a.name.clone(),
                        subtitle: a.band_name.clone(),
                        thumbnail: sized_thumb!(a.image),
                    }),
                    _ => None,
                })
                .take(SEARCH_PAGE_SIZE)
                .collect();
            Ok((Vec::new(), SearchTab::Albums(cards)))
        }
        _ => {
            let direct: Vec<(u64, u64)> = items
                .iter()
                .filter_map(|item| match item {
                    bandcamp::SearchResultItem::Track(t) => Some((t.band_id, t.track_id)),
                    _ => None,
                })
                .take(SEARCH_PAGE_SIZE)
                .collect();
            let expand: Vec<(u64, u64)> = if direct.len() >= SEARCH_PAGE_SIZE {
                Vec::new()
            } else {
                items
                    .iter()
                    .filter_map(|item| match item {
                        bandcamp::SearchResultItem::Album(a) => Some((a.band_id, a.album_id)),
                        _ => None,
                    })
                    .take(4)
                    .collect()
            };
            let fetched: Vec<bandcamp::Album> = std::thread::scope(|s| {
                let direct_handles: Vec<_> = direct
                    .iter()
                    .map(|&(band, item)| s.spawn(move || fetch_tralbum(band, item, "t")))
                    .collect();
                let expand_handles: Vec<_> = expand
                    .iter()
                    .map(|&(band, item)| s.spawn(move || fetch_tralbum(band, item, "a")))
                    .collect();
                direct_handles
                    .into_iter()
                    .chain(expand_handles)
                    .filter_map(|h| h.join().ok()?.ok())
                    .collect()
            });
            let mut seen = std::collections::HashSet::new();
            let tracks = fetched
                .iter()
                .flat_map(album_tracks)
                .filter(|t| {
                    t.provider_id(ProviderId::Bandcamp)
                        .is_some_and(|id| seen.insert(id.to_string()))
                })
                .take(SEARCH_PAGE_SIZE)
                .collect();
            Ok((tracks, SearchTab::Songs))
        }
    }
}

pub fn search_more(query: &str, offset: usize) -> Result<Vec<Track>> {
    Ok(search(query, SearchScope::Songs, offset)?.0)
}

pub fn browse(id: &str, kind: &str) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    if kind == "artist" {
        let artist = fetch_artist(id)?;
        let entries: Vec<(u64, u64, &'static str)> = artist
            .discography
            .iter()
            .take(4)
            .map(|entry| {
                let item_type = match entry.item_type {
                    bandcamp::ArtistDiscographyEntryType::Album => "a",
                    bandcamp::ArtistDiscographyEntryType::Track => "t",
                };
                (entry.band_id, entry.id, item_type)
            })
            .collect();
        let tracks: Vec<Track> = std::thread::scope(|s| {
            entries
                .into_iter()
                .map(|(band, id, kind)| s.spawn(move || fetch_tralbum(band, id, kind)))
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|h| h.join().ok()?.ok())
                .flat_map(|album| album_tracks(&album))
                .take(50)
                .collect()
        });
        return Ok((tracks, None));
    }
    let (band, item, kind) = split_release_id(id)?;
    let album = fetch_tralbum(band, item, &kind)?;
    let meta = AlbumMeta {
        badge: if kind == "t" {
            "Single".to_string()
        } else {
            "Album".to_string()
        },
        date: album.release_date.format("%Y").to_string(),
        thumbnail: sized_thumb!(album.image),
    };
    Ok((album_tracks(&album), Some(meta)))
}

pub fn fetch_artist_page(
    id: &str,
    kinds: &[crate::providers::ArtistDataKind],
) -> Result<ArtistPage> {
    use crate::providers::ArtistDataKind as K;

    let artist = fetch_artist(id)?;
    let mut page = ArtistPage::default();
    if K::Header.wanted(kinds) {
        let mut stats = Vec::new();
        if let Some(location) = &artist.location {
            if !location.is_empty() {
                stats.push(("Location".to_string(), location.clone()));
            }
        }
        page.header = Some(ArtistHeader {
            image: sized_thumb!(artist.image),
            stats,
            description: artist.bio.clone().unwrap_or_default(),
        });
    }
    if K::Popular.wanted(kinds) {
        if let Some(entry) = artist.discography.first() {
            let item_type = match entry.item_type {
                bandcamp::ArtistDiscographyEntryType::Album => "a",
                bandcamp::ArtistDiscographyEntryType::Track => "t",
            };
            if let Ok(album) = fetch_tralbum(entry.band_id, entry.id, item_type) {
                page.popular = album_tracks(&album);
            }
        }
    }
    if K::Albums.wanted(kinds) {
        page.albums = artist
            .discography
            .iter()
            .map(|e| {
                let single = matches!(e.item_type, bandcamp::ArtistDiscographyEntryType::Track);
                ArtistAlbumCard {
                    id: format!("{}:{}:{}", e.band_id, e.id, if single { "t" } else { "a" }),
                    title: e.title.clone(),
                    date: e.release_date.format("%Y").to_string(),
                    badge: if single {
                        "Single".to_string()
                    } else {
                        "Album".to_string()
                    },
                    thumbnail: sized_thumb!(e.image),
                }
            })
            .collect();
    }
    Ok(page)
}

pub fn resolve_artist_id(name: &str) -> Result<Option<String>> {
    let items = block_on(bandcamp::search(name))?;
    Ok(items.iter().find_map(|item| match item {
        bandcamp::SearchResultItem::Artist(a) => Some(a.url.clone()),
        _ => None,
    }))
}

pub fn resolve_id(track: &Track) -> Result<Option<Track>> {
    let query = track.search_query();
    let items = block_on(bandcamp::search(&query))?;
    let found = items
        .iter()
        .find_map(|item| match item {
            bandcamp::SearchResultItem::Track(t) => Some((t.band_id, t.track_id)),
            _ => None,
        })
        .and_then(|(band, item)| fetch_tralbum(band, item, "t").ok())
        .and_then(|album| {
            album
                .tracks
                .first()
                .and_then(|t| track_from_album_track(&album, t))
        });
    Ok(found)
}

pub fn download(track: &Track, download_dir: &std::path::Path) -> Result<String> {
    use std::io::Write as _;

    let url = track
        .provider_url(ProviderId::Bandcamp)
        .unwrap_or_else(|| track.primary_url())
        .to_string();
    let id = track
        .provider_id(ProviderId::Bandcamp)
        .unwrap_or("download");
    let output_path = super::download_file_path(download_dir, id);
    let mut body = super::http_agent()
        .get(&url)
        .header("User-Agent", "curl/8.5.0")
        .call()
        .context("Bandcamp download failed")?
        .body_mut()
        .read_to_vec()
        .context("Bandcamp download body unreadable")?;
    if body.is_empty() {
        anyhow::bail!("Bandcamp download was empty");
    }
    let mut file = std::fs::File::create(&output_path).context("download file not writable")?;
    file.write_all(&body).context("download write failed")?;
    body.clear();
    Ok(output_path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_id_round_trip() {
        let (band, item, kind) = split_release_id("3752216131:83593492:a").unwrap();
        assert_eq!(
            (band, item, kind.as_str()),
            (3_752_216_131, 83_593_492, "a")
        );
        assert!(split_release_id("nope").is_err());
    }
}
