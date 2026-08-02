pub fn format_duration(secs: u32) -> String {
    if secs > 0 {
        format!("{}:{:02}", secs / 60, secs % 60)
    } else {
        "--:--".to_string()
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
}
