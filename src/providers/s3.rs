//! `S3`-compatible object storage backend for user-configured buckets
//! (AWS `S3`, Cloudflare R2, Backblaze B2, `MinIO`, Wasabi, …).
//!
//! A bucket only holds the app's playlists, as JSON objects under the fixed
//! `playlists/` prefix; those snapshots may reference tracks from any
//! provider. `SigV4` is signed locally over `sha2` for signed requests, while
//! public buckets use plain URLs.

use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    data::{
        custom_providers::{self, CustomProvider, CustomProviderKind},
        s3_playlists::{self, S3PlaylistEntry},
    },
    providers::{AlbumMeta, PlaylistWrite, ProviderId, ServerPlaylist},
    types::Track,
};

const PLAYLISTS_PREFIX: &str = "playlists/";
const PLAYLIST_VERSION: u32 = 1;
const PLAYLIST_TTL_SECS: u64 = 300;

/// A resolved bucket configuration, built from a saved or draft provider.
#[derive(Debug, Clone)]
pub struct S3Config {
    scheme: String,
    endpoint_host: String,
    endpoint_path: String,
    region: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    path_style: bool,
    public: bool,
}

impl S3Config {
    fn from_custom(provider: &CustomProvider) -> Result<Self> {
        let settings = provider.s3().context("not an S3 provider")?;
        let bucket = settings.bucket.trim().to_string();
        if bucket.is_empty() {
            bail!("bucket name is required");
        }
        let region = if settings.region.trim().is_empty() {
            "us-east-1".to_string()
        } else {
            settings.region.trim().to_string()
        };
        let endpoint = if settings.endpoint.trim().is_empty() {
            format!("https://s3.{region}.amazonaws.com")
        } else {
            settings.endpoint.trim().trim_end_matches('/').to_string()
        };
        let endpoint = if endpoint.contains("://") {
            endpoint
        } else {
            format!("https://{endpoint}")
        };
        let (scheme, rest) = endpoint
            .split_once("://")
            .context("invalid endpoint address")?;
        let (endpoint_host, endpoint_path) = match rest.split_once('/') {
            Some((host, path)) => (host.to_string(), format!("/{path}")),
            None => (rest.to_string(), String::new()),
        };
        if endpoint_host.is_empty() {
            bail!("invalid endpoint address");
        }
        Ok(Self {
            scheme: scheme.to_ascii_lowercase(),
            endpoint_host,
            endpoint_path,
            region,
            bucket,
            access_key: settings.access_key.trim().to_string(),
            secret_key: settings.secret_key.trim().to_string(),
            path_style: settings.path_style,
            public: settings.public_bucket,
        })
    }

    fn host(&self) -> String {
        if self.path_style {
            self.endpoint_host.clone()
        } else {
            format!("{}.{}", self.bucket, self.endpoint_host)
        }
    }

    /// Whether requests go out unsigned: a public bucket the user gave no keys
    /// for. Keys win over the public flag, because writes always need a
    /// signature and a bucket can be public-read *and* key-protected.
    fn anonymous(&self) -> bool {
        self.public && (self.access_key.is_empty() || self.secret_key.is_empty())
    }

    fn key_path(&self, key: Option<&str>) -> String {
        let mut path = self.endpoint_path.clone();
        if self.path_style {
            path.push('/');
            path.push_str(&encode_segment(&self.bucket));
        }
        match key {
            Some(key) if !key.is_empty() => {
                path.push('/');
                path.push_str(&encode_key(key));
            }
            Some(_) => path.push('/'),
            None => {
                if path.is_empty() {
                    path.push('/');
                }
            }
        }
        path
    }
}

