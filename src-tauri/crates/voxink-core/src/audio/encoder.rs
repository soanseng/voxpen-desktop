/// PCM sample rate in Hz (matching Android AudioRecorder)
pub const SAMPLE_RATE: u32 = 16_000;
/// Number of audio channels (mono)
pub const CHANNELS: u16 = 1;
/// Bits per sample
pub const BITS_PER_SAMPLE: u16 = 16;
/// WAV RIFF header size in bytes
pub const WAV_HEADER_SIZE: u32 = 44;

/// Encode raw PCM i16 samples into a WAV file (in-memory).
///
/// Produces a complete WAV file with a 44-byte RIFF header followed by PCM data.
/// Matches Android's `AudioEncoder.pcmToWav()` logic exactly.
pub fn pcm_to_wav(pcm_data: &[i16]) -> Vec<u8> {
    let data_size = (pcm_data.len() * 2) as u32; // 2 bytes per i16 sample
    let file_size = WAV_HEADER_SIZE + data_size;
    let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE) / 8;
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;

    let mut wav = Vec::with_capacity(file_size as usize);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(file_size - 8).to_le_bytes()); // file size minus 8
    wav.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    // data sub-chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    // PCM samples as little-endian bytes
    for sample in pcm_data {
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_produce_44_byte_header() {
        let wav = pcm_to_wav(&[]);
        assert!(wav.len() >= WAV_HEADER_SIZE as usize);
    }

    #[test]
    fn should_start_with_riff_magic() {
        let wav = pcm_to_wav(&[]);
        assert_eq!(&wav[0..4], b"RIFF");
    }

    #[test]
    fn should_contain_wave_format() {
        let wav = pcm_to_wav(&[]);
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn should_contain_fmt_chunk() {
        let wav = pcm_to_wav(&[]);
        assert_eq!(&wav[12..16], b"fmt ");
    }

    #[test]
    fn should_set_pcm_format_code() {
        let wav = pcm_to_wav(&[]);
        let format = u16::from_le_bytes([wav[20], wav[21]]);
        assert_eq!(format, 1); // PCM
    }

    #[test]
    fn should_set_mono_channel() {
        let wav = pcm_to_wav(&[]);
        let channels = u16::from_le_bytes([wav[22], wav[23]]);
        assert_eq!(channels, 1);
    }

    #[test]
    fn should_set_16khz_sample_rate() {
        let wav = pcm_to_wav(&[]);
        let rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(rate, 16_000);
    }

    #[test]
    fn should_set_16_bits_per_sample() {
        let wav = pcm_to_wav(&[]);
        let bps = u16::from_le_bytes([wav[34], wav[35]]);
        assert_eq!(bps, 16);
    }

    #[test]
    fn should_contain_data_chunk() {
        let wav = pcm_to_wav(&[]);
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn should_encode_pcm_samples_correctly() {
        let samples: Vec<i16> = vec![0, 1000, -1000, i16::MAX, i16::MIN];
        let wav = pcm_to_wav(&samples);

        // Data starts at byte 44
        let data = &wav[44..];
        assert_eq!(data.len(), samples.len() * 2);

        // Verify each sample
        for (i, &expected) in samples.iter().enumerate() {
            let actual = i16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn should_set_correct_file_size() {
        let samples = vec![100i16; 80]; // 80 samples = 160 bytes
        let wav = pcm_to_wav(&samples);

        let file_size = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]);
        // RIFF chunk size = total file size - 8
        assert_eq!(file_size, wav.len() as u32 - 8);
    }

    #[test]
    fn should_set_correct_data_size() {
        let samples = vec![100i16; 50]; // 50 samples = 100 bytes
        let wav = pcm_to_wav(&samples);

        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 100);
    }

    #[test]
    fn should_produce_empty_wav_for_no_samples() {
        let wav = pcm_to_wav(&[]);
        assert_eq!(wav.len(), WAV_HEADER_SIZE as usize);

        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 0);
    }
}
