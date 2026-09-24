//! Automatic version checking and self-update.
//!
//! A background thread queries the latest stable version on crates.io,
//! compares it against the compiled-in [`APP_VERSION`], and — when a newer
//! release exists and the binary was not installed via a package manager —
//! downloads the matching platform asset from the GitHub release page
//! (`.../releases/download/v{version}/{asset}`), verifies its SHA-256
//! against the published `.sha256` sidecar, stages the replacement, and
//! spawns a detached updater that swaps in the new binary once this process
//! exits.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::Duration,
};

use serde::Deserialize;

use crate::app::{message::BackendResult, MusicPlayer};

/// Current app version (from `Cargo.toml` at compile time).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repository hosting the release binaries.
const GITHUB_REPO: &str = "GooseOb/music_plr";

/// Crate name on crates.io, the source of truth for the latest version.
const CRATES_IO_NAME: &str = "goosemusic";

/// Live status of the version-check / update pipeline, surfaced by the
/// Settings `Updates` section and the update-toast logic.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum UpdateStatus {
    /// No check has been performed yet this session.
    #[default]
    Unchecked,
    /// A version check is in flight.
    Checking,
    /// Up to date.
    UpToDate,
    /// A newer release is available for download.
    Available {
        version: String,
        release_url: String,
        asset_url: String,
    },
    /// An update is being downloaded / applied.
    /// `progress` is `(downloaded, total)` in bytes.
    Updating { progress: (u64, u64) },
    /// The update was downloaded and staged; the app is about to restart.
    UpdateApplied { version: String },
    /// Check or download failed.
    Error(String),
    /// Installed via a package manager — can't self-update.
    PackageManaged,
}

/// Outcome of a background version check, delivered as
/// [`BackendResult::VersionChecked`](crate::app::message::BackendResult).
#[derive(Debug, Clone)]
pub enum VersionCheckOutcome {
    /// Installed via a package manager — can't self-update.
    PackageManaged,
    /// No newer stable version on crates.io.
    UpToDate,
    /// A newer release is available for download.
    Available {
        version: String,
        release_url: String,
        asset_url: String,
    },
}

// ── package-manager detection ───────────────────────────────────────

/// Check whether the running binary can be replaced — i.e. whether we have
/// write access to the directory containing it. This is a direct permission
/// test instead of a path-based guess: it correctly catches package-managed
/// installs (read-only dirs), read-only filesystems, and any other scenario
/// where the user can't write the new binary.
pub fn can_self_update() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));

    // Probe by creating and removing a small temp file in the exe directory.
    let probe = dir.join(format!(".goosemusic_write_test_{}", std::process::id()));
    std::fs::write(&probe, []).is_ok() && std::fs::remove_file(&probe).is_ok()
}

// ── crates.io version lookup ──────────────────────────────────────────

/// Minimal crates.io response shape: only the latest stable version.
#[derive(Deserialize)]
struct CratesIoResponse {
    #[serde(rename = "crate")]
    crate_info: CratesIoCrate,
}

#[derive(Deserialize)]
struct CratesIoCrate {
    max_stable_version: String,
}

/// Release page URL for a bare version, e.g. `.../releases/tag/v1.2.3`.
fn release_url(version: &str) -> String {
    format!("https://github.com/{GITHUB_REPO}/releases/tag/v{version}")
}

/// Predictable download URL for this platform's asset in release `v{version}`,
/// served from the GitHub release page.
fn constructed_asset_url(version: &str) -> String {
    format!(
        "https://github.com/{GITHUB_REPO}/releases/download/v{version}/{}",
        asset_name()
    )
}

/// Query crates.io for the latest stable version.
fn fetch_latest_version() -> Result<String, String> {
    let url = format!("https://crates.io/api/v1/crates/{CRATES_IO_NAME}");
    let mut resp = agent()
        .get(&url)
        .header(
            "User-Agent",
            &format!("goosemusic/{APP_VERSION} (https://github.com/{GITHUB_REPO})"),
        )
        .call()
        .map_err(|e| format!("Version check request failed: {e}"))?;
    let parsed: CratesIoResponse = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("Failed to parse crates.io response: {e}"))?;
    let version = parsed.crate_info.max_stable_version.trim().to_string();
    if version.is_empty() {
        return Err("crates.io returned an empty version".to_string());
    }
    Ok(version)
}

