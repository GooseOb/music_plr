//! Automatic version checking and self-update via the GitHub releases API.
//!
//! A background thread queries the latest release tag, compares it against the
//! compiled-in [`APP_VERSION`], and — when a newer release exists and the binary
//! was not installed via a package manager — downloads the matching platform
//! asset, verifies its SHA-256, stages the replacement, and spawns a detached
//! updater that swaps in the new binary once this process exits.

use std::{
    io::Read,
    process::{Command, Stdio},
    time::Duration,
};

use serde::Deserialize;

use crate::app::{message::BackendResult, MusicPlayer};

/// Current app version (from `Cargo.toml` at compile time).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repository for release lookups.
const GITHUB_REPO: &str = "GooseOb/music_plr";

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
        sha256: String,
    },
    /// An update is being downloaded / applied.
    /// `progress` is `(downloaded, total)` in bytes.
    Updating { progress: (u64, u64) },
    /// The update was downloaded and staged; the app is about to restart.
    UpdateApplied,
    /// Check or download failed.
    Error(String),
    /// Installed via a package manager — can't self-update.
    PackageManaged,
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

// ── GitHub API structs ──────────────────────────────────────────────

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: String,
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

struct UpdateInfo {
    version: String,
    release_url: String,
    asset_url: String,
    sha256: String,
}

/// Spawn a detached thread that queries the GitHub releases API for the latest
/// tag, compares it against [`APP_VERSION`], and reports the outcome through
/// `tx` as [`BackendResult::VersionChecked`].
pub fn spawn_version_check(tx: std::sync::mpsc::Sender<BackendResult>) {
    std::thread::spawn(move || {
        let pkg_managed = !can_self_update();
        if pkg_managed {
            let _ = tx.send(BackendResult::VersionChecked {
                current: APP_VERSION.to_string(),
                latest: None,
                release_url: String::new(),
                asset_url: None,
                sha256: None,
                package_managed: true,
                error: None,
            });
            return;
        }

        let result: Result<Option<UpdateInfo>, String> = (|| {
            let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
            let resp = agent()
                .get(&url)
                .header("User-Agent", &format!("goosemusic/{APP_VERSION}"))
                .header("Accept", "application/vnd.github.v3+json")
                .call();

            let mut resp = match resp {
                Ok(r) => r,
                Err(ureq::Error::StatusCode(code)) => {
                    return Err(format!("GitHub API returned HTTP {code}"));
                }
                Err(e) => {
                    return Err(format!("GitHub API request failed: {e}"));
                }
            };

            let release: GitHubRelease = resp
                .body_mut()
                .read_json()
                .map_err(|e| format!("Failed to parse GitHub API response: {e}"))?;

            let tag = release.tag_name.trim_start_matches('v');

            if !version_gt(tag, APP_VERSION) {
                return Ok(None); // up to date
            }

            let name = asset_name();
            let asset = release
                .assets
                .iter()
                .find(|a| a.name == name)
                .ok_or_else(|| format!("No binary asset for this platform ({name})"))?;

            let sha = asset.digest.trim_start_matches("sha256:").to_string();
            if sha.is_empty() {
                return Err("Release asset has no checksum".to_string());
            }

            Ok(Some(UpdateInfo {
                version: tag.to_string(),
                release_url: release.html_url,
                asset_url: asset.browser_download_url.clone(),
                sha256: sha,
            }))
        })();

        let _ = tx.send(match result {
            Ok(Some(info)) => BackendResult::VersionChecked {
                current: APP_VERSION.to_string(),
                latest: Some(info.version),
                release_url: info.release_url,
                asset_url: Some(info.asset_url),
                sha256: Some(info.sha256),
                package_managed: false,
                error: None,
            },
            Ok(None) => BackendResult::VersionChecked {
                current: APP_VERSION.to_string(),
                latest: None,
                release_url: String::new(),
                asset_url: None,
                sha256: None,
                package_managed: false,
                error: None,
            },
            Err(e) => BackendResult::VersionChecked {
                current: APP_VERSION.to_string(),
                latest: None,
                release_url: String::new(),
                asset_url: None,
                sha256: None,
                package_managed: false,
                error: Some(e),
            },
        });
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
    expected_sha256: String,
    version: String,
) {
    std::thread::spawn(move || {
        let result: Result<String, String> =
            download_and_staged_apply(&asset_url, &expected_sha256, {
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

/// Download → verify SHA-256 → extract → stage → spawn updater.
/// On success the updater has been spawned and the app should exit.
fn download_and_staged_apply(
    url: &str,
    expected_sha256: &str,
    progress: impl Fn(u64, u64) + Send + 'static,
) -> std::result::Result<(), String> {
    // 1. Download the archive.
    let resp = agent()
        .get(url)
        .header("User-Agent", &format!("goosemusic/{APP_VERSION}"))
        .call()
        .map_err(|e| format!("Download request failed: {e}"))?;

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
        use std::os::unix::fs::PermissionsExt;
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
        let (asset_url, sha256, version) = match &self.update_status {
            UpdateStatus::Available {
                version,
                asset_url,
                sha256,
                ..
            } => (asset_url.clone(), sha256.clone(), version.clone()),
            _ => return,
        };
        self.update_status = UpdateStatus::Updating { progress: (0, 0) };
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            crate::app::update::spawn_update_download(tx, asset_url, sha256, version);
        });
    }
}
