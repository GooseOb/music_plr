use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use symphonia::core::{
    audio::{AudioSpec, GenericAudioBufferRef},
    codecs::audio::AudioDecoder,
    errors::Error,
    formats::{probe::Hint, FormatOptions, FormatReader, SeekMode, SeekTo, TrackType},
    io::MediaSourceStream,
    meta::MetadataOptions,
    packet::Packet,
    units::{Duration, Time, TimeBase},
};

use super::{decoder::make_audio_decoder, growing::GrowingMediaSource};

/// How many packet-read failures to skip while looking for the next audio
/// packet before treating the stream as exhausted. Muxed MP4 containers (the
/// `best[ext=mp4]` fallback for tracks with no audio-only format) interleave
/// video packets whose sample offsets require seeks, so symphonia's reader
/// reports transient `out-of-bounds for a non-seekable stream` errors
/// mid-track; giving up on the first one truncated playback after a few
/// seconds.
const MAX_PACKET_ERRORS: usize = 200;

/// A rodio `Source` that decodes a (growing) cache file via symphonia,
/// streaming samples as they arrive. Works on a partially-downloaded file
/// without seeking. Samples are produced as normalized `f32` (rodio's sample
/// type) and the known `expected_duration` is reported so the progress bar
/// works even though the container length is unknown mid-stream.
pub(super) struct SymphoniaStreamingSource {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    spec: AudioSpec,
    samples: Vec<f32>,
    sample_offset: usize,
    expected_duration: f32,
    track_id: u32,
    time_base: Option<TimeBase>,
    /// Linear sample-scaling gain (1.0 = unchanged). Applied per-sample so
    /// the gain composes with the player's master volume and peaks are
    /// clamped before the mixer.
    gain: f32,
    /// The download flag shared with the copy thread (`None` for a complete
    /// cached file). Consulted live so a seek attempted while the download is
    /// still running is rejected without touching the reader state, while one
    /// after completion goes through against the now-whole file.
    writer_alive: Option<Arc<AtomicBool>>,
}

impl SymphoniaStreamingSource {
    pub(super) fn new(
        source: GrowingMediaSource,
        expected_duration: f32,
        gain: f32,
    ) -> Result<Self, String> {
        let writer_alive = source.writer_alive.clone();
        let mss = MediaSourceStream::new(
            Box::new(source),
            symphonia::core::io::MediaSourceStreamOptions::default(),
        );

        let mut format = symphonia::default::get_probe()
            .probe(
                &Hint::new(),
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| format!("probe failed: {e:?}"))?;

        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| "no supported audio track".to_string())?;
        let track_id = track.id;
        let time_base = track.time_base;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .ok_or_else(|| "no supported audio track".to_string())?;

        let mut decoder =
            make_audio_decoder(params).map_err(|e| format!("codec init failed: {e:?}"))?;

        // Decode the first packet to establish the signal spec / buffer.
        let first_audio = loop {
            let packet = match format.next_packet() {
                Ok(Some(packet)) => packet,
                // EOF here means the (still-growing) source has no more data
                // to give; treat as not-yet-ready.
                Ok(None) | Err(Error::IoError(_)) => return Err("not enough data yet".to_string()),
                Err(e) => return Err(format!("packet read failed: {e:?}")),
            };
            if packet.track_id == track_id {
                break match decoder.decode(&packet) {
                    Ok(audio) => audio,
                    Err(Error::DecodeError(_) | Error::IoError(_)) => continue,
                    Err(e) => return Err(format!("decode failed: {e:?}")),
                };
            }
        };

        let spec = first_audio.spec().clone();
        let mut samples = Vec::new();
        first_audio.copy_to_vec_interleaved(&mut samples);

