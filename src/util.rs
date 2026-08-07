pub fn format_duration(secs: u32) -> String {
    if secs > 0 {
        format!("{}:{:02}", secs / 60, secs % 60)
    } else {
        "--:--".to_string()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(0), "--:--");
        assert_eq!(format_duration(30), "0:30");
        assert_eq!(format_duration(59), "0:59");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(60), "1:00");
        assert_eq!(format_duration(90), "1:30");
        assert_eq!(format_duration(369), "6:09");
        assert_eq!(format_duration(3600), "60:00");
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
}
