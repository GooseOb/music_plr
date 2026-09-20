//! Shared `yt-dlp` invocation helpers used by the provider backends.

use std::{
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};

use super::{run_command_with_timeout, ClientEvent};

/// yt-dlp audio downloads transcode to MP3 in real time and can legitimately
/// run for minutes, so they get a much larger budget than metadata calls.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(10);

/// `YouTube` player clients raced in parallel by [`pick_player_client`].
/// Covers the base client families plus the embedded fallback for
/// age-restricted videos; `web`-family clients need a PO token on some
/// networks while mobile/TV clients usually do not, so no single client
/// works everywhere.
pub(crate) const PLAYER_CLIENTS: &[&str] = &["ios", "android", "web", "tv", "mweb", "web_embedded"];

/// Format selector probed (and then streamed) for live playback: AAC-in-M4A
/// that symphonia decodes, falling back to any best audio, then to muxed
/// MP4/best. The muxed fallback matters for age-restricted tracks: without
/// an age-verified account `YouTube` serves only a 360p muxed MP4 whose AAC
/// audio symphonia still decodes (video track is skipped).
pub(crate) const STREAM_FORMAT: &str = "bestaudio[ext=m4a]/bestaudio/best[ext=mp4]/best";

/// Format selector probed before an `--extract-audio` download: any audio
/// works since yt-dlp re-encodes to MP3.
pub(crate) const DOWNLOAD_FORMAT: &str = "bestaudio/best";

/// Per-client probe budget; the overall race is bounded by
/// `PROBE_TIMEOUT + slack` in [`pick_player_client`].
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Whether `url` points at `YouTube` (only those honour `youtube:player_client`).
pub(crate) fn is_youtube_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("youtube.com") || lower.contains("youtu.be") || lower.contains("music.youtube")
}

/// Race the [`PLAYER_CLIENTS`] probes in parallel and return the first client
/// whose formats satisfy `format_selector` (e.g. [`STREAM_FORMAT`]).
/// Outstanding probes are killed as soon as a winner arrives. Returns `None`
/// when yt-dlp is missing, the URL is not a `YouTube` one, or no client
/// succeeded — callers then fall back to yt-dlp's own defaults.
pub(crate) fn pick_player_client(url: &str, format_selector: &str) -> Option<String> {
    if !is_youtube_url(url) {
        return None;
    }
    let yt_dlp = crate::deps::resolve_yt_dlp()?;
    let done = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for &client in PLAYER_CLIENTS {
        let tx = tx.clone();
        let done = done.clone();
        let path = yt_dlp.clone();
        let url = url.to_string();
        let format = format_selector.to_string();
        let client = client.to_string();
        std::thread::spawn(move || {
            if done.load(Ordering::SeqCst) {
                return;
            }
            if probe_client_has_format(&path, &url, &format, &client, &done) {
                done.store(true, Ordering::SeqCst);
                let _ = tx.send(client);
            }
        });
    }
    drop(tx);
    let winner = rx.recv_timeout(PROBE_TIMEOUT + Duration::from_secs(5)).ok();
    done.store(true, Ordering::SeqCst);
    winner
}

/// The most recent race winner, tried first next time so repeat
/// streams/downloads skip the full race when the client still works.
static CACHED_CLIENT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub(crate) fn cached_client() -> Option<String> {
    CACHED_CLIENT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|cached| cached.clone())
}

fn remember_client(client: &str) {
    if let Ok(mut cached) = CACHED_CLIENT.get_or_init(|| Mutex::new(None)).lock() {
        *cached = Some(client.to_string());
    }
}

pub(crate) fn forget_client() {
    if let Ok(mut cached) = CACHED_CLIENT.get_or_init(|| Mutex::new(None)).lock() {
        *cached = None;
    }
}

