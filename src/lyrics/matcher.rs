//! Shared catalog-hit matching for providers that search a lyrics catalog:
//! normalize the way lyric sites write titles, then score a search result
//! against the requested track.

use super::LyricsRequest;

/// Lowercase, unify quotes, and collapse whitespace so fragments with curly
/// apostrophes or irregular spacing still compare equal.
pub(super) fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' | '\u{2032}' | '\u{2035}' | '`'
            | '\u{b4}' => out.push('\''),
            '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' => out.push('"'),
            _ => out.extend(ch.to_lowercase()),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Relevance score for a search hit, or `None` when it can't be the same
/// song. Title must overlap either way. A non-empty artist that overlaps
/// neither way is rejected unless the title is exact and the duration is
/// within a few seconds, since romanized CJK artist names often differ from
/// the catalog's native spelling. Durations, when both are known, break ties
/// in favour of the closest one.
pub(super) fn score(
    req: &LyricsRequest,
    hit_title: &str,
    hit_artist: &str,
    hit_duration: Option<u32>,
) -> Option<u32> {
    let title = normalize(&req.title);
    let artist = normalize(&req.artist);
    let hit_title = normalize(hit_title);
    let hit_artist = normalize(hit_artist);
    if title.is_empty() || hit_title.is_empty() {
        return None;
    }
    if !hit_title.contains(&title) && !title.contains(&hit_title) {
        return None;
    }
    let duration = (req.duration > 0)
        .then_some(req.duration)
        .zip(hit_duration.filter(|d| *d > 0))
        .map(|(want, hit)| want.abs_diff(hit));
    let mut score = 2;
    if !artist.is_empty() && !hit_artist.is_empty() {
        if hit_artist == artist {
            score += 3;
        } else if hit_artist.contains(&artist) || artist.contains(&hit_artist) {
            score += 2;
        } else if hit_title != title || duration.is_none_or(|diff| diff > 5) {
            return None;
        }
    }
    if hit_title == title {
        score += 2;
    }
    if let Some(diff) = duration {
        if diff <= 3 {
            score += 3;
        } else if diff <= 10 {
            score += 1;
        }
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(artist: &str, title: &str, duration: u32) -> LyricsRequest {
        LyricsRequest {
            artist: artist.into(),
            title: title.into(),
            album: String::new(),
            duration,
        }
    }

    #[test]
    fn normalize_unifies_quotes_and_space() {
        assert_eq!(
            normalize("With the lights out, it’s  less\ndangerous"),
            "with the lights out, it's less dangerous"
        );
        assert_eq!(normalize("  HELLO   World "), "hello world");
    }

    #[test]
    fn scoring_rejects_a_different_title_or_artist() {
        let req = req("Nirvana", "Smells Like Teen Spirit", 301);
        assert!(score(&req, "Smells Like Teen Spirit", "Weezer", Some(360)).is_none());
        assert!(score(&req, "Come As You Are", "Nirvana", Some(301)).is_none());
    }

    #[test]
    fn scoring_accepts_a_romanized_artist_when_title_and_duration_match() {
        let req = req("Kenshi Yonezu", "Lemon", 255);
        assert!(score(&req, "Lemon", "米津玄師", Some(255)).is_some());
        assert!(score(&req, "Lemon", "米津玄師", Some(310)).is_none());
    }

    #[test]
    fn scoring_prefers_the_closer_duration() {
        let req = req("YOASOBI", "夜に駆ける", 261);
        let exact = score(&req, "夜に駆ける", "YOASOBI", Some(261)).unwrap();
        let live = score(&req, "夜に駆ける", "YOASOBI", Some(290)).unwrap();
        assert!(exact > live);
    }
}
