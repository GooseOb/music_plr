//! Runtime dependency detection and self-installation.
//!
//! `goosemusic` shells out to external tools. `yt-dlp` (streaming, downloads,
//! search fallback) ships standalone per-OS binaries on its GitHub releases,
//! so it can be downloaded and cached by the app itself. `ytmusicapi` (nicer
//! `YouTube` Music search) is an optional `Python` package installed via `pip`
//! when Python 3 is present; without it the app falls back to `yt-dlp` for
//! search. Python 3 itself can be auto-installed from the
//! `python-build-standalone` project (standalone, relocatable builds that
//! include pip), or found on the system PATH.
//!
//! The pinned versions + SHA-256 maps (see [`YT_DLP_VERSION`],
//! [`PYTHON_VERSION`], etc.) let downloads be verified instead of blindly
//! executing whatever the hosting service provides.

#![allow(clippy::unreadable_literal)]

use std::{
    io::Read,
    path::PathBuf,
    process::Command,
    sync::{OnceLock, RwLock},
    time::Duration,
};

use anyhow::{Context, Result};

/// Pinned `yt-dlp` release. Bump deliberately; the SHA-256 map below must be
/// updated to match the new release's `SHA2-256SUMS`.
pub const YT_DLP_VERSION: &str = "2026.08.19";

/// Pinned python-build-standalone release tag.
pub const PYTHON_PBS_RELEASE: &str = "20260807";

/// Pinned `CPython` version inside the above release.
pub const PYTHON_VERSION: &str = "3.13.15";

/// External tools the app may need. `Python3` is never auto-installed (it's an
/// OS package); the rest the app can fetch itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DepKind {
    YtDlp,
    YtMusicApi,
    Python3,
}

impl DepKind {
    pub fn name(self) -> &'static str {
        match self {
            DepKind::YtDlp => "yt-dlp",
            DepKind::YtMusicApi => "ytmusicapi",
            DepKind::Python3 => "Python 3",
        }
    }

    /// Whether the app can download/install this dependency itself.
    pub fn auto_installable(self) -> bool {
        match self {
            DepKind::YtDlp | DepKind::YtMusicApi | DepKind::Python3 => true,
        }
    }

    /// All dependency kinds, for iteration in the Settings / startup dialogs.
    pub fn all() -> &'static [DepKind] {
        &[DepKind::YtDlp, DepKind::YtMusicApi, DepKind::Python3]
    }
}

/// The `yt-dlp` release asset for the current target (standalone binary).
fn yt_dlp_asset() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "yt-dlp_linux_aarch64"
    }
    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    {
        "yt-dlp_linux"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "yt-dlp_arm64.exe"
    }
    #[cfg(all(target_os = "windows", not(target_arch = "aarch64")))]
    {
        "yt-dlp.exe"
    }
    #[cfg(target_os = "macos")]
    {
        "yt-dlp_macos"
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        "yt-dlp"
    }
}

/// Expected SHA-256 of [`yt_dlp_asset`] for [`YT_DLP_VERSION`], from the
/// release's `SHA2-256SUMS`.
fn yt_dlp_expected_sha256(asset: &str) -> &'static str {
    match asset {
        "yt-dlp_linux" => "58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a",
        "yt-dlp_linux_aarch64" => {
            "b16e4dab368a816cd05d477d698a605a6ae87ccee1c8ffd38fa21d7254141fcc"
        }
        "yt-dlp_macos" => "0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202",
        "yt-dlp.exe" => "66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a",
        "yt-dlp_arm64.exe" => "05b438997bafc3affdfda9d041353c9d73e04dc842207254b655b0887c4445b0",
        _ => "",
    }
}

/// The python-build-standalone `install_only_stripped` archive for the current
/// target platform. These are the smallest archives that include Python, pip,
/// and the standard library.
fn python_asset() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "cpython-3.13.15+20260807-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz"
    }
    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    {
        "cpython-3.13.15+20260807-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "cpython-3.13.15+20260807-aarch64-apple-darwin-install_only_stripped.tar.gz"
    }
    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        "cpython-3.13.15+20260807-x86_64-apple-darwin-install_only_stripped.tar.gz"
    }
    #[cfg(all(target_os = "windows", not(target_arch = "aarch64")))]
    {
        "cpython-3.13.15+20260807-x86_64-pc-windows-msvc-install_only_stripped.tar.gz"
    }
    #[cfg(not(any(
        all(target_os = "linux"),
        all(target_os = "macos"),
        all(target_os = "windows", not(target_arch = "aarch64"))
    )))]
    {
        ""
    }
}

