pub fn format_duration(secs: u32) -> std::borrow::Cow<'static, str> {
    if secs > 0 {
        format!("{}:{:02}", secs / 60, secs % 60).into()
    } else {
        "--:--".into()
    }
}

use std::fmt::Write as _;

/// Percent-encode a string for use in a URL query. Keeps the RFC 3986
/// unreserved set as-is, maps spaces to `+`, and percent-encodes everything
/// else (including `+`, `&`, `#`, `=`, and multi-byte UTF-8).
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push('+'),
            '+' => out.push_str("%2B"),
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '=' => out.push_str("%3D"),
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' => {
                out.push(c);
            }
            c => {
                for b in c.to_string().into_bytes() {
                    let _ = std::write!(out, "%{b:02X}");
                }
            }
        }
    }
    out
}

/// Returns "" for 1 item, "s" otherwise, for simple English pluralization.
pub const fn plural_suffix(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

/// Returns the duration of the audio file at `path` in seconds, or `None`
/// on any failure (missing file, unsupported codec, corrupt header, zero rate).
pub fn try_probe_duration(path: &str) -> Option<u32> {
    use std::fs::File;
    use symphonia::core::{
        formats::FormatOptions,
        io::{MediaSourceStream, MediaSourceStreamOptions},
        meta::MetadataOptions,
        probe::Hint,
    };

    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let track = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())?;

    let params = &track.codec_params;
    // `sample_rate == 0` would mean a corrupt header, which we treat as
    // undeterminable.
    let sample_rate = params.sample_rate.filter(|&r| r != 0)?;
    let n_frames = params.n_frames?;

    // `n_frames` is the number of PCM samples (symphonia's MP3/FLAC/etc.
    // demuxers scale codec frames up to PCM samples), so dividing by the
    // sample rate yields seconds.
    Some(n_frames as u32 / sample_rate)
}

pub fn fuzzy_match(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    let text = text.to_lowercase();
    let mut qi = query.chars().peekable();
    for c in text.chars() {
        if qi.peek() == Some(&c) {
            qi.next();
        }
    }
    qi.peek().is_none()
}

/// Remove the items at `indices` (in any order) from `list`, writing back to the
/// same collection only once. Indices that are out of bounds are silently
/// skipped. Returns the number of items actually removed.
///
/// This is the single canonical "remove by index" routine used by the queue,
/// playlists, and downloads views, so reordering edge-cases are tested in one
/// place.
pub fn remove_at<T>(list: &mut Vec<T>, indices: &[usize]) -> usize {
    let mut sorted: Vec<usize> = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut removed = 0;
    for &i in sorted.iter().rev() {
        if i < list.len() {
            list.remove(i);
            removed += 1;
        }
    }
    removed
}

