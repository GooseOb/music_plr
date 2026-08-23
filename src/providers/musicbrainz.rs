//! `MusicBrainz` provider. It resolves canonical track/artist ids (MBIDs) and
//! rich metadata but provides no audio streaming or download. A track found
//! here carries only a `MusicBrainz` id; playing or downloading it triggers a
//! fallback resolution on the default (stream+download) provider. All requests
//! go through the `musicbrainz_rs` crate, which builds the correct WS/2
//! queries (search, `recording?artist=`, `release?inc=recordings`) and
//! deserializes the typed responses.

use crate::{
    providers::{
        ArtistAlbumCard, ArtistHeader, ArtistPage, CardData, ProviderId, SearchScope, SearchTab,
    },
    theme::SEARCH_PAGE_SIZE,
    types::{Track, TrackAlbum},
};
use anyhow::Result;
use musicbrainz_rs::{
    entity::{artist::Artist, recording::Recording, release::Release},
    Browse, Fetch, Search,
};

/// Map a `musicbrainz_rs` `Recording` (from a song search or artist browse)
/// into a `Track`. The crate populates `artist_credit` and `releases` only
/// when the matching includes are requested, so this reads them defensively.
fn from_mb_recording(rec: &Recording) -> Track {
    let release = rec.releases.as_ref().and_then(|rs| rs.first());
    let album = release.map(|r| TrackAlbum {
        name: r.title.clone(),
        id: r.id.clone(),
    });
    let thumbnail = release
        .map(|r| format!("https://coverartarchive.org/release/{}/front", r.id))
        .unwrap_or_default();
    from_mb_track(rec, album.as_ref(), &thumbnail)
}

/// Shared mapper from a `musicbrainz_rs` `Recording` to a `Track`. The album
/// and thumbnail are passed in so the release-browse path can attach the
/// enclosing release's title/MBID rather than relying on the recording's own
/// (often empty) `releases` list.
fn from_mb_track(rec: &Recording, album: Option<&TrackAlbum>, thumbnail: &str) -> Track {
    let artist_credit = rec.artist_credit.as_ref().and_then(|ac| ac.first());
    let artist = artist_credit.map(|a| a.name.clone()).unwrap_or_default();
    let artist_id = artist_credit.map(|a| a.artist.id.clone());

    Track::from_provider(
        ProviderId::MusicBrainz,
        rec.id.clone(),
        String::new(),
        rec.title.clone(),
        artist,
        rec.length.unwrap_or(0) / 1000,
        thumbnail.to_string(),
        album.cloned(),
        artist_id,
    )
}