/// Parse a `sha256sum` sidecar body (`<hex>  <filename>` or bare hex) into the
/// expected digest. Rejects anything that isn't 64 hex characters.
fn parse_sha256_sidecar(body: &str) -> Result<String, String> {
    let hex = body.split_whitespace().next().unwrap_or("").to_lowercase();
    if hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(hex)
    } else {
        Err("Release checksum file is invalid".to_string())
    }
}

/// Fetch the `.sha256` sidecar published next to the release asset. A `404`
/// means the release has no binary for this platform.
fn fetch_expected_sha256(asset_url: &str) -> Result<String, String> {
    let url = format!("{asset_url}.sha256");
    let mut resp = agent()
        .get(&url)
        .header("User-Agent", &format!("goosemusic/{APP_VERSION}"))
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => {
                format!(
                    "No sha256 checksum file found for this platform ({})",
                    asset_name()
                )
            }
            _ => format!("Checksum download failed: {e}"),
        })?;
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Checksum download failed: {e}"))?;
    parse_sha256_sidecar(&body)
}

/// Release asset filename for the current compilation target, e.g.
/// `goosemusic-x86_64-unknown-linux-gnu.tar.gz`.
fn asset_name() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    {
        "goosemusic-x86_64-unknown-linux-gnu.tar.gz"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64", target_env = "gnu"))]
    {
        "goosemusic-aarch64-unknown-linux-gnu.tar.gz"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "goosemusic-x86_64-pc-windows-msvc.zip"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "goosemusic-x86_64-apple-darwin.tar.gz"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "goosemusic-aarch64-apple-darwin.tar.gz"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"),
        all(target_os = "linux", target_arch = "aarch64", target_env = "gnu"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    {
        "goosemusic-unknown"
    }
}

/// Returns `true` when `latest` (e.g. `"1.0.2"`) is strictly greater than
/// `current` (e.g. `"1.0.1"`). The leading `v` on tags is stripped.
fn version_gt(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    parse(latest) > parse(current)
}

/// Shared `ureq` agent with timeouts so a dead GitHub endpoint can't hang a
/// background thread indefinitely.
fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::config::Config::builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .new_agent()
    })
}

// ── background operations ────────────────────────────────────────────

/// Spawn a detached thread that queries crates.io for the latest stable
/// version, compares it against [`APP_VERSION`], and reports the outcome
/// through `tx` as [`BackendResult::VersionChecked`]. The asset URL is
/// constructed from the version and the current platform.
pub fn spawn_version_check(tx: std::sync::mpsc::Sender<BackendResult>) {
    std::thread::spawn(move || {
        if !can_self_update() {
            let _ = tx.send(BackendResult::VersionChecked(Ok(
                VersionCheckOutcome::PackageManaged,
            )));
            return;
        }

        let result: Result<VersionCheckOutcome, String> = (|| {
            let latest = fetch_latest_version()?;

            if !version_gt(&latest, APP_VERSION) {
                return Ok(VersionCheckOutcome::UpToDate);
            }

            Ok(VersionCheckOutcome::Available {
                release_url: release_url(&latest),
                asset_url: constructed_asset_url(&latest),
                version: latest,
            })
        })();

        let _ = tx.send(BackendResult::VersionChecked(result));
    });
}

/// Spawn a detached thread that downloads the release asset, verifies it,
/// extracts the binary, stages it next to the current executable, and spawns
/// a detached updater helper. Progress is reported through `tx` as
/// [`BackendResult::UpdateProgress`] and completion as
/// [`BackendResult::UpdateComplete`].
pub fn spawn_update_download(
    tx: std::sync::mpsc::Sender<BackendResult>,
    asset_url: String,
    version: String,
) {
    std::thread::spawn(move || {
        let result: Result<String, String> = download_and_staged_apply(&asset_url, {
            let tx = tx.clone();
            move |downloaded, total| {
                let _ = tx.send(BackendResult::UpdateProgress(downloaded, total));
            }
        })
        .map(|()| version);
        let _ = tx.send(BackendResult::UpdateComplete(result));
    });
}