/// Expected SHA-256 of [`python_asset`] for the pinned release.
fn python_expected_sha256(asset: &str) -> &'static str {
    match asset {
        "cpython-3.13.15+20260807-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz" => {
            "faae10a9faa9bec06da009ac69326cc1d9691dc138fec6a1b69159dff1781f35"
        }
        "cpython-3.13.15+20260807-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz" => {
            "1dfc9565c26f8892a33202b5966bdf9ff45c56a57b06e8fa65fecf05030afe5b"
        }
        "cpython-3.13.15+20260807-x86_64-apple-darwin-install_only_stripped.tar.gz" => {
            "187eed2282e9c3a5b6b14953d564ee25a9f35cf2c209c9fa292186ee48b0e4a1"
        }
        "cpython-3.13.15+20260807-aarch64-apple-darwin-install_only_stripped.tar.gz" => {
            "dbadb0ffe46f8bace50daaf8a0c5fc6903c003690776da9eb5269e33c856bb53"
        }
        "cpython-3.13.15+20260807-x86_64-pc-windows-msvc-install_only_stripped.tar.gz" => {
            "44bf9ae71f4b45e3ba3104ae331c6eff3f7002593c26fd12453eb9310c4f259a"
        }
        _ => "",
    }
}

/// The cache directory for the standalone Python installation.
fn python_cache_path() -> PathBuf {
    crate::data::cache_path("python").join(PYTHON_VERSION)
}

fn probe_python(exe: &str) -> Option<PathBuf> {
    Command::new(exe)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| PathBuf::from(exe))
}

/// Resolve the Python 3 interpreter to invoke. Resolution order:
///   1. `GOOSEMUSIC_PYTHON` env var override
///   2. Previously downloaded + cached standalone copy
///   3. `python3` / `python` resolved via PATH
///
/// Returns `None` when no Python 3 is available.
pub(crate) fn python_exe() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            if let Ok(p) = std::env::var("GOOSEMUSIC_PYTHON") {
                let path = PathBuf::from(&p);
                if path.exists() {
                    return Some(path);
                }
            }
            let cached = python_cache_path();
            let bin_dir = cached.join("python").join("bin");
            let python_bin = {
                #[cfg(target_os = "windows")]
                {
                    cached.join("python").join("python.exe")
                }
                #[cfg(not(target_os = "windows"))]
                {
                    bin_dir.join("python3")
                }
            };
            if python_bin.exists() {
                return Some(python_bin);
            }
            ["python3", "python"].into_iter().find_map(probe_python)
        })
        .clone()
}

/// Resolve a system Python (not the managed copy). Used as a fallback when the
/// managed Python lacks a needed package but the system Python has it.
pub(crate) fn system_python_exe() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| ["python3", "python"].into_iter().find_map(probe_python))
        .clone()
}

pub(crate) fn python3_present() -> bool {
    python_exe().is_some()
}

