/// Abbreviated count for display (1234 -> "1.2K"). Values that would round
/// up across a unit boundary (999 500 -> "1000.0K") step to the next unit
/// instead ("1.0M").
pub fn format_count(n: u64) -> String {
    const UNITS: [(u64, char); 4] = [
        (1_000_000_000_000, 'T'),
        (1_000_000_000, 'B'),
        (1_000_000, 'M'),
        (1_000, 'K'),
    ];
    let v = n as f64;
    let Some(mut i) = UNITS.iter().position(|&(u, _)| v >= u as f64) else {
        return n.to_string();
    };
    let mut scaled = v / UNITS[i].0 as f64;
    if scaled.round() >= 1000.0 {
        i = i.saturating_sub(1);
        scaled = v / UNITS[i].0 as f64;
    }
    format!("{scaled:.1}{}", UNITS[i].1)
}

/// Abbreviated byte size for display (binary units, so the numbers line up
/// with the MB-based cache cap: 1024 MB == "1.0 GB").
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(1 << 30, "GB"), (1 << 20, "MB"), (1 << 10, "KB")];
    let Some(&(unit, label)) = UNITS.iter().find(|&&(unit, _)| bytes >= unit) else {
        return format!("{bytes} B");
    };
    format!("{:.1} {label}", bytes as f64 / unit as f64)
}

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

/// Hand `url` to the desktop's default handler (`xdg-open`/`open`/`start`).
/// The opener is spawned and not waited on — it exits once the browser has the
/// URL — so this reports only whether the launcher itself could start.
pub fn open_external(url: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", ""]);
        cmd
    };
    #[cfg(not(windows))]
    let mut cmd = std::process::Command::new(if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    });
    cmd.arg(url).spawn().map(|_| ())
}

/// Returns the duration of the audio file at `path` in seconds, or `None`
/// on any failure (missing file, unsupported codec, corrupt header, zero rate).
pub fn try_probe_duration(path: &str) -> Option<u32> {
    use std::fs::File;

    use symphonia::core::{
        formats::{probe::Hint, FormatOptions, TrackType},
        io::{MediaSourceStream, MediaSourceStreamOptions},
        meta::MetadataOptions,
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

    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;

    let track = format.default_track(TrackType::Audio)?;
    let params = track.codec_params.as_ref()?.audio()?;

    // Prefer the exact frame count when the container provides one (its
    // demuxers scale codec frames up to PCM frames); otherwise fall back to
    // the track duration in its own timebase, which also covers containers
    // whose timescale differs from the sample rate.
    let seconds = if let Some(n_frames) = track.num_frames {
        // `sample_rate == 0` would mean a corrupt header, which we treat as
        // undeterminable.
        let sample_rate = params.sample_rate.filter(|&r| r != 0)?;
        n_frames / u64::from(sample_rate)
    } else {
        let time = track.time_base?.calc_duration(track.duration?)?;
        time.as_secs_f64() as u64
    };

    u32::try_from(seconds).ok()
}

/// Latin spelling of a letter that Unicode does not decompose (`ł`, `ø`, `ß`,
/// …) or of a Cyrillic letter; `Some("")` marks letters with no Latin value
/// (the soft/hard signs), `None` means "decompose it instead".
///
/// Where a letter has competing romanizations, the spelling is chosen so the
/// common ones are all subsequences of it (`ё` → `yoe` matches both "yolka"
/// and "elka", `й` → `yi` both "tolstoy" and "tolstoi"), because matching is
/// subsequence-based rather than exact.
fn fold_form(c: char) -> Option<&'static str> {
    Some(match c {
        'ß' => "ss",
        'þ' => "th",
        'æ' => "ae",
        'œ' => "oe",
        'ı' | 'и' | 'і' => "i",
        'ł' | 'л' => "l",
        'đ' | 'ð' | 'д' => "d",
        'ħ' => "h",
        'ŋ' | 'н' => "n",
        'ŧ' | 'т' => "t",
        'ø' | 'о' => "o",
        'е' | 'э' => "e",
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' | 'ґ' => "g",
        'ё' => "yoe",
        'ж' => "zh",
        'з' => "z",
        'й' | 'ы' | 'ї' => "yi",
        'к' => "k",
        'м' => "m",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'у' | 'ў' => "u",
        'ф' => "f",
        'х' => "kh",
        'ц' => "ts",
        'ч' => "tch",
        'ш' => "sh",
        'щ' => "shch",
        'ъ' | 'ь' => "",
        'ю' => "yu",
        'я' => "ya",
        'є' => "ye",
        'ј' => "j",
        'љ' => "lj",
        'њ' => "nj",
        'ћ' => "c",
        'ђ' => "dj",
        'џ' | 'ѕ' => "dz",
        'ќ' => "kj",
        'ѓ' => "gj",
        _ => return None,
    })
}