/// Whether yt-dlp's stderr looks like a client-dependent failure (missing
/// format, inaccessible video, sign-in/bot/PO-token walls) where another
/// player client may succeed — as opposed to network/tool errors where
/// retrying with a different client is pointless.
pub(crate) fn is_client_failure(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    [
        "requested format is not available",
        "video unavailable",
        "video is unavailable",
        "sign in to confirm",
        "not a bot",
        "po token",
        "login required",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Resolve the player client for `url`, emitting [`ClientEvent`]s for toasts.
///
/// Returns the remembered winner immediately (no yt-dlp call) when present
/// and only runs the full [`pick_player_client`] race when there is no cache.
/// The `bool` reports whether a race ran: callers start streaming/downloading
/// right away and, if that attempt fails with [`is_client_failure`], forget
/// the cache and call this again to race and retry. Returns `(None, false)`
/// (silently, no events) for non-`YouTube` URLs or when yt-dlp is missing —
/// callers then fall back to yt-dlp's defaults.
pub(crate) fn resolve_player_client(
    url: &str,
    format_selector: &str,
    emit: &dyn Fn(ClientEvent),
) -> (Option<String>, bool) {
    if !is_youtube_url(url) {
        return (None, false);
    }
    if let Some(cached) = cached_client() {
        return (Some(cached), false);
    }
    if crate::deps::resolve_yt_dlp().is_none() {
        return (None, false);
    }
    emit(ClientEvent::Resolving);
    let winner = pick_player_client(url, format_selector);
    match &winner {
        Some(client) => {
            remember_client(client);
            emit(ClientEvent::Resolved(client.clone()));
        }
        None => emit(ClientEvent::Unavailable),
    }
    (winner, true)
}

/// Check that `client` can resolve `format_selector` for `url` by asking
/// yt-dlp to print the matched format without downloading. Killed early when
/// `done` flips (another client already won) or `PROBE_TIMEOUT` elapses.
fn probe_client_has_format(
    yt_dlp: &std::path::Path,
    url: &str,
    format_selector: &str,
    client: &str,
    done: &AtomicBool,
) -> bool {
    let extractor_arg = format!("youtube:player_client={client}");
    let cookie_args = crate::deps::cookie_args();
    let mut args = vec![
        "--skip-download",
        "--no-warnings",
        "--no-playlist",
        "--socket-timeout",
        "10",
        "-f",
        format_selector,
        "--print",
        "format_id",
    ];
    args.extend(cookie_args.iter().map(String::as_str));
    args.extend(["--extractor-args", extractor_arg.as_str(), url]);
    let Ok(mut child) = std::process::Command::new(yt_dlp)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let started = Instant::now();
    loop {
        if done.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if started.elapsed() >= PROBE_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return false,
        }
    }
}

/// Download `url` as an MP3 via `yt-dlp --extract-audio` into `output_path`,
/// appending `extra_args` (e.g. provider-specific `--extractor-args`). Returns
/// the final path with any `%(ext)s` template resolved.
pub(crate) fn download_audio(url: &str, output_path: &str, extra_args: &[&str]) -> Result<String> {
    let ext = "mp3";
    let mut cmd = crate::deps::yt_dlp_command().context("Failed to download audio")?;
    cmd.args([
        "--extract-audio",
        "--audio-format",
        ext,
        "--audio-quality",
        "0",
        "--output",
        output_path,
        "--no-warnings",
    ])
    .args(extra_args)
    .arg(url);

    let output =
        run_command_with_timeout(&mut cmd, DOWNLOAD_TIMEOUT).context("Failed to download audio")?;

    if !output.status.success() {
        anyhow::bail!(
            "yt-dlp download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(output_path.replace("%(ext)s", ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn detects_youtube_urls() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc123"));
        assert!(is_youtube_url("https://youtu.be/abc123"));
        assert!(is_youtube_url("https://music.youtube.com/watch?v=abc123"));
        assert!(!is_youtube_url("https://soundcloud.com/artist/track"));
        assert!(!is_youtube_url("https://example.com/audio.mp3"));
    }

    #[test]
    fn skips_probe_for_non_youtube_urls() {
        assert!(pick_player_client("https://soundcloud.com/a/b", STREAM_FORMAT).is_none());
    }

    #[test]
    fn winner_cache_remembers_and_forgets() {
        let _guard = lock_tests();
        forget_client();
        assert!(cached_client().is_none());
        remember_client("ios");
        assert_eq!(cached_client().as_deref(), Some("ios"));
        forget_client();
        assert!(cached_client().is_none());
    }

    #[test]
    fn resolve_stays_silent_for_non_youtube_urls() {
        let _guard = lock_tests();
        forget_client();
        let events = std::cell::RefCell::new(Vec::new());
        let (winner, raced) =
            resolve_player_client("https://soundcloud.com/a/b", STREAM_FORMAT, &|e| {
                events.borrow_mut().push(format!("{e:?}"));
            });
        assert!(winner.is_none() && !raced);
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn detects_client_failures() {
        assert!(is_client_failure(
            "ERROR: [youtube] abc: Requested format is not available"
        ));
        assert!(is_client_failure("ERROR: This video is unavailable"));
        assert!(is_client_failure("Sign in to confirm you're not a bot"));
        assert!(is_client_failure("Failed to fetch PO Token for web client"));
        assert!(is_client_failure(
            "ERROR: [youtube] abc: Video is unavailable"
        ));
        assert!(!is_client_failure("ERROR: Service unavailable (HTTP 503)"));
        assert!(!is_client_failure(
            "ERROR: Unable to download webpage: HTTP Error 500"
        ));
        assert!(!is_client_failure(
            "ERROR: HTTP Error 500: Internal Server Error"
        ));
        assert!(!is_client_failure(""));
    }
}
