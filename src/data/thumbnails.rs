use std::{collections::HashMap, fmt::Write as _, path::PathBuf};

use crate::providers::ProviderId;

fn thumbnails_dir() -> PathBuf {
    super::cache_path("thumbnails")
}

fn provider_dir(provider: ProviderId) -> PathBuf {
    thumbnails_dir().join(provider.slug())
}

fn encode_segment(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for b in id.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn decode_segment(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = name.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
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

fn ext_for_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP".as_slice()) {
        Some("webp")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

fn ext_matches_bytes(path: &std::path::Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = std::fs::read(path).unwrap_or_default();
    match ext_for_bytes(&bytes) {
        Some("jpg") => ext == "jpg" || ext == "jpeg",
        Some(kind) => ext == kind,
        None => false,
    }
}

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

fn has_image_ext(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

pub(crate) fn thumbnail_path(provider: ProviderId, id: &str) -> PathBuf {
    let dir = provider_dir(provider);
    let stem = encode_segment(id);
    if let Ok(read) = std::fs::read_dir(&dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.file_stem().and_then(|s| s.to_str()) == Some(stem.as_str())
                && has_image_ext(&path)
            {
                return path;
            }
        }
    }
    dir.join(format!("{stem}.jpg"))
}

pub(crate) fn thumbnail_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg")
}

pub fn download(provider: ProviderId, id: &str, url: &str) {
    let _ = std::fs::create_dir_all(provider_dir(provider));
    let path = thumbnail_path(provider, id);
    if path.exists() {
        if ext_matches_bytes(&path) {
            return;
        }
        let _ = std::fs::remove_file(&path);
    }
    let url = if url.is_empty() {
        &thumbnail_url(id)
    } else {
        url
    };
    match http_agent().get(url).call() {
        Ok(resp) => {
            let mut bytes = Vec::new();
            if std::io::copy(&mut resp.into_body().as_reader(), &mut bytes).is_err() {
                return;
            }
            let Some(ext) = ext_for_bytes(&bytes) else {
                tracing::warn!("Thumbnail for {id} is not an image, skipping");
                return;
            };
            let path = provider_dir(provider).join(format!("{}.{ext}", encode_segment(id)));
            if std::fs::write(&path, &bytes).is_err() {
                let _ = std::fs::remove_file(&path);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to download {id}: {e}");
        }
    }
}

#[derive(Default)]
pub struct ThumbnailIndex {
    entries: HashMap<(ProviderId, String), Option<PathBuf>>,
    pending: Vec<(ProviderId, String, String)>,
}

impl ThumbnailIndex {
    /// Build the index from the per-provider thumbnail directories. Every
    /// existing `.jpg` becomes an entry. Legacy flat files from before
    /// per-provider directories are removed (regenerable cache).
    pub fn load() -> Self {
        let mut entries = HashMap::new();
        let dir = thumbnails_dir();
        if let Ok(read) = std::fs::read_dir(&dir) {
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if has_image_ext(&path) {
                        let _ = std::fs::remove_file(&path);
                    }
                    continue;
                }
                let Some(provider) = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(ProviderId::from_slug)
                else {
                    continue;
                };
                if let Ok(read) = std::fs::read_dir(&path) {
                    for entry in read.flatten() {
                        let path = entry.path();
                        if !has_image_ext(&path) {
                            continue;
                        }
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if !ext_matches_bytes(&path) {
                                let _ = std::fs::remove_file(&path);
                                continue;
                            }
                            if let Some(id) = decode_segment(stem) {
                                entries.insert((provider, id), Some(path));
                            }
                        }
                    }
                }
            }
        }
        Self {
            entries,
            pending: Vec::new(),
        }
    }

    pub fn get(&self, provider: ProviderId, id: &str) -> Option<&PathBuf> {
        self.entries
            .get(&(provider, id.to_string()))
            .and_then(|p| p.as_ref())
    }

    pub fn ensure(&mut self, provider: ProviderId, id: &str, url: &str) {
        let key = (provider, id.to_string());
        if let Some(Some(_)) = self.entries.get(&key) {
            return;
        }
        if !url.is_empty() && !self.entries.contains_key(&key) {
            self.entries.insert(key.clone(), None);
            self.pending.push((key.0, key.1, url.to_string()));
        }
    }

    pub fn mark_downloaded(&mut self, provider: ProviderId, id: &str) {
        let path = thumbnail_path(provider, id);
        if path.exists() {
            self.entries.insert((provider, id.to_string()), Some(path));
        }
    }

    pub fn drain_pending(&mut self) -> Option<Vec<(ProviderId, String, String)>> {
        if self.pending.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_round_trip() {
        for id in ["abc123", "Cher\x1fBelieve", "https://x/y?a=b", "100%"] {
            assert_eq!(decode_segment(&encode_segment(id)).as_deref(), Some(id));
        }
        assert_eq!(encode_segment("abc-_.9"), "abc-_.9");
    }

    #[test]
    fn paths_are_namespaced() {
        let a = thumbnail_path(ProviderId::Bandcamp, "123");
        let b = thumbnail_path(ProviderId::SoundCloud, "123");
        assert_ne!(a, b);
        assert!(a.to_string_lossy().contains("bandcamp"));
    }

    #[test]
    fn ext_sniffing_matches_common_formats() {
        assert_eq!(ext_for_bytes(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]), Some("jpg"));
        assert_eq!(
            ext_for_bytes(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A]),
            Some("png")
        );
        assert_eq!(ext_for_bytes(b"GIF89a..."), Some("gif"));
        assert_eq!(ext_for_bytes(b"RIFF\x00\x00\x00\x00WEBPVP8 "), Some("webp"));
        assert_eq!(ext_for_bytes(b"<html>nope</html>"), None);
        assert_eq!(ext_for_bytes(&[]), None);
    }
}