/// One character in its matching form: lowercased, transliterated or
/// compatibility-decomposed, combining marks dropped. Unused slots stay `'\0'`.
fn fold(c: char) -> [char; 4] {
    let mut out = ['\0'; 4];
    if c.is_ascii() {
        out[0] = c;
        return out;
    }
    if let Some(spelled) = fold_form(c) {
        for (slot, folded) in out.iter_mut().zip(spelled.chars()) {
            *slot = folded;
        }
        return out;
    }
    let mut len = 0;
    unicode_normalization::char::decompose_compatible(c, |d| {
        if len < out.len() && !unicode_normalization::char::is_combining_mark(d) {
            out[len] = d;
            len += 1;
        }
    });
    out
}

/// Case-, accent- and script-insensitive characters of `text` (`Björk` →
/// `bjork`, `Straße` → `strasse`, `Кино` → `kino`).
fn folded(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars()
        .flat_map(char::to_lowercase)
        .flat_map(fold)
        .filter(|&c| c != '\0')
}

/// A query folded once, for subsequence-matching many texts with the same
/// needle (list filters run this per row on every keystroke).
pub struct FuzzyQuery {
    needles: Vec<char>,
}

impl FuzzyQuery {
    pub fn new(query: &str) -> Self {
        Self {
            needles: folded(query).collect(),
        }
    }

    /// Whether the query characters appear in `text` in order, ignoring case,
    /// accents, and compatibility forms (`motörhead` matches `Motorhead`).
    pub fn matches(&self, text: &str) -> bool {
        if self.needles.is_empty() {
            return true;
        }
        let mut needles = self.needles.iter().copied().peekable();
        for c in folded(text) {
            if needles.peek() == Some(&c) {
                needles.next();
            }
        }
        needles.peek().is_none()
    }
}

pub fn sorted_targets(indices: &[usize]) -> std::iter::Peekable<std::vec::IntoIter<usize>> {
    let mut sorted: Vec<usize> = indices.to_vec();
    sorted.sort_unstable();
    sorted.into_iter().peekable()
}

/// Remove the items at `indices` (in any order) from `list`, writing back to the
/// same collection only once. Indices that are out of bounds are silently
/// skipped. Returns the number of items actually removed.
///
/// This is the single canonical "remove by index" routine used by the queue,
/// playlists, and downloads views, so reordering edge-cases are tested in one
/// place.
pub fn remove_at<T>(list: &mut Vec<T>, indices: &[usize]) -> usize {
    if indices.is_empty() {
        return 0;
    }
    let mut targets = sorted_targets(indices);
    let original = list.len();
    let mut i = 0usize;
    list.retain(|_| {
        let drop_it = matches!(targets.peek(), Some(&r) if r == i);
        if drop_it {
            targets.next();
        }
        i += 1;
        !drop_it
    });
    original - list.len()
}

/// Move the items at `indices` to `drop_idx`, returning the resolved
/// insertion point and the number of moved items. `indices` must be sorted
/// ascending (removal runs it in reverse; release builds don't check).
/// [`reorder_tracks`] builds on this and additionally remaps a selection;
/// callers that only move (e.g. sidebar rows) use this directly.
pub fn move_items<T: Clone>(
    items: &mut Vec<T>,
    drop_idx: usize,
    indices: &[usize],
) -> (usize, usize) {
    debug_assert!(
        indices.windows(2).all(|w| w[0] <= w[1]),
        "move_items expects sorted indices"
    );
    let extracted: Vec<T> = indices
        .iter()
        .filter_map(|&i| items.get(i).cloned())
        .collect();
    let mut targets = indices.iter().peekable();
    let mut i = 0usize;
    let mut removed_before = 0;
    items.retain(|_| {
        let drop_it = matches!(targets.peek(), Some(&&r) if r == i);
        if drop_it {
            targets.next();
            removed_before += usize::from(i < drop_idx);
        }
        i += 1;
        !drop_it
    });
    let adjusted_drop = (drop_idx - removed_before).min(items.len());
    let moved = extracted.len();
    items.splice(adjusted_drop..adjusted_drop, extracted);
    (adjusted_drop, moved)
}

