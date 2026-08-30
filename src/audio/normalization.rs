//! Loudness analysis for volume normalization.
//!
//! Decodes a complete audio file via symphonia (the same native decoder the
//! player uses) and derives a linear sample-scaling gain that brings the
//! track's perceived loudness (RMS) close to a target, clamped so it never
//! clips and never makes extreme changes.

use std::path::Path;

use symphonia::{
    core::{
        codecs::{DecoderOptions, CODEC_TYPE_NULL},
        formats::FormatOptions,
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    },
    default::get_probe,
};

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
    let mut probed = get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let track = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    let mut sum_squares: f64 = 0.0;
    let mut n_samples: u64 = 0;
    let mut peak: f32 = 0.0;

    while let Ok(packet) = probed.format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let Ok(audio_buf) = decoder.decode(&packet) else {
            // Recoverable decode error: skip the packet and keep going.
            continue;
        };
        let spec = audio_buf.spec().to_owned();
        let mut buf = symphonia::core::audio::SampleBuffer::<i16>::new(
            symphonia::core::units::Duration::from(audio_buf.capacity() as u64),
            spec,
        );
        buf.copy_interleaved_ref(audio_buf);
        for &s in buf.samples() {
            let v = f32::from(s) / 32768.0;
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
