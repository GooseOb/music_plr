//! Lyrics.ovh: plain-text-only lyrics over a different URL shape —
//! `/v1/{artist}/{title}` returns `{"lyrics": ...}` or 404.

use anyhow::Result;
use serde::Deserialize;

use super::{http::get_json_opt, Lyrics, LyricsProvider, LyricsRequest};
use crate::util::urlencode;

const LYRICS_OVH_BASE: &str = "https://api.lyrics.ovh/v1";

#[derive(Deserialize)]
struct OvhBody {
    lyrics: String,
}

pub(super) fn fetch(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    let url = format!(
        "{}/{}/{}",
        LYRICS_OVH_BASE,
        urlencode(&req.artist),
        urlencode(&req.title)
    );
    let resp: OvhBody = match get_json_opt(&url, "Lyrics.ovh")? {
        Some(body) => body,
        None => return Ok(None),
    };
    let plain = resp.lyrics.trim().to_string();
    if plain.is_empty() {
        return Ok(None);
    }
    Ok(Some(Lyrics {
        lines: Vec::new(),
        plain,
        provider: LyricsProvider::LyricsOvh,
        translations: Vec::new(),
    }))
}