fn configured(provider: ProviderId) -> Result<S3Config> {
    let ProviderId::Custom(CustomProviderKind::S3, id) = provider else {
        bail!("not an S3 provider");
    };
    let saved = custom_providers::get(id).context("S3 provider is not configured")?;
    S3Config::from_custom(&saved)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaylistFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    tracks: Vec<Track>,
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&sha256(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + data.len());
    inner.extend(block.iter().map(|byte| byte ^ 0x36));
    inner.extend_from_slice(data);
    let inner = sha256(&inner);
    let mut outer = Vec::with_capacity(64 + inner.len());
    outer.extend(block.iter().map(|byte| byte ^ 0x5c));
    outer.extend_from_slice(&inner);
    sha256(&outer)
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let date_key = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let region_key = hmac_sha256(&date_key, region.as_bytes());
    let service_key = hmac_sha256(&region_key, service.as_bytes());
    hmac_sha256(&service_key, b"aws4_request")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = i64::try_from(year_of_era).unwrap_or(i64::MAX) + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn amz_datetime(secs: u64) -> (String, String) {
    let secs = i64::try_from(secs).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let stamp = format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    );
    (stamp.clone(), stamp[..8].to_string())
}

fn uri_encode(text: &str, keep_slash: bool) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (keep_slash && byte == b'/')
        {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn encode_segment(text: &str) -> String {
    uri_encode(text, false)
}

fn encode_key(key: &str) -> String {
    uri_encode(key, true)
}

fn canonical_query(params: &[(&str, String)]) -> String {
    let mut pairs: Vec<(String, String)> = params
        .iter()
        .map(|(name, value)| (uri_encode(name, false), uri_encode(value, false)))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn signed_request(
    config: &S3Config,
    method: &str,
    key: Option<&str>,
    query: &[(&str, String)],
    payload: Option<&[u8]>,
) -> (String, Vec<(String, String)>) {
    let host = config.host();
    let path = config.key_path(key);
    let query_string = canonical_query(query);
    let url = if query_string.is_empty() {
        format!("{}://{host}{path}", config.scheme)
    } else {
        format!("{}://{host}{path}?{query_string}", config.scheme)
    };
    if config.anonymous() {
        return (url, Vec::new());
    }

    let payload_hash = hex(&sha256(payload.unwrap_or(&[])));
    let (amz_date, date) = amz_datetime(now_secs());
    let scope = format_args!("{date}/{}/s3/aws4_request", config.region);
    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "{method}\n{path}\n{query_string}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex(&sha256(canonical_request.as_bytes()))
    );
    let key = signing_key(&config.secret_key, &date, &config.region, "s3");
    let signature = hex(&hmac_sha256(&key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        config.access_key
    );
    (
        url,
        vec![
            ("x-amz-content-sha256".into(), payload_hash),
            ("x-amz-date".into(), amz_date),
            ("Authorization".into(), authorization),
        ],
    )
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::config::Config::builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_mins(1)))
            .build()
            .new_agent()
    })
}

fn apply_headers<B>(
    builder: ureq::RequestBuilder<B>,
    headers: &[(String, String)],
) -> ureq::RequestBuilder<B> {
    let mut builder = builder;
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder
}

fn map_error(error: ureq::Error) -> anyhow::Error {
    let conflict = matches!(error, ureq::Error::StatusCode(412));
    let error = anyhow::Error::from(error);
    if conflict {
        error.context("playlist changed on the server since it was loaded")
    } else {
        error
    }
}

fn get_object(config: &S3Config, key: &str) -> Result<(Vec<u8>, String)> {
    let (url, headers) = signed_request(config, "GET", Some(key), &[], None);
    let response = apply_headers(agent().get(&url), &headers)
        .call()
        .map_err(map_error)
        .with_context(|| format!("failed to read {key}"))?;
    let etag = header(&response, "etag");
    let bytes = response
        .into_body()
        .read_to_vec()
        .context("failed to read object body")?;
    Ok((bytes, etag))
}

fn put_object(
    config: &S3Config,
    key: &str,
    body: &[u8],
    precondition: Option<(&str, &str)>,
) -> Result<String> {
    let (url, headers) = signed_request(config, "PUT", Some(key), &[], Some(body));
    let mut all = headers;
    all.push(("content-type".into(), "application/json".into()));
    if let Some((name, value)) = precondition {
        all.push((name.to_string(), value.to_string()));
    }
    let response = apply_headers(agent().put(&url), &all)
        .send(body)
        .map_err(map_error)
        .with_context(|| format!("failed to write {key}"))?;
    Ok(header(&response, "etag"))
}

