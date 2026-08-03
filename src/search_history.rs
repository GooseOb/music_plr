use serde::{Deserialize, Serialize};

/// User data: the persisted list of past search queries.
/// Preferences (`max_visible`, `max_stored`) live in `config.rs`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchHistory {
    queries: Vec<String>,
}

fn store_path() -> std::path::PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("", "", "music_plr") {
        dirs.config_dir().join("search_history.json")
    } else {
        std::path::PathBuf::from("search_history.json")
    }
}

impl SearchHistory {
    pub fn load() -> Self {
        std::fs::read_to_string(store_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    #[cfg(not(test))]
    pub fn save(&self) {
        let path = store_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }

    #[cfg(test)]
    pub fn save(&self) {}

    pub fn get(&self) -> &[String] {
        &self.queries
    }

    pub fn contains(&self, query: &str) -> bool {
        self.queries.iter().any(|q| q == query)
    }

    pub fn push(&mut self, query: String, max_stored: usize) {
        if self.queries.iter().any(|q| q == &query) {
            return;
        }
        self.queries.push(query);
        if self.queries.len() > max_stored {
            self.queries.remove(0);
        }
        self.save();
    }

    pub fn remove(&mut self, query: &str) {
        self.queries.retain(|q| q != query);
        self.save();
    }

    pub fn filtered(&self, query_lower: &str) -> Vec<String> {
        if query_lower.is_empty() {
            return self.queries.clone();
        }
        self.queries
            .iter()
            .filter(|q| crate::util::fuzzy_match(query_lower, &q.to_lowercase()))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_dedup() {
        let mut h = SearchHistory::default();
        h.push("abc".into(), 100);
        h.push("def".into(), 100);
        h.push("abc".into(), 100); // duplicate, ignored
        assert_eq!(h.get(), &["abc", "def"]);
    }

    #[test]
    fn push_truncates_to_max() {
        let mut h = SearchHistory::default();
        for i in 0..10 {
            h.push(i.to_string(), 5);
        }
        assert_eq!(h.get().len(), 5);
        assert_eq!(h.get(), &["5", "6", "7", "8", "9"]);
    }

    #[test]
    fn remove_existing() {
        let mut h = SearchHistory::default();
        h.push("abc".into(), 100);
        h.push("def".into(), 100);
        h.remove("abc");
        assert_eq!(h.get(), &["def"]);
    }

    #[test]
    fn filtered_empty_query_returns_all() {
        let mut h = SearchHistory::default();
        h.push("abc".into(), 100);
        h.push("xyz".into(), 100);
        assert_eq!(h.filtered("").len(), 2);
    }

    #[test]
    fn filtered_subsequence() {
        let mut h = SearchHistory::default();
        h.push("hello world".into(), 100);
        h.push("hi there".into(), 100);
        let filtered = h.filtered("hlw");
        assert_eq!(filtered, vec!["hello world".to_string()]);
    }
}
