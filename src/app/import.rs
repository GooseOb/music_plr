//! Playlist import dialog state and the filename-pattern matching engine
//! used by the "File list" import method.
//!
//! A filename pattern is a literal string with `{name}` / `{artist}` /
//! `{album}` placeholders (plus the wildcards `{ext}` and `{*}`) that extract
//! track metadata from a file's name. Multiple patterns can be supplied; a
//! file is parsed by the first pattern that matches. Two patterns "conflict"
//! when they share the same literal skeleton yet assign different named roles
//! to a slot, so the same file could be parsed with contradictory metadata
//! (e.g. `{name} - {artist}.{ext}` vs `{artist} - {name}.{ext}`).

use std::{collections::HashMap, path::Path};

use crate::{
    providers::{ProviderId, ProviderTrack},
    types::{Track, TrackAlbum},
};

/// Which import source the dialog is currently configured for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportMethod {
    /// Read an existing `playlists.json` and merge its playlists in.
    #[default]
    Native,
    /// Build one playlist from audio files in a folder, parsing each name
    /// against the configured filename patterns.
    FileList,
    /// Build one playlist from a CSV file, mapping columns to name/artist/album.
    Csv,
}

/// Which CSV column field a message is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportCsvField {
    Name,
    Artist,
    Album,
}

/// Known CSV column-header presets. Selecting one fills the column fields with
/// a recognised export's headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CsvPreset {
    /// Generic lowercase headers (`name`/`artist`/`album`).
    #[default]
    Default,
    /// Spotify playlists exported from <https://exportify.net/>
    /// (columns "Track Name","Album Name","Artist Name(s)").
    Exportify,
}

impl CsvPreset {
    /// The (name, artist, album) column headers this preset fills in.
    pub fn columns(self) -> (String, String, String) {
        match self {
            CsvPreset::Default => (
                "name".to_string(),
                "artist".to_string(),
                "album".to_string(),
            ),
            CsvPreset::Exportify => (
                "Track Name".to_string(),
                "Artist Name(s)".to_string(),
                "Album Name".to_string(),
            ),
        }
    }
}

/// Live state of the "Import playlist" popup.
#[derive(Debug, Clone)]
pub struct ImportPlaylistDialog {
    pub method: ImportMethod,
    pub csv_preset: CsvPreset,
    /// CSV column headers mapped onto the track fields. Empty means "skip".
    pub csv_name_col: String,
    pub csv_artist_col: String,
    pub csv_album_col: String,
    /// Filename patterns for the File-list method (first match wins).
    pub patterns: Vec<String>,
    /// Playlist name for the CSV / File-list methods. Empty falls back to the
    /// selected file or folder name.
    pub playlist_name: String,
}

impl Default for ImportPlaylistDialog {
    fn default() -> Self {
        Self {
            method: ImportMethod::default(),
            csv_preset: CsvPreset::default(),
            csv_name_col: "name".to_string(),
            csv_artist_col: "artist".to_string(),
            csv_album_col: "album".to_string(),
            patterns: vec!["{artist} - {name} - {album}.{ext}".to_string()],
            playlist_name: String::new(),
        }
    }
}

impl ImportPlaylistDialog {
    /// Return the two patterns (if any) that overlap ambiguously, so the UI
    /// can disable selection and surface an error.
    pub fn conflict_pair(&self) -> Option<(String, String)> {
        let items: Vec<(String, Vec<Part>)> = self
            .patterns
            .iter()
            .map(|p| (p.trim().to_string(), parse_pattern(p)))
            .filter(|(p, _)| !p.is_empty())
            .collect();
        for i in 0..items.len() {
            for j in (i + 1)..items.len() {
                if same_skeleton(&items[i].1, &items[j].1)
                    && roles_conflict(&items[i].1, &items[j].1)
                {
                    return Some((items[i].0.clone(), items[j].0.clone()));
                }
            }
        }
        None
    }

    /// Fill the CSV column fields with the selected preset's headers.
    pub fn apply_csv_preset(&mut self, preset: CsvPreset) {
        self.csv_preset = preset;
        let (name, artist, album) = preset.columns();
        self.csv_name_col = name;
        self.csv_artist_col = artist;
        self.csv_album_col = album;
    }

    /// Whether the Select action is currently allowed.
    pub fn can_select(&self) -> bool {
        match self.method {
            ImportMethod::Native | ImportMethod::Csv => true,
            ImportMethod::FileList => {
                self.conflict_pair().is_none() && self.patterns.iter().any(|p| !p.trim().is_empty())
            }
        }
    }
}

/// One token of a parsed pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Lit(String),
    Var(Role),
}

