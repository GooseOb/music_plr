//! Shared `yt-dlp` invocation helpers used by the provider backends.

use super::run_command_with_timeout;
use anyhow::{Context, Result};
use std::process::Command;
use std::time::Duration;

/// yt-dlp audio downloads transcode to MP3 in real time and can legitimately
/// run for minutes, so they get a much larger budget than metadata calls.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(10);

/// Download `url` as an MP3 via `yt-dlp --extract-audio` into `output_path`,
/// appending `extra_args` (e.g. provider-specific `--extractor-args`). Returns
/// the final path with any `%(ext)s` template resolved.
pub(crate) fn download_audio(url: &str, output_path: &str, extra_args: &[&str]) -> Result<String> {
    let ext = "mp3";
    let mut cmd = Command::new("yt-dlp");
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