fn delete_object(config: &S3Config, key: &str) -> Result<()> {
    let (url, headers) = signed_request(config, "DELETE", Some(key), &[], None);
    apply_headers(agent().delete(&url), &headers)
        .call()
        .map_err(map_error)
        .with_context(|| format!("failed to delete {key}"))?;
    Ok(())
}

fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> String {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObjectEntry {
    key: String,
    etag: String,
}

fn unescape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => {
                let code = if let Some(hex) = entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    entity.strip_prefix('#').and_then(|d| d.parse().ok())
                };
                code.and_then(char::from_u32)
            }
        };
        match decoded {
            Some(ch) => out.push(ch),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn tag_blocks(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut blocks = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        blocks.push(after[..end].to_string());
        rest = &after[end + close.len()..];
    }
    blocks
}

fn tag_text(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(unescape_xml(&block[start..end]))
}

fn list_objects(config: &S3Config, prefix: &str) -> Result<Vec<ObjectEntry>> {
    let mut objects = Vec::new();
    let mut token: Option<String> = None;
    for _ in 0..100 {
        let mut query: Vec<(&str, String)> =
            vec![("list-type", "2".into()), ("max-keys", "1000".into())];
        if !prefix.is_empty() {
            query.push(("prefix", prefix.to_string()));
        }
        if let Some(token) = &token {
            query.push(("continuation-token", token.clone()));
        }
        let (url, headers) = signed_request(config, "GET", None, &query, None);
        let response = apply_headers(agent().get(&url), &headers)
            .call()
            .map_err(map_error)
            .context("failed to list bucket")?;
        let body = response
            .into_body()
            .read_to_string()
            .context("failed to read bucket listing")?;
        for block in tag_blocks(&body, "Contents") {
            let Some(key) = tag_text(&block, "Key") else {
                continue;
            };
            objects.push(ObjectEntry {
                key,
                etag: tag_text(&block, "ETag").unwrap_or_default(),
            });
        }
        let truncated =
            tag_text(&body, "IsTruncated").is_some_and(|value| value.eq_ignore_ascii_case("true"));
        let next = tag_text(&body, "NextContinuationToken");
        if !truncated || next.is_none() {
            break;
        }
        token = next;
    }
    Ok(objects)
}

pub fn browse(
    provider: ProviderId,
    id: &str,
    kind: &str,
) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    let config = configured(provider)?;
    match kind {
        "playlist" => {
            let entry = load_entry(&config, provider, id)?;
            Ok((entry.tracks, None))
        }
        _ => Ok((Vec::new(), None)),
    }
}

fn playlist_object_key(id: &str) -> String {
    format!("{PLAYLISTS_PREFIX}{id}.json")
}

fn playlist_id_from_key(key: &str) -> Option<String> {
    let name = key.strip_prefix(PLAYLISTS_PREFIX)?;
    name.strip_suffix(".json").map(str::to_string)
}

fn playlist_body(entry: &S3PlaylistEntry) -> Result<Vec<u8>> {
    serde_json::to_vec(&PlaylistFile {
        version: PLAYLIST_VERSION,
        name: entry.name.clone(),
        tracks: entry.tracks.clone(),
    })
    .context("failed to serialize playlist")
}

fn entry_from_body(
    provider: ProviderId,
    id: &str,
    body: &[u8],
    etag: String,
) -> Result<S3PlaylistEntry> {
    let file: PlaylistFile = serde_json::from_slice(body).context("invalid playlist file")?;
    if file.version > PLAYLIST_VERSION {
        bail!(
            "playlist \"{id}\" was written by a newer version (format {})",
            file.version
        );
    }
    Ok(S3PlaylistEntry {
        provider,
        id: id.to_string(),
        name: if file.name.trim().is_empty() {
            id.to_string()
        } else {
            file.name
        },
        tracks: file.tracks,
        etag,
        fetched_at: now_secs(),
        dirty: false,
    })
}

fn load_from_bucket(config: &S3Config, provider: ProviderId, id: &str) -> Result<S3PlaylistEntry> {
    let (body, etag) = get_object(config, &playlist_object_key(id))?;
    let entry = entry_from_body(provider, id, &body, etag)?;
    s3_playlists::put(entry.clone());
    Ok(entry)
}