/// The field a `{placeholder}` slot contributes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Role {
    Name,
    Artist,
    Album,
    Other,
    Wildcard,
}

fn role_for(token: &str) -> Role {
    match token.trim().to_lowercase().as_str() {
        "name" => Role::Name,
        "artist" => Role::Artist,
        "album" => Role::Album,
        "ext" | "*" => Role::Wildcard,
        _ => Role::Other,
    }
}

/// Split a pattern into literal runs and `{placeholder}` slots.
fn parse_pattern(pattern: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut token = String::new();
            for nc in chars.by_ref() {
                if nc == '}' {
                    break;
                }
                token.push(nc);
            }
            if !lit.is_empty() {
                parts.push(Part::Lit(std::mem::take(&mut lit)));
            }
            parts.push(Part::Var(role_for(&token)));
        } else {
            lit.push(c);
        }
    }
    if !lit.is_empty() {
        parts.push(Part::Lit(lit));
    }
    parts
}

/// Two patterns have the same skeleton when they share the same literals in
/// the same positions and the same number of slots (roles may differ).
fn same_skeleton(a: &[Part], b: &[Part]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).all(|(x, y)| match (x, y) {
        (Part::Lit(s1), Part::Lit(s2)) => s1 == s2,
        (Part::Var(_), Part::Var(_)) => true,
        _ => false,
    })
}

/// Two same-skeleton patterns conflict when some slot is a named role in both
/// yet a different one (e.g. name vs artist), so one file yields contradictory
/// metadata. Wildcard-vs-wildcard or wildcard-vs-role slots don't conflict.
fn roles_conflict(a: &[Part], b: &[Part]) -> bool {
    a.iter().zip(b).any(|(x, y)| match (x, y) {
        (Part::Var(r1), Part::Var(r2))
            if matches!(r1, Role::Name | Role::Artist | Role::Album | Role::Other)
                && matches!(r2, Role::Name | Role::Artist | Role::Album | Role::Other) =>
        {
            r1 != r2
        }
        _ => false,
    })
}

/// Match `text` (a filename) against `parts`, writing any named-role captures
/// into `out`. `byte_offsets` maps char indices to byte boundaries so slicing
/// stays valid for multibyte filenames; `pos`/`end` are char indices.
fn match_parts(
    parts: &[Part],
    text: &str,
    byte_offsets: &[usize],
    pos: usize,
    out: &mut HashMap<Role, String>,
) -> bool {
    let Some((head, rest)) = parts.split_first() else {
        return pos == byte_offsets.len() - 1;
    };
    match head {
        Part::Lit(l) => {
            text[byte_offsets[pos]..].starts_with(l.as_str())
                && match_parts(rest, text, byte_offsets, pos + l.chars().count(), out)
        }
        Part::Var(role) => {
            // Locate the next literal so the variable can be bounded.
            let idx = rest.iter().position(|p| matches!(p, Part::Lit(_)));
            if let Some(idx) = idx {
                let Part::Lit(s) = &rest[idx] else {
                    unreachable!()
                };
                let lit = s;
                let lit_chars = lit.chars().count();
                // `end` is the char index where the next literal begins.
                if byte_offsets.len() - 1 < lit_chars {
                    return false;
                }
                for end in pos..=(byte_offsets.len() - 1 - lit_chars) {
                    if text[byte_offsets[end]..].starts_with(lit.as_str()) {
                        if matches!(role, Role::Name | Role::Artist | Role::Album | Role::Other) {
                            out.insert(
                                *role,
                                text[byte_offsets[pos]..byte_offsets[end]].to_string(),
                            );
                        }
                        if match_parts(&rest[idx + 1..], text, byte_offsets, end + lit_chars, out) {
                            return true;
                        }
                        out.remove(role);
                    }
                }
                false
            } else {
                let content = text[byte_offsets[pos]..].to_string();
                if matches!(role, Role::Name | Role::Artist | Role::Album | Role::Other) {
                    out.insert(*role, content);
                }
                true
            }
        }
    }
}

/// Parse `filename` (a file's base name, including extension) against the
/// supplied patterns; returns `(name, artist, album)` from the first match.
pub(crate) fn parse_filename(
    patterns: &[String],
    filename: &str,
) -> Option<(String, String, String)> {
    // Byte offset of every char boundary (plus one past the end) so the
    // matcher can slice `&str` safely even with multibyte filenames.
    let byte_offsets: Vec<usize> = std::iter::once(0)
        .chain(filename.char_indices().map(|(i, _)| i))
        .chain(std::iter::once(filename.len()))
        .collect();
    for raw in patterns {
        let pattern = raw.trim();
        if pattern.is_empty() {
            continue;
        }
        let parts = parse_pattern(pattern);
        if parts.is_empty() {
            continue;
        }
        let mut out = HashMap::new();
        if match_parts(&parts, filename, &byte_offsets, 0, &mut out) {
            let name = out.get(&Role::Name).cloned().unwrap_or_default();
            let artist = out.get(&Role::Artist).cloned().unwrap_or_default();
            let album = out.get(&Role::Album).cloned().unwrap_or_default();
            return Some((name, artist, album));
        }
    }
    None
}

