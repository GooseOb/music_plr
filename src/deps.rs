//! Runtime dependency detection and self-installation.
//!
//! `goosemusic` shells out to external tools. `yt-dlp` (streaming, downloads,
//! search fallback) ships standalone per-OS binaries on its GitHub releases,
//! so it can be downloaded and cached by the app itself — no Python required.
//! `ytmusicapi` (nicer `YouTube` Music search) is an optional `Python` package
//! installed via `pip` when `Python 3` is present; without it the app falls back
//! to `yt-dlp` for search. `python3` itself is an OS prerequisite the app
//! cannot install, so it is surfaced in the dependency dialog as a manual step.
//!
//! The pinned `yt-dlp` version + SHA-256 (see [`YT_DLP_VERSION`] /
//! [`yt_dlp_expected_sha256`]) let the download be verified instead of blindly
//! executing whatever GitHub serves.

#![allow(clippy::unreadable_literal)]

use std::{fmt::Write, io::Read, path::PathBuf, process::Command, sync::Mutex, time::Duration};

use anyhow::{Context, Result};

/// Pinned `yt-dlp` release. Bump deliberately; the SHA-256 map below must be
/// updated to match the new release's `SHA2-256SUMS`.
pub const YT_DLP_VERSION: &str = "2026.08.19";

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
            DepKind::YtDlp | DepKind::YtMusicApi => true,
            DepKind::Python3 => false,
        }
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

#[allow(clippy::unnecessary_map_or)]
pub(crate) fn python3_present() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map_or(false, |o| o.status.success())
}

#[allow(clippy::unnecessary_map_or)]
fn ytmusicapi_present() -> bool {
    Command::new("python3")
        .args(["-c", "import ytmusicapi"])
        .output()
        .map_or(false, |o| o.status.success())
}

/// The cached download path for the pinned `yt-dlp` build (if present).
fn yt_dlp_cache_path() -> PathBuf {
    crate::data::cache_path("yt-dlp")
        .join(YT_DLP_VERSION)
        .join(yt_dlp_asset())
}

/// Return the `yt-dlp` executable to use, preferring (in order):
///
///     1. an explicit `GOOSEMUSIC_YT_DLP` override,
///     2. a previously downloaded + cached copy,
///     3. `yt-dlp` resolved via `PATH`.
/// `None` means `yt-dlp` is not available and must be installed.
#[allow(clippy::unnecessary_map_or)]
pub fn resolve_yt_dlp() -> Option<PathBuf> {
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
    if Command::new("yt-dlp")
        .arg("--version")
        .output()
        .map_or(false, |o| o.status.success())
    {
        return Some(PathBuf::from("yt-dlp"));
    }
    None
}

/// Build a `Command` pre-targeted at the resolved `yt-dlp`, or an error
/// directing the user to the dependency dialog.
pub fn yt_dlp_command() -> Result<Command> {
    let path = resolve_yt_dlp().context(
        "yt-dlp not found. Install it from the Dependencies dialog, or place yt-dlp on PATH.",
    )?;
    Ok(Command::new(path))
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

static AVAILABILITY: Mutex<DepAvailability> = Mutex::new(DepAvailability {
    yt_dlp: false,
    ytmusicapi: false,
    python3: false,
});

/// Current external-tool availability (drives per-provider capabilities).
pub fn availability() -> DepAvailability {
    *AVAILABILITY.lock().unwrap()
}

/// Replace the cached availability (called by [`detect_missing`]).
pub fn set_availability(a: DepAvailability) {
    *AVAILABILITY.lock().unwrap() = a;
}

/// Record that `yt-dlp` is now present (e.g. after a successful install).
pub fn set_yt_dlp_available() {
    let mut a = availability();
    a.yt_dlp = true;
    set_availability(a);
}

/// Record that `ytmusicapi` is now present (e.g. after a successful install).
pub fn set_ytmusicapi_available() {
    let mut a = availability();
    a.ytmusicapi = true;
    set_availability(a);
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
        DepKind::Python3 => anyhow::bail!("Python 3 must be installed manually (OS package)."),
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

fn install_yt_dlp(progress: impl Fn(u64, u64) + 'static) -> Result<()> {
    let asset = yt_dlp_asset();
    let url =
        format!("https://github.com/yt-dlp/yt-dlp/releases/download/{YT_DLP_VERSION}/{asset}");
    let resp = ureq::get(&url)
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
        .context("Failed to read yt-dlp download")?;

    let expected = yt_dlp_expected_sha256(asset);
    if expected.is_empty() {
        anyhow::bail!("No pinned SHA-256 for asset {asset}; cannot verify download.");
    }
    if sha256(&bytes) != expected {
        anyhow::bail!("yt-dlp checksum mismatch — download may be corrupted or tampered.");
    }

    let dir = crate::data::cache_path("yt-dlp").join(YT_DLP_VERSION);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    let path = dir.join(asset);
    let tmp = dir.join(format!("{asset}.part"));
    std::fs::write(&tmp, &bytes).context("Failed to write yt-dlp")?;
    #[cfg(unix)]
    std::fs::set_permissions(&tmp, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .context("Failed to mark yt-dlp executable")?;
    std::fs::rename(&tmp, &path).context("Failed to install yt-dlp")?;
    set_yt_dlp_available();
    Ok(())
}

fn install_ytmusicapi() -> Result<()> {
    let output = crate::providers::run_command_with_timeout(
        Command::new("python3").args(["-m", "pip", "install", "--user", "ytmusicapi"]),
        Duration::from_mins(5),
    )
    .context("Failed to run pip")?;
    if !output.status.success() {
        anyhow::bail!(
            "pip install ytmusicapi failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    set_ytmusicapi_available();
    Ok(())
}

/// Compact, dependency-free SHA-256 (used to verify the yt-dlp download).
#[allow(clippy::many_single_char_names)]
fn sha256(data: &[u8]) -> String {
    #[allow(clippy::unreadable_literal)]
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let big_s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(big_s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let big_s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = big_s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for x in h {
        let _ = write!(out, "{x:08x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::sha256;

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