/// Reorder `tracks` by moving the items at `indices` to `drop_idx`.
///
/// `selection` is the current set of selected indices in the list. It is
/// remapped so that the returned vector contains the **new** positions of all
/// originally-selected tracks — both the moved ones and those that merely
/// shifted due to the removal/insertion. This ensures `selected_indices` stays
/// correct regardless of whether the dragged tracks were part of the
/// selection.
pub fn reorder_tracks<T: Clone>(
    tracks: &mut Vec<T>,
    drop_idx: usize,
    indices: &[usize],
    selection: &[usize],
) -> Vec<usize> {
    let sorted_indices: Vec<usize> = {
        let mut s = indices.to_vec();
        s.sort_unstable();
        s
    };
    let extracted: Vec<T> = sorted_indices
        .iter()
        .filter_map(|&i| tracks.get(i).cloned())
        .collect();
    for &i in sorted_indices.iter().rev() {
        if i < tracks.len() {
            tracks.remove(i);
        }
    }
    let removed_before = sorted_indices.iter().filter(|&&i| i < drop_idx).count();
    let adjusted_drop = (drop_idx - removed_before).min(tracks.len());
    let new_count = extracted.len();
    for (j, track) in extracted.into_iter().enumerate() {
        tracks.insert(adjusted_drop + j, track);
    }

    let mut new_selected: Vec<usize> = Vec::with_capacity(selection.len());
    for &sel_idx in selection {
        if let Some(pos) = sorted_indices.iter().position(|&i| i == sel_idx) {
            new_selected.push(adjusted_drop + pos);
        } else {
            let removed_before_sel = sorted_indices.iter().filter(|&&i| i < sel_idx).count();
            let after_removal = sel_idx - removed_before_sel;
            let insert_shift = if after_removal >= adjusted_drop {
                new_count
            } else {
                0
            };
            new_selected.push(after_removal + insert_shift);
        }
    }
    new_selected.sort_unstable();
    new_selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Track;

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(0).as_ref(), "--:--");
        assert_eq!(format_duration(30).as_ref(), "0:30");
        assert_eq!(format_duration(59).as_ref(), "0:59");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(60).as_ref(), "1:00");
        assert_eq!(format_duration(90).as_ref(), "1:30");
        assert_eq!(format_duration(369).as_ref(), "6:09");
        assert_eq!(format_duration(3600).as_ref(), "60:00");
    }

    #[test]
    fn fuzzy_match_exact() {
        assert!(fuzzy_match("hello", "hello"));
    }

    #[test]
    fn fuzzy_match_subsequence() {
        assert!(fuzzy_match("hlo", "hello"));
        assert!(fuzzy_match("hlo", "Hello World"));
    }

    #[test]
    fn fuzzy_match_empty_query() {
        assert!(fuzzy_match("", "anything"));
    }

    #[test]
    fn fuzzy_match_no_match() {
        assert!(!fuzzy_match("xyz", "hello"));
    }

    #[test]
    fn fuzzy_match_case_insensitive() {
        assert!(fuzzy_match("HELLO", "hello"));
        assert!(fuzzy_match("HeLlO", "HELLO"));
    }

    #[test]
    fn fuzzy_match_partial() {
        assert!(fuzzy_match("ell", "hello"));
        assert!(!fuzzy_match("leh", "hello"));
    }

    #[test]
    fn remove_at_multiple() {
        let mut v = vec![0, 1, 2, 3, 4];
        assert_eq!(remove_at(&mut v, &[1, 3]), 2);
        assert_eq!(v, vec![0, 2, 4]);
    }

    #[test]
    fn remove_at_unsorted_and_dedup() {
        let mut v = vec![0, 1, 2, 3, 4];
        assert_eq!(remove_at(&mut v, &[3, 0, 3, 99]), 2);
        assert_eq!(v, vec![1, 2, 4]);
    }

    #[test]
    fn probe_duration_missing_file() {
        assert_eq!(
            try_probe_duration("/nonexistent/path/file.mp3").unwrap_or(0),
            0
        );
    }

    // ── reorder_tracks ───────────────────────────────────

    fn make_tracks(count: usize) -> Vec<Track> {
        (0..count)
            .map(|i| {
                let mut providers = std::collections::HashMap::new();
                providers.insert(
                    crate::providers::ProviderId::YouTube,
                    crate::types::ProviderTrack {
                        id: format!("id{i}"),
                        url: format!("url{i}"),
                        artist_id: None,
                    },
                );
                Track {
                    title: format!("Track {i}"),
                    artist: "Artist".into(),
                    duration: 10,
                    thumbnail: String::new(),
                    download_path: None,
                    album: None,
                    origin: crate::providers::ProviderId::YouTube,
                    providers,
                }
            })
            .collect()
    }

    #[test]
    fn move_single_not_selected_remaps_selection() {
        let mut tracks = make_tracks(5); // [id0, id1, id2, id3, id4]
        let selection = vec![1, 2]; // id1, id2 selected
                                    // Move index 4 (id4) to position 0
        let new_sel = reorder_tracks(&mut tracks, 0, &[4], &selection);
        assert_eq!(tracks_ids(&tracks), ["id4", "id0", "id1", "id2", "id3"]);
        // id1 was at 1, shifted right by 1 (id4 inserted before it) → 2
        // id2 was at 2, shifted right by 1 → 3
        assert_eq!(new_sel, [2, 3]);
    }

    #[test]
    fn move_single_selected_remaps_selection() {
        let mut tracks = make_tracks(5); // [id0, id1, id2, id3, id4]
        let selection = vec![1, 2]; // id1, id2 selected
                                    // Move index 2 (id2) to position 0
        let new_sel = reorder_tracks(&mut tracks, 0, &[2], &selection);
        assert_eq!(tracks_ids(&tracks), ["id2", "id0", "id1", "id3", "id4"]);
        // id2 was moved to position 0
        // id1 was at 1, shifted right by 1 (id2 inserted before it) → 2
        assert_eq!(new_sel, [0, 2]);
    }

    #[test]
    fn move_multiple_selected_remaps_selection() {
        let mut tracks = make_tracks(6); // [id0, id1, id2, id3, id4, id5]
        let selection = vec![1, 2, 4]; // id1, id2, id4 selected
                                       // Move indices [1, 2] (id1, id2) to position 5 (after id5)
        let new_sel = reorder_tracks(&mut tracks, 5, &[1, 2], &selection);
        assert_eq!(
            tracks_ids(&tracks),
            ["id0", "id3", "id4", "id1", "id2", "id5"]
        );
        // id1 was moved to position 3
        // id2 was moved to position 4
        // id4 was at 4, removed 2 before it (id1, id2), so after_removal = 4 - 2 = 2
        // adjusted_drop = 5 - 2 = 3; after_removal (2) < 3, so no insert_shift → 2
        assert_eq!(new_sel, [2, 3, 4]);
    }

    #[test]
    fn move_non_selected_above_selection() {
        let mut tracks = make_tracks(5); // [id0, id1, id2, id3, id4]
        let selection = vec![2, 3]; // id2, id3 selected
                                    // Move index 0 (id0) to position 4 (end)
        let new_sel = reorder_tracks(&mut tracks, 4, &[0], &selection);
        assert_eq!(tracks_ids(&tracks), ["id1", "id2", "id3", "id0", "id4"]);
        // id2 was at 2, removed 1 before it (id0), after_removal = 2 - 1 = 1
        // adjusted_drop = 4 - 1 = 3; after_removal (1) < 3, no shift → 1
        // id3 was at 3, removed 1 before it, after_removal = 3 - 1 = 2
        // adjusted_drop = 3; after_removal (2) < 3, no shift → 2
        assert_eq!(new_sel, [1, 2]);
    }

    #[test]
    fn move_non_selected_between_selected() {
        let mut tracks = make_tracks(6); // [id0, id1, id2, id3, id4, id5]
        let selection = vec![1, 3, 4]; // id1, id3, id4 selected
                                       // Move index 0 (id0) to position 2 (between id1 and id2)
        let new_sel = reorder_tracks(&mut tracks, 2, &[0], &selection);
        assert_eq!(
            tracks_ids(&tracks),
            ["id1", "id0", "id2", "id3", "id4", "id5"]
        );
        // id1 was at 1, removed 1 before it (id0), after_removal = 0
        // adjusted_drop = 2 - 1 = 1; after_removal (0) < 1, no shift → 0
        // id3 was at 3, removed 1 before it, after_removal = 2
        // adjusted_drop = 1; after_removal (2) >= 1, shift by 1 → 3
        // id4 was at 4, removed 1 before it, after_removal = 3
        // adjusted_drop = 1; after_removal (3) >= 1, shift by 1 → 4
        assert_eq!(new_sel, [0, 3, 4]);
    }

    #[test]
    fn empty_selection_returns_empty() {
        let mut tracks = make_tracks(3);
        let new_sel = reorder_tracks(&mut tracks, 0, &[1], &[]);
        assert!(new_sel.is_empty());
    }

    #[test]
    fn move_all_selected_to_front() {
        let mut tracks = make_tracks(5); // [id0, id1, id2, id3, id4]
        let selection = vec![0, 1, 2, 3, 4];
        // Move all to position 4 (they stay in order, just re-inserted)
        let new_sel = reorder_tracks(&mut tracks, 4, &[0, 1, 2, 3, 4], &selection);
        assert_eq!(tracks_ids(&tracks), ["id0", "id1", "id2", "id3", "id4"]);
        assert_eq!(new_sel, [0, 1, 2, 3, 4]);
    }

    fn tracks_ids(tracks: &[Track]) -> Vec<&str> {
        tracks
            .iter()
            .map(|t| {
                t.provider_id(crate::providers::ProviderId::YouTube)
                    .unwrap_or("")
            })
            .collect()
    }
}
