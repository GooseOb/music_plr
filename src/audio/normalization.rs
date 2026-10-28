//! Loudness analysis for volume normalization.
//!
//! Decodes a complete audio file via symphonia (the same native decoder the
//! player uses) and derives a linear sample-scaling gain that brings the
//! track's perceived loudness (RMS) close to a target, clamped so it never
//! clips and never makes extreme changes.

use std::path::Path;

use symphonia::{
    core::{
        formats::{probe::Hint, FormatOptions, TrackType},
        io::MediaSourceStream,
        meta::MetadataOptions,
    },
    default::get_probe,
};

use super::decoder::make_audio_decoder;

/// Target RMS amplitude (linear, full scale = 1.0). Tracks quieter or louder
/// than this are scaled toward it, so playback loudness is consistent.
const TARGET_RMS: f32 = 0.2;
/// Lower bound on the gain so already-loud tracks are not boosted and quiet
/// ones are not left below audibility.
const MIN_GAIN: f32 = 0.5;
/// Upper bound on the gain so a single near-silent track cannot blow up.
const MAX_GAIN: f32 = 3.0;
/// Hard ceiling on post-gain peak to prevent clipping.
const MAX_PEAK: f32 = 0.99;

/// Compute a linear sample-scaling gain for `path` that evens out perceived
/// loudness. Returns `None` if the file can't be probed or decoded.
pub fn compute_normalization_gain(path: &Path) -> Option<f32> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(
        Box::new(file),
        symphonia::core::io::MediaSourceStreamOptions::default(),
    );
    let mut format = get_probe()
        .probe(
            &Hint::new(),
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;

    let track = format.default_track(TrackType::Audio)?;
    let params = track.codec_params.as_ref()?.audio()?;
    let track_id = track.id;
    let mut decoder = make_audio_decoder(params).ok()?;

    let mut sum_squares: f64 = 0.0;
    let mut n_samples: u64 = 0;
    let mut peak: f32 = 0.0;
    let mut samples: Vec<f32> = Vec::new();

    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }
        let Ok(audio_buf) = decoder.decode(&packet) else {
            // Recoverable decode error: skip the packet and keep going.
            continue;
        };
        audio_buf.copy_to_vec_interleaved(&mut samples);
        for &v in &samples {
            sum_squares += f64::from(v) * f64::from(v);
            n_samples += 1;
            let a = v.abs();
            if a > peak {
                peak = a;
            }
        }
    }

    if n_samples == 0 {
        return None;
    }
    let rms = (sum_squares / n_samples as f64).sqrt() as f32;
    if rms <= 0.0 {
        return None;
    }

    let mut gain = TARGET_RMS / rms;
    gain = gain.clamp(MIN_GAIN, MAX_GAIN);
    if peak * gain > MAX_PEAK {
        gain = MAX_PEAK / peak;
    }
    Some(gain)
}

pub fn load_gains() -> std::collections::HashMap<String, f32> {
    let path = crate::data::cache_path("normalization.json");
    let mut gains: std::collections::HashMap<String, f32> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut remapped = std::collections::HashMap::new();
    let mut migrated = false;
    for (old_key, gain) in gains.drain() {
        let new_key = canonical_gain_key(&old_key);
        migrated |= new_key != old_key;
        remapped.entry(new_key).or_insert(gain);
    }
    if migrated {
        save_gains(&remapped);
        return remapped;
    }
    remapped
}

/// Rewrite a pre-slug `Debug:id` gain key (e.g. `YouTube:…`, `LastFm:…`) to
/// the [`cache_key`](crate::providers::ProviderId::cache_key) format;
/// already-prefixed and bare keys pass through untouched.
fn canonical_gain_key(key: &str) -> String {
    let Some((prefix, id)) = key.split_once(':') else {
        return key.to_string();
    };
    match crate::providers::ProviderId::from_legacy_key(prefix) {
        Some(provider) => provider.cache_key(id),
        None => key.to_string(),
    }
}

pub fn save_gains(gains: &std::collections::HashMap<String, f32>) {
    let path = crate::data::cache_path("normalization.json");
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    if let Ok(s) = serde_json::to_string(gains) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_keys_rewrite_legacy_debug_prefixes() {
        assert_eq!(canonical_gain_key("YouTube:x"), "youtube:x");
        assert_eq!(canonical_gain_key("LastFm:x"), "lastfm:x");
        assert_eq!(canonical_gain_key("SoundCloud:x"), "soundcloud:x");
    }

    #[test]
    fn gain_keys_keep_slug_and_bare_keys() {
        assert_eq!(canonical_gain_key("youtube:x"), "youtube:x");
        assert_eq!(canonical_gain_key(" x "), " x ");
        assert_eq!(canonical_gain_key("no-colon"), "no-colon");
    }
}
