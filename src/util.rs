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
    fn remove_at_empty() {
        let mut v: Vec<usize> = vec![];
        assert_eq!(remove_at(&mut v, &[0, 1]), 0);
    }
}
