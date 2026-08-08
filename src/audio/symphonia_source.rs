use super::growing::GrowingMediaSource;

/// A rodio `Source` that decodes a (growing) cache file via symphonia,
/// streaming samples as they arrive. Mirrors rodio's internal
/// `SymphoniaDecoder` but built over our non-seekable `GrowingMediaSource`,
/// so it works on a partially-downloaded file without seeking. Samples are
/// produced as `i16` (matching rodio's `Sink` expectation) and the known
/// `expected_duration` is reported so the progress bar works even though the
/// container length is unknown mid-stream.
pub(super) struct SymphoniaStreamingSource {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    spec: symphonia::core::audio::SignalSpec,
    buffer: symphonia::core::audio::SampleBuffer<i16>,
    current_frame_offset: usize,
    expected_duration: f32,
    track_id: u32,
}

impl SymphoniaStreamingSource {
    pub(super) fn new(source: GrowingMediaSource, expected_duration: f32) -> Result<Self, String> {
        use symphonia::core::{
            codecs::{DecoderOptions, CODEC_TYPE_NULL},
            formats::FormatOptions,
            io::MediaSourceStream,
            meta::MetadataOptions,
            probe::Hint,
        };

        let mss = MediaSourceStream::new(
            Box::new(source),
            symphonia::core::io::MediaSourceStreamOptions::default(),
        );

        let mut probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| format!("probe failed: {e:?}"))?;

        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| "no supported audio track".to_string())?;
        let track_id = track.id;

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| format!("codec init failed: {e:?}"))?;

        // Decode the first packet to establish the signal spec / buffer.
        let first_decoded = loop {
            let packet = match probed.format.next_packet() {
                Ok(p) => p,
                // IoError here means the (still-growing) source hit a
                // temporary EOF while blocking; treat as not-yet-ready.
                Err(symphonia::core::errors::Error::IoError(_)) => {
                    return Err("not enough data yet".to_string())
                }
                Err(e) => return Err(format!("packet read failed: {e:?}")),
            };
            if packet.track_id() == track_id {
                break match decoder.decode(&packet) {
                    Ok(d) => d,
                    Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                    Err(e) => return Err(format!("decode failed: {e:?}")),
                };
            }
        };

        let spec = first_decoded.spec().to_owned();
        let mut buffer = symphonia::core::audio::SampleBuffer::<i16>::new(
            symphonia::core::units::Duration::from(first_decoded.capacity() as u64),
            spec,
        );
        buffer.copy_interleaved_ref(first_decoded);

        Ok(Self {
            format: probed.format,
            decoder,
            spec,
            buffer,
            current_frame_offset: 0,
            expected_duration,
            track_id,
        })
    }

    /// Decode `packet`, retrying on up to `RETRIES` further packets when the
    /// decoder reports a recoverable error, then install the result as the
    /// current sample buffer.
    ///
    /// Shared by [`Iterator::next`] (steady-state playback) and
    /// [`rodio::Source::try_seek`] (post-seek refill); both need the exact
    /// same retry-then-rebuild sequence, only their error types differ, so
    /// this returns a plain `Option` that each caller maps.
    fn decode_into_buffer(&mut self, packet: &symphonia::core::formats::Packet) -> Option<()> {
        const RETRIES: usize = 3;

        let mut decoded = self.decoder.decode(packet);
        for _ in 0..RETRIES {
            if decoded.is_ok() {
                break;
            }
            let next = self.format.next_packet().ok()?;
            decoded = self.decoder.decode(&next);
        }
        let decoded = decoded.ok()?;

        let spec = decoded.spec().to_owned();
        let duration = symphonia::core::units::Duration::from(decoded.capacity() as u64);
        let mut buffer = symphonia::core::audio::SampleBuffer::<i16>::new(duration, spec);
        buffer.copy_interleaved_ref(decoded);
        self.spec = spec;
        self.buffer = buffer;
        Some(())
    }

    /// Pull the next packet belonging to the selected audio track, skipping
    /// packets from other tracks in the container.
    fn next_audio_packet(&mut self) -> Option<symphonia::core::formats::Packet> {
        loop {
            let p = self.format.next_packet().ok()?;
            if p.track_id() == self.track_id {
                return Some(p);
            }
        }
    }
}

impl Iterator for SymphoniaStreamingSource {
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        if self.current_frame_offset >= self.buffer.len() {
            let packet = self.next_audio_packet()?;
            self.decode_into_buffer(&packet)?;
            self.current_frame_offset = 0;
        }
        let sample = self.buffer.samples()[self.current_frame_offset];
        self.current_frame_offset += 1;
        Some(sample)
    }
}

impl rodio::Source for SymphoniaStreamingSource {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.buffer.samples().len())
    }

    fn channels(&self) -> u16 {
        self.spec.channels.count() as u16
    }

    fn sample_rate(&self) -> u32 {
        self.spec.rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        if self.expected_duration > 0.0 {
            Some(std::time::Duration::from_secs_f32(self.expected_duration))
        } else {
            None
        }
    }

    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        use symphonia::core::formats::{SeekMode, SeekTo};

        const FAILED: rodio::source::SeekError = rodio::source::SeekError::NotSupported {
            underlying_source: "streaming source seek failed",
        };

        // Live (still-downloading) sources are non-seekable, so the underlying
        // `format.seek` returns an error that we surface as a seek failure.
        let seek_res = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: pos.as_secs_f64().into(),
                    track_id: None,
                },
            )
            .map_err(|_| FAILED)?;

        // Symphonia seeks to the nearest packet boundary at or before the
        // target, so skip whole packets until the one containing the requested
        // timestamp, then offset into it below.
        let mut samples_to_pass = seek_res.required_ts - seek_res.actual_ts;
        let packet = loop {
            let candidate = self.format.next_packet().map_err(|_| FAILED)?;
            if candidate.dur() > samples_to_pass {
                break candidate;
            }
            samples_to_pass -= candidate.dur();
        };

        self.decode_into_buffer(&packet).ok_or(FAILED)?;
        self.current_frame_offset = samples_to_pass as usize * self.channels() as usize;
        Ok(())
    }
}