fn load_entry(config: &S3Config, provider: ProviderId, id: &str) -> Result<S3PlaylistEntry> {
    let cached = s3_playlists::get(provider, id);
    if let Some(entry) = &cached {
        if entry.dirty || now_secs().saturating_sub(entry.fetched_at) < PLAYLIST_TTL_SECS {
            return Ok(entry.clone());
        }
    }
    match load_from_bucket(config, provider, id) {
        Ok(entry) => Ok(entry),
        Err(error) => cached.with_context(|| format!("playlist {id}: {error:#}")),
    }
}

fn is_offline(error: &anyhow::Error) -> bool {
    if let Some(ureq::Error::StatusCode(_)) = error.downcast_ref::<ureq::Error>() {
        return false;
    }
    matches!(
        error.downcast_ref::<ureq::Error>(),
        Some(ureq::Error::Timeout(_) | ureq::Error::HostNotFound | ureq::Error::Io(_))
    )
}

fn is_conflict(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<ureq::Error>(),
        Some(ureq::Error::StatusCode(412))
    )
}

/// The precondition a write carries so it cannot overwrite a copy the edit
/// did not see.
#[derive(Clone, Copy)]
enum Precondition<'a> {
    /// Overwrite only the version the edit was based on; an empty etag means
    /// the object was never read, so the write is unconditional.
    IfMatch(&'a str),
    /// Create only when the key is still free.
    IfNoneMatch,
}

/// Write `entry` to the bucket and hand back the state the cache should hold:
/// the write's etag when it landed, the same entry marked dirty when the
/// bucket was unreachable. The caller persists it, so a batch of writes
/// (an offline flush) can be saved once.
fn commit(
    config: &S3Config,
    precondition: Precondition<'_>,
    mut entry: S3PlaylistEntry,
) -> Result<S3PlaylistEntry> {
    let body = playlist_body(&entry)?;
    let precondition = match precondition {
        Precondition::IfMatch(etag) if !etag.is_empty() => Some(("if-match", etag)),
        Precondition::IfMatch(_) => None,
        Precondition::IfNoneMatch => Some(("if-none-match", "*")),
    };
    match put_object(config, &playlist_object_key(&entry.id), &body, precondition) {
        Ok(etag) => {
            entry.etag = etag;
            entry.fetched_at = now_secs();
            entry.dirty = false;
            Ok(entry)
        }
        Err(error) if is_offline(&error) => {
            entry.dirty = true;
            Ok(entry)
        }
        Err(error) => Err(error),
    }
}

/// Write the edits that were made while the bucket was unreachable. The
/// `If-Match` still names the state the edit was based on, so an edit that
/// lost a race is dropped instead of overwriting the bucket's copy.
fn flush_pending(config: &S3Config, provider: ProviderId) {
    let mut flushed = Vec::new();
    for entry in s3_playlists::dirty(provider) {
        let id = entry.id.clone();
        let etag = entry.etag.clone();
        match commit(config, Precondition::IfMatch(&etag), entry) {
            Ok(entry) => flushed.push(entry),
            Err(error) if is_conflict(&error) => {
                s3_playlists::remove(provider, &id);
            }
            Err(_) => {}
        }
    }
    s3_playlists::put_all(flushed);
}

fn synthetic_entry(provider: ProviderId, id: &str) -> S3PlaylistEntry {
    S3PlaylistEntry {
        provider,
        id: id.to_string(),
        name: id.to_string(),
        tracks: Vec::new(),
        etag: String::new(),
        fetched_at: 0,
        dirty: false,
    }
}