/// Reorder `tracks` by moving the items at `indices` to `drop_idx`.
///
/// `selection` is the current set of selected indices in the list. It is
/// remapped in place so that the returned vector contains the **new**
/// positions of all originally-selected tracks — both the moved ones and
/// those that merely shifted due to the removal/insertion. This ensures
/// `selected_indices` stays correct regardless of whether the dragged tracks
/// were part of the selection. The mapping is 1:1, so the input buffer is
/// reused exactly (no reallocation); pass it via `mem::take` to recycle the
/// live selection's allocation too.
pub fn reorder_tracks<T: Clone>(
    tracks: &mut Vec<T>,
    drop_idx: usize,
    indices: &[usize],
    selection: Vec<usize>,
) -> Vec<usize> {
    let (adjusted_drop, new_count) = move_items(tracks, drop_idx, indices);

    let mut remapped = selection;
    for sel_idx in &mut remapped {
        let prev = *sel_idx;
        *sel_idx = if let Some(pos) = indices.iter().position(|&i| i == prev) {
            adjusted_drop + pos
        } else {
            let removed_before_sel = indices.iter().filter(|&&i| i < prev).count();
            let after_removal = prev - removed_before_sel;
            let insert_shift = if after_removal >= adjusted_drop {
                new_count
            } else {
                0
            };
            after_removal + insert_shift
        };
    }
    remapped.sort_unstable();
    remapped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Track;

    #[test]
    fn formats_count_boundaries() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_234), "1.2K");
        assert_eq!(format_count(9_999), "10.0K");
        assert_eq!(format_count(12_345), "12.3K");
        assert_eq!(format_count(841_000), "841.0K");
        assert_eq!(format_count(999_499), "999.5K");
        assert_eq!(format_count(999_500), "1.0M");
        assert_eq!(format_count(1_234_567), "1.2M");
        assert_eq!(format_count(949_999_999), "950.0M");
        assert_eq!(format_count(1_000_000_000), "1.0B");
        assert_eq!(format_count(1_234_567_891), "1.2B");
    }

    #[test]
    fn format_bytes_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(1536 * 1024), "1.5 MB");
    }

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

    fn matched(query: &str, text: &str) -> bool {
        FuzzyQuery::new(query).matches(text)
    }

    #[test]
    fn fuzzy_match_exact() {
        assert!(matched("hello", "hello"));
    }

    #[test]
    fn fuzzy_match_subsequence() {
        assert!(matched("hlo", "hello"));
        assert!(matched("hlo", "Hello World"));
    }

    #[test]
    fn fuzzy_match_empty_query() {
        assert!(matched("", "anything"));
    }

    #[test]
    fn fuzzy_match_no_match() {
        assert!(!matched("xyz", "hello"));
    }

    #[test]
    fn fuzzy_match_case_insensitive() {
        assert!(matched("HELLO", "hello"));
        assert!(matched("HeLlO", "HELLO"));
    }

    #[test]
    fn fuzzy_match_partial() {
        assert!(matched("ell", "hello"));
        assert!(!matched("leh", "hello"));
    }

    #[test]
    fn fuzzy_match_ignores_accents() {
        assert!(matched("bjork", "Björk"));
        assert!(matched("motörhead", "Motorhead"));
        assert!(matched("sao paulo", "São Paulo"));
        assert!(matched("zolc", "Żółć"));
        assert!(matched("smorrebrod", "Smørrebrød"));
        assert!(matched("strasse", "Straße"));
        assert!(matched("piotr", "Piotr Łukasz"));
    }

    #[test]
    fn fuzzy_match_transliterates_cyrillic() {
        assert!(matched("kino", "Кино"));
        assert!(matched("zemfira", "Земфира"));
        assert!(matched("mumiy troll", "Мумий Тролль"));
        assert!(matched("korolev", "Королёв"));
        assert!(matched("yolka", "Ёлка"));
        assert!(matched("tolstoi", "Толстой"));
        assert!(matched("tolstoy", "Толстой"));
        assert!(matched("kharkiv", "Харків"));
        assert!(matched("kyiv", "Київ"));
        assert!(matched("minsk", "Мінск"));
        assert!(matched("tchaikovsky", "Чайковский"));
        assert!(matched("chekhov", "Чехов"));
        assert!(matched("кот", "kot"));
        assert!(matched("кино", "Kino"));
    }

    #[test]
    fn fuzzy_match_ignores_normalization() {
        let composed = "café";
        let decomposed = "cafe\u{301}";
        assert!(matched(composed, decomposed));
        assert!(matched(decomposed, composed));
        assert!(matched("cafe", decomposed));
    }

    #[test]
    fn fuzzy_match_keeps_distinct_letters_apart() {
        assert!(!matched("cat", "dog"));
        assert!(!matched("kot", "корова"));
        assert!(!matched("kot", "κοτ"));
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
                        duration: 10,
                        thumbnail: String::new(),
                        album: None,
                        play_count: 0,
                    },
                );
                Track {
                    title: format!("Track {i}"),
                    artist: "Artist".into(),
                    source: crate::providers::ProviderId::YouTube,
                    providers,
                }
            })
            .collect()
    }

    #[test]
    fn move_single_row_to_end_reports_landing() {
        let mut tracks = make_tracks(5); // [id0, id1, id2, id3, id4]
        let (adjusted_drop, moved) = move_items(&mut tracks, 5, &[1]);
        assert_eq!(tracks_ids(&tracks), ["id0", "id2", "id3", "id4", "id1"]);
        assert_eq!(adjusted_drop, 4);
        assert_eq!(moved, 1);
    }

    #[test]
    fn move_single_not_selected_remaps_selection() {
        let mut tracks = make_tracks(5); // [id0, id1, id2, id3, id4]
        let selection = vec![1, 2]; // id1, id2 selected
                                    // Move index 4 (id4) to position 0
        let new_sel = reorder_tracks(&mut tracks, 0, &[4], selection);
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
        let new_sel = reorder_tracks(&mut tracks, 0, &[2], selection);
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
        let new_sel = reorder_tracks(&mut tracks, 5, &[1, 2], selection);
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
        let new_sel = reorder_tracks(&mut tracks, 4, &[0], selection);
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
        let new_sel = reorder_tracks(&mut tracks, 2, &[0], selection);
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
        let new_sel = reorder_tracks(&mut tracks, 0, &[1], Vec::new());
        assert!(new_sel.is_empty());
    }

    #[test]
    fn move_all_selected_to_front() {
        let mut tracks = make_tracks(5); // [id0, id1, id2, id3, id4]
        let selection = vec![0, 1, 2, 3, 4];
        // Move all to position 4 (they stay in order, just re-inserted)
        let new_sel = reorder_tracks(&mut tracks, 4, &[0, 1, 2, 3, 4], selection);
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

    #[test]
    fn urlencode_keeps_unreserved() {
        assert_eq!(urlencode("abc-XYZ_0189.~"), "abc-XYZ_0189.~");
    }

    #[test]
    fn urlencode_space_becomes_plus() {
        assert_eq!(urlencode("daft punk"), "daft+punk");
    }

    #[test]
    fn urlencode_reserved_chars() {
        assert_eq!(urlencode("a+b&c=d#e"), "a%2Bb%26c%3Dd%23e");
        assert_eq!(urlencode("?/:"), "%3F%2F%3A");
        assert_eq!(urlencode("%"), "%25");
    }

    #[test]
    fn urlencode_multibyte_utf8() {
        // U+00E9 é -> UTF-8 C3 A9
        assert_eq!(urlencode("café"), "caf%C3%A9");
        assert_eq!(
            urlencode("初音ミク"),
            "%E5%88%9D%E9%9F%B3%E3%83%9F%E3%82%AF"
        );
    }
}