pub fn search(query: &str, scope: SearchScope, offset: usize) -> (Vec<Track>, SearchTab) {
    let limit = SEARCH_PAGE_SIZE as u8;
    let offset = offset as u16;
    match scope {
        SearchScope::Artists => {
            let cards = Artist::search(format!("artist:{query}"))
                .limit(limit)
                .offset(offset)
                .execute()
                .map(|r| {
                    r.entities
                        .into_iter()
                        .map(|a| CardData {
                            id: a.id,
                            title: a.name,
                            subtitle: String::new(),
                            thumbnail: String::new(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            (Vec::new(), SearchTab::Artists(cards))
        }
        SearchScope::Albums => {
            let cards = Release::search(format!("release:{query}"))
                .limit(limit)
                .offset(offset)
                .execute()
                .map(|r| {
                    r.entities
                        .into_iter()
                        .map(|rel| CardData {
                            id: rel.id,
                            title: rel.title,
                            subtitle: rel
                                .artist_credit
                                .map(|credit| {
                                    credit
                                        .iter()
                                        .map(|a| a.name.clone())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default(),
                            thumbnail: String::new(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            (Vec::new(), SearchTab::Albums(cards))
        }
        _ => {
            let tracks = Recording::search(format!("recording:{query}"))
                .limit(limit)
                .offset(offset)
                .execute()
                .map(|r| {
                    r.entities
                        .into_iter()
                        .map(|t| from_mb_recording(&t))
                        .collect()
                })
                .unwrap_or_default();
            (tracks, SearchTab::Songs)
        }
    }
}

pub fn browse(id: &str, kind: &str) -> Result<Vec<Track>> {
    match kind {
        "artist" => {
            let result = Recording::browse()
                .by_artist(id)
                .with_artist_credits()
                .execute()?;
            Ok(result
                .entities
                .into_iter()
                .map(|r| from_mb_recording(&r))
                .collect())
        }
        "album" => {
            let release = Release::fetch()
                .id(id)
                .with_recordings()
                .with_artist_credits()
                .execute()?;
            let album = TrackAlbum {
                name: release.title.clone(),
                id: release.id.clone(),
            };
            let thumbnail = format!("https://coverartarchive.org/release/{}/front", release.id);
            let tracks = release
                .media
                .into_iter()
                .flatten()
                .flat_map(|medium| medium.tracks.into_iter().flatten())
                .filter_map(|track| {
                    track
                        .recording
                        .as_ref()
                        .map(|rec| from_mb_track(rec, Some(&album), &thumbnail))
                })
                .collect();
            Ok(tracks)
        }
        _ => anyhow::bail!("unsupported MusicBrainz browse kind: {kind}"),
    }
}

/// Fetch the rich artist page: header stats (country, life-span, tags) and
/// the discography as release groups (albums/EPs/singles with real dates and
/// Cover Art Archive covers). Sections `MusicBrainz` cannot serve (popular
/// tracks, playlists, related artists) come back empty.
pub fn fetch_artist_page(
    id: &str,
    kinds: &[crate::providers::ArtistDataKind],
) -> Result<ArtistPage> {
    use crate::providers::ArtistDataKind as K;
    use musicbrainz_rs::entity::release_group::ReleaseGroupPrimaryType as PrimaryType;

    // Header and albums come from one fetch; skip it entirely when neither
    // is requested (MB cannot serve popular/playlists/related anyway).
    if !K::Header.wanted(kinds) && !K::Albums.wanted(kinds) {
        return Ok(ArtistPage::default());
    }
    let artist = Artist::fetch()
        .id(id)
        .with_release_groups()
        .with_tags()
        .execute()?;
    let mut stats = Vec::new();
    if let Some(country) = &artist.country {
        if !country.is_empty() {
            stats.push(("Country".to_string(), country.clone()));
        }
    }
    if let Some(span) = &artist.life_span {
        let begin = span.begin.as_ref().map(|d| d.0.clone()).unwrap_or_default();
        let end = span.end.as_ref().map(|d| d.0.clone()).unwrap_or_default();
        if !begin.is_empty() {
            let (label, value) = if end.is_empty() {
                ("Formed", begin)
            } else {
                ("Active", format!("{begin}\u{2013}{end}"))
            };
            stats.push((label.to_string(), value));
        }
    }
    let top_tags: Vec<String> = artist
        .tags
        .as_ref()
        .map(|ts| {
            let mut ts: Vec<&musicbrainz_rs::entity::tag::Tag> = ts.iter().collect();
            ts.sort_by_key(|t| std::cmp::Reverse(t.count));
            ts.iter().take(5).map(|t| t.name.clone()).collect()
        })
        .unwrap_or_default();
    if !top_tags.is_empty() {
        stats.push(("Tags".to_string(), top_tags.join(", ")));
    }
    let albums = artist
        .release_groups
        .unwrap_or_default()
        .iter()
        .map(|rg| ArtistAlbumCard {
            id: rg.id.clone(),
            title: rg.title.clone(),
            date: rg
                .first_release_date
                .as_ref()
                .map(|d| d.0.chars().take(4).collect())
                .unwrap_or_default(),
            badge: match rg.primary_type.as_ref() {
                Some(PrimaryType::Album) => "Album".to_string(),
                Some(PrimaryType::Ep) => "EP".to_string(),
                Some(PrimaryType::Single) => "Single".to_string(),
                _ => String::new(),
            },
            thumbnail: format!("https://coverartarchive.org/release-group/{}/front", rg.id),
        })
        .collect();
    Ok(ArtistPage {
        header: Some(ArtistHeader {
            image: String::new(),
            stats,
            description: artist.disambiguation.clone(),
        }),
        popular: Vec::new(),
        albums,
        playlists: Vec::new(),
        related: Vec::new(),
    })
}

/// Resolve an artist name to an MBID via artist search. Returns `Ok(None)`
/// when nothing matched.
#[allow(clippy::unnecessary_wraps)]
pub fn resolve_artist_id(name: &str) -> Result<Option<String>> {
    Ok(Artist::search(format!("artist:{name}"))
        .execute()
        .ok()
        .and_then(|r| r.entities.into_iter().next())
        .map(|a| a.id))
}

pub fn search_more(query: &str, offset: usize) -> Vec<Track> {
    search(query, SearchScope::Songs, offset).0
}

/// Resolve a logical track to a `MusicBrainz` recording MBID. Returns
/// `Ok(None)` when no match is found (not an error).
#[allow(clippy::unnecessary_wraps)]
pub fn resolve_id(track: &Track) -> Result<Option<Track>> {
    let q = track.search_query();
    Ok(Recording::search(format!("recording:{q}"))
        .execute()
        .ok()
        .and_then(|r| r.entities.into_iter().next())
        .map(|rec| from_mb_recording(&rec)))
}