        Ok(Self {
            format,
            decoder,
            spec,
            samples,
            sample_offset: 0,
            expected_duration,
            track_id,
            time_base,
            gain,
            writer_alive,
        })
    }

    fn is_live(&self) -> bool {
        self.writer_alive
            .as_ref()
            .is_some_and(|w| w.load(Ordering::Acquire))
    }

    /// Decode `packet`, retrying on up to `RETRIES` further packets when the
    /// decoder reports a recoverable error, then install the result as the
    /// current sample buffer.
    ///
    /// Shared by [`Iterator::next`] (steady-state playback) and
    /// [`rodio::Source::try_seek`] (post-seek refill); both need the exact
    /// same retry-then-rebuild sequence, only their error types differ, so
    /// this returns a plain `Option` that each caller maps.
    fn decode_into_buffer(&mut self, packet: &Packet) -> Option<()> {
        const RETRIES: usize = 3;

        if let Ok(audio) = self.decoder.decode(packet) {
            install_decoded(&audio, &mut self.spec, &mut self.samples);
            return Some(());
        }
        for _ in 0..RETRIES {
            let next = self.next_audio_packet()?;
            if let Ok(audio) = self.decoder.decode(&next) {
                install_decoded(&audio, &mut self.spec, &mut self.samples);
                return Some(());
            }
        }
        None
    }

    /// Pull the next packet belonging to the selected audio track, skipping
    /// packets from other tracks in the container. Transient reader errors
    /// (e.g. isomp4 reporting `out-of-bounds for a non-seekable stream` on
    /// muxed containers mid-track) are skipped up to [`MAX_PACKET_ERRORS`]; a
    /// clean EOF ends the stream, and only a sustained failure run is treated
    /// like one.
    fn next_audio_packet(&mut self) -> Option<Packet> {
        let mut errors = 0;
        loop {
            match self.format.next_packet() {
                Ok(Some(packet)) => {
                    if packet.track_id == self.track_id {
                        return Some(packet);
                    }
                }
                Ok(None) => return None,
                Err(_) => {
                    errors += 1;
                    if errors > MAX_PACKET_ERRORS {
                        return None;
                    }
                }
            }
        }
    }
}

fn install_decoded(
    audio: &GenericAudioBufferRef<'_>,
    spec: &mut AudioSpec,
    samples: &mut Vec<f32>,
) {
    let audio_spec = audio.spec();
    if *audio_spec != *spec {
        *spec = audio_spec.clone();
    }
    audio.copy_to_vec_interleaved(samples);
}

impl Iterator for SymphoniaStreamingSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        while self.sample_offset >= self.samples.len() {
            let packet = self.next_audio_packet()?;
            if self.decode_into_buffer(&packet).is_none() {
                continue;
            }
            self.sample_offset = 0;
        }
        let sample = self.samples[self.sample_offset];
        self.sample_offset += 1;
        // Apply the volume-normalization gain (clamped to the f32 sample range).
        Some((sample * self.gain).clamp(-1.0, 1.0))
    }
}

impl rodio::Source for SymphoniaStreamingSource {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len())
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(self.spec.channels().count().max(1) as u16)
            .expect("audio specs always have at least one channel")
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(self.spec.rate().max(1)).expect("audio sample rates are non-zero")
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        if self.expected_duration > 0.0 {
            Some(std::time::Duration::from_secs_f32(self.expected_duration))
        } else {
            None
        }
    }

    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        const FAILED: rodio::source::SeekError = rodio::source::SeekError::NotSupported {
            underlying_source: "streaming source seek failed",
        };

        // Reject seeks while the download is still running without touching
        // the reader: a failed `format.seek` on a sequential stream leaves it
        // positioned mid-track, after which playback cut out within a second.
        // Once the download has finished the file is whole, so a seek then
        // goes through against the complete data.
        if self.is_live() {
            return Err(FAILED);
        }
        let time = Time::try_from_nanos_u128(pos.as_nanos()).ok_or(FAILED)?;
        let seek_res = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|_| FAILED)?;

        self.decoder.reset();

        // Symphonia seeks to the nearest packet boundary at or before the
        // target, so skip whole packets until the one containing the requested
        // timestamp, then offset into it below. Timestamps are in the audio
        // track's timebase, which is not necessarily one tick per frame.
        let mut ticks_to_pass = seek_res
            .required_ts
            .duration_from(seek_res.actual_ts)
            .map_or(0, Duration::get);
        let packet = loop {
            let candidate = self
                .format
                .next_packet()
                .map_err(|_| FAILED)?
                .ok_or(FAILED)?;
            if candidate.track_id != self.track_id {
                continue;
            }
            let duration = candidate.dur.get();
            if duration == 0 || duration > ticks_to_pass {
                break candidate;
            }
            ticks_to_pass -= duration;
        };

        self.decode_into_buffer(&packet).ok_or(FAILED)?;

        let frames_to_skip = match self.time_base {
            Some(time_base) => time_base
                .calc_duration(Duration::new(ticks_to_pass))
                .map_or(ticks_to_pass as usize, |time| {
                    (time.as_secs_f64() * f64::from(self.spec.rate())).round() as usize
                }),
            None => ticks_to_pass as usize,
        };
        let offset = frames_to_skip.saturating_mul(self.spec.channels().count());
        self.sample_offset = offset.min(self.samples.len());
        Ok(())
    }
}
