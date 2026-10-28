//! Symphonia audio decoder construction.
//!
//! Symphonia 0.6's AAC decoder rejects HE-AAC (SBR) outright, while 0.5
//! decoded the AAC-LC core at the core sample rate. The muxed MP4 fallback
//! for age-restricted tracks is sometimes HE-AAC, so retry such tracks with a
//! minimal AAC-LC audio-specific config to keep them playable (without the SBR
//! extension), matching the previous behavior.

use symphonia::core::{
    codecs::audio::{
        well_known::CODEC_ID_AAC, AudioCodecParameters, AudioDecoder, AudioDecoderOptions,
    },
    errors::Error,
};

pub(super) fn make_audio_decoder(
    params: &AudioCodecParameters,
) -> Result<Box<dyn AudioDecoder>, Error> {
    let options = AudioDecoderOptions::default();
    let result = symphonia::default::get_codecs().make_audio_decoder(params, &options);
    let original = match result {
        Err(error) if matches!(&error, Error::Unsupported(_)) => error,
        _ => return result,
    };
    let Some(params) = aac_lc_params(params) else {
        return Err(original);
    };
    symphonia::default::get_codecs()
        .make_audio_decoder(&params, &options)
        .map_err(|_| original)
}

/// Build an `AudioSpecificConfig` for the track's LC core: object type 2,
/// the track's sample rate and mono/stereo channel config, then the three
/// all-zero `GASpecificConfig` flags (`frameLengthFlag`,
/// `dependsOnCoreCoder`, `extensionFlag`).
fn aac_lc_params(params: &AudioCodecParameters) -> Option<AudioCodecParameters> {
    if params.codec != CODEC_ID_AAC {
        return None;
    }
    let rate_index: u16 = match params.sample_rate? {
        96000 => 0,
        88200 => 1,
        64000 => 2,
        48000 => 3,
        44100 => 4,
        32000 => 5,
        24000 => 6,
        22050 => 7,
        16000 => 8,
        12000 => 9,
        11025 => 10,
        8000 => 11,
        7350 => 12,
        _ => return None,
    };
    let channel_config: u16 = match params.channels.as_ref()?.count() {
        1 => 1,
        2 => 2,
        _ => return None,
    };
    let asc = (2u16 << 11) | (rate_index << 7) | (channel_config << 3);

    let mut params = params.clone();
    params.extra_data = Some(Box::new(asc.to_be_bytes()));
    Some(params)
}

#[cfg(test)]
mod tests {
    use symphonia::core::audio::Channels;

    use super::*;

    fn params(sample_rate: u32, channels: u16) -> AudioCodecParameters {
        let mut params = AudioCodecParameters::new();
        params.codec = CODEC_ID_AAC;
        params.sample_rate = Some(sample_rate);
        params.channels = Some(Channels::Discrete(channels));
        params
    }

    #[test]
    fn builds_lc_asc_for_common_rates() {
        let stereo = params(44_100, 2);
        assert_eq!(
            aac_lc_params(&stereo).unwrap().extra_data.as_deref(),
            Some(&[0x12, 0x10][..])
        );

        let stereo = params(22_050, 2);
        assert_eq!(
            aac_lc_params(&stereo).unwrap().extra_data.as_deref(),
            Some(&[0x13, 0x90][..])
        );

        let mono = params(48_000, 1);
        assert_eq!(
            aac_lc_params(&mono).unwrap().extra_data.as_deref(),
            Some(&[0x11, 0x88][..])
        );
    }

    #[test]
    fn rejects_unsupported_rates_and_layouts() {
        assert!(aac_lc_params(&params(44_001, 2)).is_none());
        assert!(aac_lc_params(&params(44_100, 3)).is_none());
        assert!(aac_lc_params(&params(44_100, 6)).is_none());
    }
}