/// Recursively search `dir` for an executable file named `goosemusic` (or
/// `goosemusic.exe` on Windows). Returns the first match found.
fn find_binary_in_dir(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let target = if cfg!(windows) {
        "goosemusic.exe"
    } else {
        "goosemusic"
    };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name().is_some_and(|n| n == target) {
                return Some(path);
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    None
}

/// Fetch sidecar → download → verify SHA-256 → extract → stage → spawn
/// updater. On success the updater has been spawned and the app should exit.
fn download_and_staged_apply(
    url: &str,
    progress: impl Fn(u64, u64) + Send + 'static,
) -> std::result::Result<(), String> {
    // 0. Fetch the expected SHA-256 sidecar published next to the asset.
    let expected_sha256 = fetch_expected_sha256(url)?;

    // 1. Download the archive.
    let resp = agent()
        .get(url)
        .header("User-Agent", &format!("goosemusic/{APP_VERSION}"))
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => {
                format!("No binary asset for this platform ({})", asset_name())
            }
            _ => format!("Download request failed: {e}"),
        })?;

    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut body = resp.into_body();
    let mut reader = body.as_reader();
    let mut bytes = Vec::new();
    let mut downloaded: u64 = 0;
    let mut last_sent: u64 = 0;
    let step = (total / 50).max(8192);
    let mut buf = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Download read failed: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        downloaded += n as u64;
        if downloaded - last_sent >= step || downloaded >= total {
            last_sent = downloaded;
            progress(downloaded, total);
        }
    }

    // 2. Verify SHA-256.
    let actual = crate::deps::sha256(&bytes);
    if actual != expected_sha256 {
        return Err(format!(
            "Checksum mismatch — expected {expected_sha256}, got {actual}"
        ));
    }

    // 3. Extract the binary from the archive.
    let exe = std::env::current_exe().map_err(|e| format!("Cannot resolve exe path: {e}"))?;
    let exe_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));

    let temp_base = std::env::temp_dir().join(format!("goosemusic_update_{}", std::process::id()));
    std::fs::create_dir_all(&temp_base).map_err(|e| format!("Cannot create temp dir: {e}"))?;

    let archive_path = temp_base.join(asset_name());
    std::fs::write(&archive_path, &bytes).map_err(|e| format!("Cannot write archive: {e}"))?;

    let extract_dir = temp_base.join("extracted");
    std::fs::create_dir_all(&extract_dir).map_err(|e| format!("Cannot create extract dir: {e}"))?;

    let status = Command::new("tar")
        .args([
            "xf",
            &archive_path.to_string_lossy(),
            "-C",
            &extract_dir.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("Extraction failed: {e}"))?;
    if !status.success() {
        return Err("Extraction failed".to_string());
    }

    // Locate the binary inside the extracted directory.
    let binary_name = if cfg!(windows) {
        "goosemusic.exe"
    } else {
        "goosemusic"
    };
    let mut new_binary = extract_dir.join(binary_name);
    if !new_binary.exists() {
        new_binary = find_binary_in_dir(&extract_dir)
            .ok_or_else(|| "No binary found in archive".to_string())?;
    }
    if !new_binary.is_file() {
        return Err(format!(
            "Extracted path is not a regular file: {}",
            new_binary.display()
        ));
    }

    // 4. Copy to <exe_dir>/goosemusic.updating{.exe}.
    let staging_name = if cfg!(windows) {
        "goosemusic.updating.exe"
    } else {
        "goosemusic.updating"
    };
    let staged = exe_dir.join(staging_name);
    std::fs::copy(&new_binary, &staged).map_err(|e| format!("Cannot stage new binary: {e}"))?;

    // 5. Make it executable (Unix).
    #[cfg(unix)]
    {
        std::fs::set_permissions(&staged, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .map_err(|e| format!("Cannot set permissions: {e}"))?;
    }

    // 6. Write + spawn a detached updater that replaces the binary after exit.
    let pid = std::process::id();
    let old_str = exe.to_string_lossy().to_string();
    let new_str = staged.to_string_lossy().to_string();
    spawn_updater(pid, &old_str, &new_str).map_err(|e| format!("Cannot spawn updater: {e}"))?;

    // Clean up the temp dir (the staged binary is independent).
    let _ = std::fs::remove_dir_all(&temp_base);

    Ok(())
}

/// Write a tiny platform-specific updater script and spawn it detached.
/// The updater waits for the old process (pid) to exit, then moves the
/// staged binary over the real one and relaunches.
fn spawn_updater(pid: u32, old: &str, new: &str) -> std::result::Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        let script = include_str!("./updater.sh")
            .replace("{PID}", &pid.to_string())
            .replace("{NEW}", &format!("{new:?}"))
            .replace("{OLD}", &format!("{old:?}"));
        let helper = std::env::temp_dir().join(format!("goosemusic-updater-{pid}.sh"));
        std::fs::write(&helper, &script)?;
        std::fs::set_permissions(&helper, PermissionsExt::from_mode(0o755))?;
        Command::new(&helper)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
    }

    #[cfg(windows)]
    {
        let script = include_str!("./updater.bat")
            .replace("{PID}", &pid.to_string())
            .replace("{NEW}", new)
            .replace("{OLD}", old);
        let helper = std::env::temp_dir().join(format!("goosemusic-updater-{pid}.bat"));
        std::fs::write(&helper, &script)?;
        Command::new(&helper)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
    }

    Ok(())
}