/// Check if a specific Python interpreter has ytmusicapi installed.
fn has_ytmusicapi(py: &PathBuf) -> bool {
    Command::new(py)
        .args(["-c", "import ytmusicapi"])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn ytmusicapi_present() -> bool {
    // Check the resolved Python (managed or system) first.
    if python_exe().is_some_and(|py| has_ytmusicapi(&py)) {
        return true;
    }
    // Fall back: check system Python3/Python when the managed copy lacks it.
    ["python3", "python"].into_iter().any(|exe| {
        Command::new(exe)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
            && {
                let path = PathBuf::from(exe);
                has_ytmusicapi(&path)
            }
    })
}

/// The cached download path for the pinned `yt-dlp` build (if present).
fn yt_dlp_cache_path() -> PathBuf {
    crate::data::cache_path("yt-dlp")
        .join(YT_DLP_VERSION)
        .join(yt_dlp_asset())
}

/// Marker file written when the app installs `ytmusicapi` via `pip`, so the
/// Settings view can tell an app-managed install from a system-provided one.
fn yt_music_api_marker() -> PathBuf {
    crate::data::cache_path("ytmusicapi")
}

static YT_DLP_PATH_PROBE: OnceLock<bool> = OnceLock::new();

/// Return the `yt-dlp` executable to use, preferring (in order):
///
///     1. an explicit `GOOSEMUSIC_YT_DLP` override,
///     2. a previously downloaded + cached copy,
///     3. `yt-dlp` resolved via `PATH`.
/// `None` means `yt-dlp` is not available and must be installed.
#[allow(clippy::unnecessary_map_or)]
pub fn resolve_yt_dlp() -> Option<PathBuf> {
    // Env override and cached copy are cheap; PATH probe is cached because it
    // spawns `--version` on every search/stream/download otherwise.
    if let Ok(p) = std::env::var("GOOSEMUSIC_YT_DLP") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let cached = yt_dlp_cache_path();
    if cached.exists() {
        return Some(cached);
    }
    let available = *YT_DLP_PATH_PROBE.get_or_init(|| {
        Command::new("yt-dlp")
            .arg("--version")
            .output()
            .map_or(false, |o| o.status.success())
    });
    available.then(|| PathBuf::from("yt-dlp"))
}

/// Build a `Command` pre-targeted at the resolved `yt-dlp`, or an error
/// directing the user to the dependency dialog. Includes the configured
/// `--cookies-from-browser` args (if any) so every `yt-dlp` call shares them.
pub fn yt_dlp_command() -> Result<Command> {
    let path = resolve_yt_dlp().context(
        "yt-dlp not found. Install it from the Dependencies dialog, or place yt-dlp on PATH.",
    )?;
    let mut cmd = Command::new(path);
    cmd.args(cookie_args());
    Ok(cmd)
}

/// Browsers `yt-dlp --cookies-from-browser` can read cookies from. Offered in
/// the Settings picker; stored lowercase in [`crate::data::config::Config`].
pub const COOKIE_BROWSERS: &[&str] = &[
    "brave", "chrome", "chromium", "edge", "firefox", "opera", "safari", "vivaldi",
];

static COOKIE_BROWSER: OnceLock<std::sync::Mutex<Option<String>>> = OnceLock::new();

/// Record which browser `yt-dlp` should read cookies from (`None` disables).
/// Unknown names are rejected defensively. Synced from the config at startup
/// and on every settings change; background threads read it via
/// [`cookie_args`] so no plumbing is needed at each call site.
pub fn set_cookie_browser(browser: Option<String>) {
    let normalized = browser.and_then(|b| {
        let lower = b.to_ascii_lowercase();
        COOKIE_BROWSERS.contains(&lower.as_str()).then_some(lower)
    });
    if let Ok(mut guard) = COOKIE_BROWSER
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
    {
        *guard = normalized;
    }
}

/// Extra `yt-dlp` args for the configured cookie browser, or empty when
/// disabled. Used by call sites that build their `Command` manually (stream,
/// probes, batch metadata); [`yt_dlp_command`] already includes them.
pub(crate) fn cookie_args() -> Vec<String> {
    COOKIE_BROWSER
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .map(|b| vec!["--cookies-from-browser".to_string(), b])
        .unwrap_or_default()
}

/// Detect which dependencies are missing, returning them for the startup
/// dialog. Cheap: a couple of short `--version`/`import` probes.
/// Runtime availability of the external tools, cached from the last detection
/// (or updated as the user installs them from the startup dialog). The OS
/// environment is process-global, so this is a global cache read by
/// [`crate::providers::ProviderId::capabilities`] to decide whether a source is
/// searchable / streamable / downloadable right now.
#[derive(Debug, Clone, Copy, Default)]
pub struct DepAvailability {
    pub yt_dlp: bool,
    pub ytmusicapi: bool,
    pub python3: bool,
}

static AVAILABILITY: RwLock<DepAvailability> = RwLock::new(DepAvailability {
    yt_dlp: false,
    ytmusicapi: false,
    python3: false,
});

/// Current external-tool availability (drives per-provider capabilities).
pub fn availability() -> DepAvailability {
    *AVAILABILITY
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Replace the cached availability (called by [`detect_missing`]).
pub fn set_availability(a: DepAvailability) {
    if let Ok(mut slot) = AVAILABILITY.write() {
        *slot = a;
    }
}

/// Record that `kind` is now present (e.g. after a successful install), updating
/// the cached availability that drives per-provider capabilities.
pub fn set_available(kind: DepKind) {
    let mut a = availability();
    match kind {
        DepKind::YtDlp => a.yt_dlp = true,
        DepKind::YtMusicApi => a.ytmusicapi = true,
        DepKind::Python3 => a.python3 = true,
    }
    set_availability(a);
}

/// Whether `kind` is currently available (on PATH / importable), regardless of
/// whether the app manages its own copy.
pub fn is_available(kind: DepKind) -> bool {
    let a = availability();
    match kind {
        DepKind::YtDlp => a.yt_dlp,
        DepKind::YtMusicApi => a.ytmusicapi,
        DepKind::Python3 => a.python3,
    }
}

/// Whether the app has installed its own managed copy of `kind` (as opposed to
/// relying on a system-provided one). For `yt-dlp` this is the cached binary;
/// for `ytmusicapi` it's the app's `pip install` marker file. The app can only
/// remove deps it manages itself, so this doubles as the uninstall guard.
pub fn installed_via_app(kind: DepKind) -> bool {
    match kind {
        DepKind::YtDlp => yt_dlp_cache_path().exists(),
        DepKind::YtMusicApi => yt_music_api_marker().exists(),
        DepKind::Python3 => python_cache_path().exists(),
    }
}

/// Remove the app-managed copy of `kind` (falls back to any system-provided
/// one). Returns an error for kinds the app cannot uninstall.
pub fn uninstall(kind: DepKind) -> Result<()> {
    match kind {
        DepKind::YtDlp => {
            let dir = crate::data::cache_path("yt-dlp");
            if dir.exists() {
                std::fs::remove_dir_all(&dir)
                    .with_context(|| format!("Failed to remove {}", dir.display()))?;
            }
            let mut a = availability();
            a.yt_dlp = resolve_yt_dlp().is_some();
            set_availability(a);
            Ok(())
        }
        DepKind::YtMusicApi => {
            let py = python_exe().ok_or_else(|| {
                anyhow::anyhow!("Python 3 not found; install it to manage ytmusicapi.")
            })?;
            let output = crate::providers::run_command_with_timeout(
                Command::new(&py).args(["-m", "pip", "uninstall", "-y", "ytmusicapi"]),
                Duration::from_mins(5),
            )
            .context("Failed to run pip uninstall")?;
            let python3 = python3_present();
            let still_present = python3 && ytmusicapi_present();
            let mut a = availability();
            a.python3 = python3;
            a.ytmusicapi = still_present;
            set_availability(a);
            // Clear the app-installed marker regardless of the pip outcome.
            let _ = std::fs::remove_file(yt_music_api_marker());
            if !output.status.success() && still_present {
                anyhow::bail!(
                    "pip uninstall ytmusicapi failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Ok(())
        }
        DepKind::Python3 => {
            let dir = python_cache_path();
            if dir.exists() {
                std::fs::remove_dir_all(&dir)
                    .with_context(|| format!("Failed to remove {}", dir.display()))?;
            }
            let mut a = availability();
            a.python3 = python3_present();
            set_availability(a);
            Ok(())
        }
    }
}

pub fn detect_missing() -> Vec<DepKind> {
    let yt_dlp = resolve_yt_dlp().is_some();
    let python3 = python3_present();
    let ytmusicapi = python3 && ytmusicapi_present();
    set_availability(DepAvailability {
        yt_dlp,
        ytmusicapi,
        python3,
    });

    let mut missing = Vec::new();
    if !yt_dlp {
        missing.push(DepKind::YtDlp);
    }
    if !python3 {
        missing.push(DepKind::Python3);
    }
    if !ytmusicapi {
        missing.push(DepKind::YtMusicApi);
    }
    missing
}

/// Install a single dependency. Auto-installable deps do the work here; calling
/// this with `Python3` returns an error (the dialog disables that row).
pub fn install(kind: DepKind, progress: impl Fn(u64, u64) + 'static) -> Result<()> {
    match kind {
        DepKind::YtDlp => install_yt_dlp(progress),
        DepKind::YtMusicApi => install_ytmusicapi(),
        DepKind::Python3 => install_python(progress),
    }
}

/// A `Read` adapter that reports download progress (bytes fetched / total)
/// through `cb`, throttled to ~2% steps so the UI isn't flooded with updates.
struct ProgressReader<R> {
    inner: R,
    downloaded: u64,
    total: u64,
    last_sent: u64,
    cb: Box<dyn Fn(u64, u64)>,
}

impl<R: std::io::Read> std::io::Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.downloaded += n as u64;
            let step = if self.total == 0 {
                1 << 16
            } else {
                (self.total / 50).max(1)
            };
            if self.downloaded - self.last_sent >= step || self.downloaded >= self.total {
                self.last_sent = self.downloaded;
                (self.cb)(self.downloaded, self.total);
            }
        }
        Ok(n)
    }
}

fn download_verified(
    url: &str,
    expected_sha256: &str,
    what: &str,
    progress: impl Fn(u64, u64) + 'static,
) -> Result<Vec<u8>> {
    if expected_sha256.is_empty() {
        anyhow::bail!("No pinned SHA-256 for {what}; cannot verify download.");
    }
    let resp = ureq::get(url)
        .call()
        .with_context(|| format!("Failed to download {url}"))?;
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let mut body = resp.into_body();
    let reader = body.as_reader();
    let mut reader = ProgressReader {
        inner: reader,
        downloaded: 0,
        total,
        last_sent: 0,
        cb: Box::new(progress),
    };
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .with_context(|| format!("Failed to read {what} download"))?;
    if sha256(&bytes) != expected_sha256 {
        anyhow::bail!("{what} checksum mismatch — download may be corrupted or tampered.");
    }
    Ok(bytes)
}

fn install_yt_dlp(progress: impl Fn(u64, u64) + 'static) -> Result<()> {
    let asset = yt_dlp_asset();
    let url =
        format!("https://github.com/yt-dlp/yt-dlp/releases/download/{YT_DLP_VERSION}/{asset}");
    let bytes = download_verified(&url, yt_dlp_expected_sha256(asset), "yt-dlp", progress)?;

    let dir = crate::data::cache_path("yt-dlp").join(YT_DLP_VERSION);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    let path = dir.join(asset);
    let tmp = dir.join(format!("{asset}.part"));
    std::fs::write(&tmp, &bytes).context("Failed to write yt-dlp")?;
    #[cfg(unix)]
    std::fs::set_permissions(&tmp, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .context("Failed to mark yt-dlp executable")?;
    std::fs::rename(&tmp, &path).context("Failed to install yt-dlp")?;
    set_available(DepKind::YtDlp);
    Ok(())
}

fn install_python(progress: impl Fn(u64, u64) + 'static) -> Result<()> {
    let asset = python_asset();
    if asset.is_empty() {
        anyhow::bail!("No standalone Python build available for this platform.");
    }
    let url = format!(
        "https://github.com/astral-sh/python-build-standalone/releases/download/{PYTHON_PBS_RELEASE}/{asset}"
    );
    let bytes = download_verified(&url, python_expected_sha256(asset), "Python", progress)?;

    let dir = python_cache_path();
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    let cursor = std::io::Cursor::new(bytes);
    let gz = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);
    archive
        .unpack(&dir)
        .context("Failed to extract Python archive")?;

    let python_bin = {
        #[cfg(target_os = "windows")]
        {
            dir.join("python").join("python.exe")
        }
        #[cfg(not(target_os = "windows"))]
        {
            dir.join("python").join("bin").join("python3")
        }
    };
    if !python_bin.exists() {
        anyhow::bail!(
            "Python archive extracted but binary not found at {}",
            python_bin.display()
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin_dir = dir.join("python").join("bin");
        for entry in std::fs::read_dir(&bin_dir)
            .with_context(|| format!("Failed to read {}", bin_dir.display()))?
        {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                std::fs::set_permissions(entry.path(), PermissionsExt::from_mode(0o755))?;
            }
        }
    }

    set_available(DepKind::Python3);
    Ok(())
}

fn install_ytmusicapi() -> Result<()> {
    let py = python_exe()
        .ok_or_else(|| anyhow::anyhow!("Python 3 not found; install it to use pip."))?;
    let output = crate::providers::run_command_with_timeout(
        Command::new(&py).args(["-m", "pip", "install", "ytmusicapi"]),
        Duration::from_mins(5),
    )
    .context("Failed to run pip")?;
    if !output.status.success() {
        anyhow::bail!(
            "pip install ytmusicapi failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    set_available(DepKind::YtMusicApi);
    let marker = yt_music_api_marker();
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, b"");
    Ok(())
}

/// SHA-256 hex digest (used to verify downloads).
pub(crate) fn sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::{cookie_args, set_cookie_browser, sha256};

    #[test]
    fn cookie_browser_normalizes_and_rejects_unknown() {
        set_cookie_browser(Some("Firefox".into()));
        assert_eq!(
            cookie_args(),
            vec!["--cookies-from-browser".to_string(), "firefox".to_string()]
        );
        set_cookie_browser(Some("netscape".into()));
        assert!(cookie_args().is_empty());
        set_cookie_browser(None);
        assert!(cookie_args().is_empty());
    }

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }
}