pub fn list_playlists(provider: ProviderId) -> Result<Vec<ServerPlaylist>> {
    let config = configured(provider)?;
    flush_pending(&config, provider);
    let listing = list_objects(&config, PLAYLISTS_PREFIX)?;
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for object in listing.iter().filter(|object| {
        object
            .key
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("json"))
    }) {
        let Some(id) = playlist_id_from_key(&object.key) else {
            continue;
        };
        seen.insert(id.clone());
        let cached = s3_playlists::get(provider, &id);
        let entry = match &cached {
            Some(entry) if entry.dirty || entry.etag == object.etag => entry.clone(),
            _ => load_from_bucket(&config, provider, &id).unwrap_or_else(|_| {
                cached
                    .clone()
                    .unwrap_or_else(|| synthetic_entry(provider, &id))
            }),
        };
        rows.push(ServerPlaylist {
            id,
            name: entry.name,
            track_count: entry.tracks.len(),
            thumbnail: String::new(),
        });
    }
    for entry in s3_playlists::for_provider(provider) {
        if entry.dirty && !seen.contains(&entry.id) {
            rows.push(ServerPlaylist {
                id: entry.id,
                name: entry.name,
                track_count: entry.tracks.len(),
                thumbnail: String::new(),
            });
        }
    }
    s3_playlists::retain(provider, &seen);
    Ok(rows)
}

fn load_for_write(config: &S3Config, provider: ProviderId, id: &str) -> Result<S3PlaylistEntry> {
    flush_pending(config, provider);
    load_entry(config, provider, id)
}