/// Build a `Local` track that carries only metadata (no playable file), used
/// by the CSV import where only name/artist/album are known.
pub(crate) fn build_reference_track(title: String, artist: String, album: String) -> Track {
    let mut providers = std::collections::HashMap::new();
    let album_pt = if album.is_empty() {
        None
    } else {
        Some(TrackAlbum {
            name: album.clone(),
            id: album,
        })
    };
    providers.insert(
        ProviderId::Local,
        ProviderTrack {
            id: title.clone(),
            url: String::new(),
            artist_id: None,
            duration: 0,
            thumbnail: String::new(),
            album: album_pt,
            play_count: 0,
        },
    );
    Track {
        title,
        artist,
        source: ProviderId::Local,
        providers,
    }
}

/// Build a playable `Local` track from a real audio file, using the parsed
/// `name`/`artist`/`album` (falling back to the file stem for the title).
pub(crate) fn build_file_track(path: &Path, name: String, artist: String, album: String) -> Track {
    let path_str = path.to_string_lossy().to_string();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let title = if name.is_empty() { stem.clone() } else { name };
    let duration = crate::util::try_probe_duration(&path_str).unwrap_or(0);
    let mut providers = std::collections::HashMap::new();
    let album_pt = if album.is_empty() {
        None
    } else {
        Some(TrackAlbum {
            name: album.clone(),
            id: album,
        })
    };
    providers.insert(
        ProviderId::Local,
        ProviderTrack {
            id: stem,
            url: path_str,
            artist_id: None,
            duration,
            thumbnail: String::new(),
            album: album_pt,
            play_count: 0,
        },
    );
    Track {
        title,
        artist,
        source: ProviderId::Local,
        providers,
    }
}

/// Recursively collect audio files under `dir` into `out`.
pub(crate) fn gather_audio_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let exts = ["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            gather_audio_files(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if exts.contains(&ext.to_lowercase().as_str()) {
                out.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_pattern() {
        let (name, artist, album) = parse_filename(
            &["{name} - {artist} - {album}.{ext}".into()],
            "Song - The Band - Debut.mp3",
        )
        .unwrap();
        assert_eq!(name, "Song");
        assert_eq!(artist, "The Band");
        assert_eq!(album, "Debut");
    }

    #[test]
    fn parse_first_matching_pattern_wins() {
        let patterns = vec![
            "{album}/{name}.{ext}".to_string(),
            "{name} - {artist}.{ext}".to_string(),
        ];
        let (name, artist, _) = parse_filename(&patterns, "Song - The Band.mp3").unwrap();
        assert_eq!(name, "Song");
        assert_eq!(artist, "The Band");
    }

    #[test]
    fn unmatched_file_returns_none() {
        assert!(parse_filename(&["{name} - {artist}.{ext}".into()], "justasong.mp3").is_none());
    }

    #[test]
    fn multibyte_filename_parses() {
        let (name, artist, album) = parse_filename(
            &["{name} - {artist} - {album}.{ext}".into()],
            "Schrödinger - Mötley Crüe - Über Album.mp3",
        )
        .unwrap();
        assert_eq!(name, "Schrödinger");
        assert_eq!(artist, "Mötley Crüe");
        assert_eq!(album, "Über Album");
    }

    #[test]
    fn conflicting_skeletons_detected() {
        let dialog = ImportPlaylistDialog {
            patterns: vec![
                "{name} - {artist}.{ext}".to_string(),
                "{artist} - {name}.{ext}".to_string(),
            ],
            ..Default::default()
        };
        assert!(dialog.conflict_pair().is_some());
    }

    #[test]
    fn non_conflicting_skeletons_ok() {
        let dialog = ImportPlaylistDialog {
            patterns: vec![
                "{name} - {artist}.{ext}".to_string(),
                "{name} - {artist}.{mp3}".to_string(),
            ],
            ..Default::default()
        };
        assert!(dialog.conflict_pair().is_none());
    }

    #[test]
    fn wildcard_ext_not_conflicting() {
        let dialog = ImportPlaylistDialog {
            patterns: vec![
                "{name} - {artist}.{ext}".to_string(),
                "{name} - {artist}.{*}".to_string(),
            ],
            ..Default::default()
        };
        assert!(dialog.conflict_pair().is_none());
    }
}
