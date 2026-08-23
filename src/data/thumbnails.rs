use std::{collections::HashMap, path::PathBuf};

fn thumbnails_dir() -> PathBuf {
    super::cache_path("thumbnails")
}

/// Shared `ureq` agent with connect + overall timeouts so a stalled CDN
/// can't hang a thumbnail thread indefinitely.
fn http_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            .build()
            .new_agent()
    })
}

pub(crate) fn thumbnail_path(video_id: &str) -> PathBuf {
    thumbnails_dir().join(format!("{video_id}.jpg"))
}

pub(crate) fn thumbnail_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg")
}

pub fn download(video_id: &str, url: &str) {
    let path = thumbnail_path(video_id);
    if path.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(thumbnails_dir());
    let url = if url.is_empty() {
        &thumbnail_url(video_id)
    } else {
        url
    };
    match http_agent().get(url).call() {
        Ok(resp) => {
            let mut body = resp.into_body();
            let mut reader = body.as_reader();
            if let Ok(mut file) = std::fs::File::create(&path) {
                if std::io::copy(&mut reader, &mut file).is_err() {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to download {video_id}: {e}");
        }
    }
}

#[derive(Default)]
pub struct ThumbnailIndex {
    entries: HashMap<String, Option<PathBuf>>,
    pending: Vec<(String, String)>,
}

impl ThumbnailIndex {
    /// Build the index from the thumbnails directory. Every existing `.jpg`
    /// becomes an entry `id -> Some(path)`; missing thumbnails are added lazily
    /// via [`ensure`].
    pub fn load() -> Self {
        let mut entries = HashMap::new();
        let dir = thumbnails_dir();
        if let Ok(read) = std::fs::read_dir(&dir) {
            for entry in read.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jpg") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        entries.insert(stem.to_string(), Some(path));
                    }
                }
            }
        }
        Self {
            entries,
            pending: Vec::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&PathBuf> {
        self.entries.get(id).and_then(|p| p.as_ref())
    }

    pub fn ensure(&mut self, id: &str, url: &str) -> Option<PathBuf> {
        if let Some(Some(path)) = self.entries.get(id) {
            return Some(path.clone());
        }
        if !url.is_empty() && !self.entries.contains_key(id) {
            self.entries.insert(id.to_string(), None);
            self.pending.push((id.to_string(), url.to_string()));
        }
        None
    }

    pub fn mark_downloaded(&mut self, id: &str) {
        let path = thumbnail_path(id);
        if path.exists() {
            self.entries.insert(id.to_string(), Some(path));
        }
    }

    pub fn drain_pending(&mut self) -> Option<Vec<(String, String)>> {
        if self.pending.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }
}