fn new_playlist_id() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!(
        "{nanos:x}-{:x}-{:x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

pub fn create_playlist(
    provider: ProviderId,
    name: &str,
    tracks: &[Track],
) -> Result<ServerPlaylist> {
    let config = configured(provider)?;
    flush_pending(&config, provider);
    let name = if name.trim().is_empty() {
        "Playlist".to_string()
    } else {
        name.trim().to_string()
    };
    let entry = S3PlaylistEntry {
        provider,
        id: new_playlist_id(),
        name: name.clone(),
        tracks: tracks.to_vec(),
        etag: String::new(),
        fetched_at: 0,
        dirty: true,
    };
    let track_count = entry.tracks.len();
    let playlist_id = entry.id.clone();
    s3_playlists::put(commit(&config, Precondition::IfNoneMatch, entry)?);
    Ok(ServerPlaylist {
        id: playlist_id,
        name,
        track_count,
        thumbnail: String::new(),
    })
}

pub fn insert_into_playlist(
    provider: ProviderId,
    playlist_id: &str,
    index: usize,
    tracks: &[Track],
) -> Result<PlaylistWrite> {
    let config = configured(provider)?;
    let previous = load_for_write(&config, provider, playlist_id)?;
    let mut entry = previous.clone();
    let added = insert_missing(&mut entry, tracks, index);
    if added == 0 {
        return Ok(PlaylistWrite::default());
    }
    s3_playlists::put(commit(
        &config,
        Precondition::IfMatch(&previous.etag),
        entry,
    )?);
    Ok(PlaylistWrite { added, skipped: 0 })
}

fn insert_missing(entry: &mut S3PlaylistEntry, tracks: &[Track], index: usize) -> usize {
    let mut at = index.min(entry.tracks.len());
    let mut added = 0;
    for track in tracks {
        let key = track.cache_key();
        if entry
            .tracks
            .iter()
            .any(|existing| existing.cache_key() == key)
        {
            continue;
        }
        entry.tracks.insert(at, track.clone());
        at += 1;
        added += 1;
    }
    added
}

pub fn remove_from_playlist(
    provider: ProviderId,
    playlist_id: &str,
    positions: &[usize],
) -> Result<usize> {
    let config = configured(provider)?;
    let previous = load_for_write(&config, provider, playlist_id)?;
    let mut entry = previous.clone();
    let removed = crate::util::remove_at(&mut entry.tracks, positions);
    if removed == 0 {
        return Ok(0);
    }
    s3_playlists::put(commit(
        &config,
        Precondition::IfMatch(&previous.etag),
        entry,
    )?);
    Ok(removed)
}

pub fn reorder_playlist(provider: ProviderId, playlist_id: &str, tracks: &[Track]) -> Result<()> {
    let config = configured(provider)?;
    let previous = load_for_write(&config, provider, playlist_id)?;
    let mut entry = previous.clone();
    entry.tracks = tracks.to_vec();
    s3_playlists::put(commit(
        &config,
        Precondition::IfMatch(&previous.etag),
        entry,
    )?);
    Ok(())
}

pub fn rename_playlist(provider: ProviderId, playlist_id: &str, name: &str) -> Result<()> {
    let config = configured(provider)?;
    let previous = load_for_write(&config, provider, playlist_id)?;
    let mut entry = previous.clone();
    entry.name = name.trim().to_string();
    s3_playlists::put(commit(
        &config,
        Precondition::IfMatch(&previous.etag),
        entry,
    )?);
    Ok(())
}

pub fn delete_playlist(provider: ProviderId, playlist_id: &str) -> Result<()> {
    let config = configured(provider)?;
    delete_object(&config, &playlist_object_key(playlist_id))?;
    s3_playlists::remove(provider, playlist_id);
    Ok(())
}

/// Make the next read of `id` go back to the bucket, unless unsaved local
/// edits are waiting to be written.
pub fn invalidate_playlist(provider: ProviderId, id: &str) {
    if let Some(entry) = s3_playlists::get(provider, id) {
        if !entry.dirty {
            s3_playlists::set_fetched(provider, id, 0, entry.etag);
        }
    }
}

/// The tracks this playlist is cached with, without reaching the bucket.
pub fn snapshot_tracks(provider: ProviderId, id: &str) -> Vec<Track> {
    s3_playlists::get(provider, id)
        .map(|entry| entry.tracks)
        .unwrap_or_default()
}

/// Drop every cached playlist of a deleted bucket.
pub fn forget_playlists(provider: ProviderId) {
    s3_playlists::forget_provider(provider);
}

pub fn test_connection(provider: &CustomProvider) -> Result<String> {
    let config = S3Config::from_custom(provider)?;
    let query: Vec<(&str, String)> = vec![("list-type", "2".into()), ("max-keys", "1".into())];
    let (url, headers) = signed_request(&config, "GET", None, &query, None);
    apply_headers(agent().get(&url), &headers)
        .call()
        .map_err(map_error)?;
    Ok(format!(
        "Connected to bucket \"{}\" ({})",
        config.bucket, config.region
    ))
}

fn now_secs() -> u64 {
    s3_playlists::now_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::custom_providers::{CustomProviderConnection, S3Settings};

    fn config() -> S3Config {
        S3Config {
            scheme: "https".into(),
            endpoint_host: "s3.us-east-1.amazonaws.com".into(),
            endpoint_path: String::new(),
            region: "us-east-1".into(),
            bucket: "music".into(),
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            path_style: false,
            public: false,
        }
    }

    #[test]
    fn hmac_matches_known_vector() {
        assert_eq!(
            hex(&hmac_sha256(
                b"key",
                b"The quick brown fox jumps over the lazy dog"
            )),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ami_datetime_is_utc() {
        let (stamp, date) = amz_datetime(1_440_892_800);
        assert_eq!(stamp, "20150830T000000Z");
        assert_eq!(date, "20150830");
    }

    #[test]
    fn path_encoding_keeps_slashes_only_in_keys() {
        assert_eq!(encode_key("a b/c+d.mp3"), "a%20b/c%2Bd.mp3");
        assert_eq!(encode_segment("a/b"), "a%2Fb");
        assert_eq!(encode_key("plain-_.~"), "plain-_.~");
    }

    #[test]
    fn xml_entities_and_blocks_are_decoded() {
        let xml = "<Contents><Key>a &amp; b &#35;1.flac</Key><Size>4</Size></Contents>";
        let blocks = tag_blocks(xml, "Contents");
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            tag_text(&blocks[0], "Key").as_deref(),
            Some("a & b #1.flac")
        );
        assert_eq!(tag_text(&blocks[0], "Size").as_deref(), Some("4"));
        assert_eq!(unescape_xml("&lt;x&gt;"), "<x>");
    }

    #[test]
    fn virtual_and_path_style_urls() {
        let mut cfg = config();
        assert_eq!(cfg.host(), "music.s3.us-east-1.amazonaws.com");
        assert_eq!(cfg.key_path(Some("a b.mp3")), "/a%20b.mp3");

        cfg.path_style = true;
        assert_eq!(cfg.host(), "s3.us-east-1.amazonaws.com");
        assert_eq!(cfg.key_path(Some("a b.mp3")), "/music/a%20b.mp3");
    }

    #[test]
    fn public_urls_are_unsigned() {
        let mut cfg = config();
        cfg.public = true;
        cfg.access_key.clear();
        cfg.secret_key.clear();
        let (url, headers) = signed_request(&cfg, "GET", Some("x.mp3"), &[], None);
        assert!(headers.is_empty());
        assert!(url.starts_with("https://music.s3.us-east-1.amazonaws.com/"));
    }

    #[test]
    fn keys_sign_even_a_public_bucket() {
        let mut cfg = config();
        cfg.public = true;
        let (_, headers) = signed_request(&cfg, "PUT", Some("x.json"), &[], Some(b"{}"));
        assert!(headers.iter().any(|(name, _)| name == "Authorization"));
    }

    #[test]
    fn playlist_ids_are_derived_from_json_objects() {
        assert_eq!(
            playlist_id_from_key("playlists/abc.json").as_deref(),
            Some("abc")
        );
        assert_eq!(playlist_id_from_key("playlists/abc.txt"), None);
        assert_eq!(playlist_id_from_key("abc.json"), None);
        assert_eq!(playlist_object_key("abc"), "playlists/abc.json");
    }

    #[test]
    fn playlist_files_round_trip() {
        let entry = S3PlaylistEntry {
            provider: ProviderId::Custom(CustomProviderKind::S3, 1),
            id: "abc".into(),
            name: "Road trip".into(),
            tracks: vec![Track {
                title: "Song".into(),
                artists: vec!["Artist".into()],
                source: ProviderId::YouTube,
                providers: std::collections::HashMap::new(),
            }],
            etag: "etag".into(),
            fetched_at: 0,
            dirty: false,
        };
        let body = playlist_body(&entry).unwrap();
        let provider = ProviderId::Custom(CustomProviderKind::S3, 1);
        let restored = entry_from_body(provider, "abc", &body, "etag".into()).unwrap();
        assert_eq!(restored.name, "Road trip");
        assert_eq!(restored.tracks.len(), 1);
        assert_eq!(restored.tracks[0].title, "Song");
    }

    #[test]
    fn insert_missing_keeps_the_position_and_skips_known_tracks() {
        let provider = ProviderId::Custom(CustomProviderKind::S3, 1);
        let track = |title: &str| {
            Track::from_provider(
                ProviderId::YouTube,
                title.into(),
                format!("https://example.com/{title}"),
                title,
                "Artist",
                10,
                String::new(),
                None,
                None,
            )
        };
        let existing_a = track("a");
        let mut entry = S3PlaylistEntry {
            provider,
            id: "abc".into(),
            name: "Road trip".into(),
            tracks: vec![existing_a.clone(), track("b")],
            etag: String::new(),
            fetched_at: 0,
            dirty: false,
        };
        let added = insert_missing(&mut entry, &[track("x"), existing_a, track("y")], 1);
        assert_eq!(added, 2);
        let titles: Vec<&str> = entry
            .tracks
            .iter()
            .map(|track| track.title.as_str())
            .collect();
        assert_eq!(titles, ["a", "x", "y", "b"]);
    }

    #[test]
    fn custom_config_requires_a_bucket_and_defaults_the_endpoint() {
        let provider = CustomProvider {
            id: 1,
            name: "Bucket".into(),
            connection: CustomProviderConnection::S3(S3Settings {
                bucket: "music".into(),
                ..Default::default()
            }),
        };
        let cfg = S3Config::from_custom(&provider).unwrap();
        assert_eq!(cfg.region, "us-east-1");
        assert_eq!(cfg.endpoint_host, "s3.us-east-1.amazonaws.com");

        let empty = CustomProvider {
            id: 1,
            name: "Bucket".into(),
            connection: CustomProviderConnection::S3(S3Settings::default()),
        };
        assert!(S3Config::from_custom(&empty).is_err());
    }

    #[test]
    fn offline_errors_are_recognized() {
        let offline = anyhow::Error::from(ureq::Error::HostNotFound).context("failed to write key");
        assert!(is_offline(&offline));
        let denied = anyhow::Error::from(ureq::Error::StatusCode(403));
        assert!(!is_offline(&denied));
        let conflict = anyhow::Error::from(ureq::Error::StatusCode(412))
            .context("playlist changed on the server since it was loaded");
        assert!(is_conflict(&conflict));
        assert!(!is_conflict(&denied));
    }
}