/// Remove a stale `goosemusic.updating` file left by a previous failed/cancelled
/// update, so it doesn't interfere with the next launch.
pub fn cleanup_stale_update() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["goosemusic.updating", "goosemusic.updating.exe"] {
                let _ = std::fs::remove_file(dir.join(name));
            }
        }
    }
}

// ── MusicPlayer integration ────────────────────────────────────────

impl MusicPlayer {
    /// Begin a background version check. Idempotent while a check or update is
    /// already in flight; a `PackageManaged` status short-circuits.
    pub fn check_for_updates(&mut self) {
        if matches!(
            self.update_status,
            UpdateStatus::Checking | UpdateStatus::Updating { .. } | UpdateStatus::PackageManaged
        ) {
            return;
        }
        self.update_status = UpdateStatus::Checking;
        let tx = self.result_tx.clone();
        crate::app::update::spawn_version_check(tx);
    }

    /// Download, verify, and stage the available update, then signal the app
    /// to restart.
    pub fn start_update(&mut self) {
        let (asset_url, version) = match &self.update_status {
            UpdateStatus::Available {
                version, asset_url, ..
            } => (asset_url.clone(), version.clone()),
            _ => return,
        };
        self.update_status = UpdateStatus::Updating { progress: (0, 0) };
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            crate::app::update::spawn_update_download(tx, asset_url, version);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_gt_orders_bare_versions() {
        assert!(version_gt("1.3.6", "1.3.5"));
        assert!(version_gt("1.4.0", "1.3.9"));
        assert!(!version_gt("1.3.5", "1.3.5"));
        assert!(!version_gt("1.3.4", "1.3.5"));
    }

    #[test]
    fn crates_response_parses_max_stable_version() {
        let body = r#"{"crate": {"id": "goosemusic", "max_stable_version": "1.3.5"}}"#;
        let parsed: CratesIoResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.crate_info.max_stable_version, "1.3.5");
    }

    #[test]
    fn constructed_urls_follow_release_layout() {
        let asset_url = constructed_asset_url("1.3.6");
        assert_eq!(
            asset_url,
            format!(
                "https://github.com/GooseOb/music_plr/releases/download/v1.3.6/{}",
                asset_name()
            )
        );
        assert_eq!(
            release_url("1.3.6"),
            "https://github.com/GooseOb/music_plr/releases/tag/v1.3.6"
        );
    }

    #[test]
    fn sidecar_parses_sha256sum_format() {
        let hex = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(
            parse_sha256_sidecar(&format!("{hex}  goosemusic-x.tar.gz\n")).unwrap(),
            hex
        );
        assert_eq!(parse_sha256_sidecar(&format!("{hex}\n")).unwrap(), hex);
        assert_eq!(parse_sha256_sidecar(&hex.to_uppercase()).unwrap(), hex);
    }

    #[test]
    fn sidecar_rejects_garbage() {
        assert!(parse_sha256_sidecar("").is_err());
        assert!(parse_sha256_sidecar("not-a-checksum\n").is_err());
        assert!(parse_sha256_sidecar("abc123  file.tar.gz\n").is_err());
    }
}
