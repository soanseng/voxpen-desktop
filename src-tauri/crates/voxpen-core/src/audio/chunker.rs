use crate::error::AppError;

/// Maximum chunk size in bytes (25 MB, matching Groq/OpenAI limit).
pub const MAX_CHUNK_SIZE: usize = 25 * 1024 * 1024;

/// WAV header size in bytes.
const WAV_HEADER_SIZE: usize = 44;

/// Split a WAV file into chunks, each with a valid WAV header.
///
/// If the file is smaller than `MAX_CHUNK_SIZE`, returns it as-is in a single-element vec.
/// Each chunk gets a copy of the original WAV header with updated data/file sizes.
///
/// Mirrors Android's `AudioChunker`.
pub fn chunk_wav(wav_data: &[u8]) -> Result<Vec<Vec<u8>>, AppError> {
    if wav_data.len() < WAV_HEADER_SIZE {
        return Err(AppError::Audio("invalid WAV file: too short".to_string()));
    }

    if &wav_data[0..4] != b"RIFF" || &wav_data[8..12] != b"WAVE" {
        return Err(AppError::Audio(
            "invalid WAV file: not RIFF/WAVE".to_string(),
        ));
    }

    if wav_data.len() <= MAX_CHUNK_SIZE {
        return Ok(vec![wav_data.to_vec()]);
    }

    let pcm_data = &wav_data[WAV_HEADER_SIZE..];
    let max_pcm_per_chunk = MAX_CHUNK_SIZE - WAV_HEADER_SIZE;

    let mut chunks = Vec::new();
    let mut offset = 0;

    while offset < pcm_data.len() {
        let end = (offset + max_pcm_per_chunk).min(pcm_data.len());
        let chunk_pcm = &pcm_data[offset..end];

        let mut chunk = Vec::with_capacity(WAV_HEADER_SIZE + chunk_pcm.len());
        chunk.extend_from_slice(&wav_data[..WAV_HEADER_SIZE]);

        // Update file size (bytes 4-7): total_size - 8
        let file_size = (WAV_HEADER_SIZE + chunk_pcm.len() - 8) as u32;
        chunk[4..8].copy_from_slice(&file_size.to_le_bytes());

        // Update data size (bytes 40-43)
        let data_size = chunk_pcm.len() as u32;
        chunk[40..44].copy_from_slice(&data_size.to_le_bytes());

        chunk.extend_from_slice(chunk_pcm);
        chunks.push(chunk);

        offset = end;
    }

    Ok(chunks)
}

/// Calculate the duration of a WAV file in seconds from its header.
pub fn wav_duration_seconds(wav_data: &[u8]) -> Result<f64, AppError> {
    if wav_data.len() < WAV_HEADER_SIZE {
        return Err(AppError::Audio("WAV too short for duration calc".to_string()));
    }
    let sample_rate = u32::from_le_bytes([wav_data[24], wav_data[25], wav_data[26], wav_data[27]]);
    let bits_per_sample = u16::from_le_bytes([wav_data[34], wav_data[35]]);
    let num_channels = u16::from_le_bytes([wav_data[22], wav_data[23]]);
    let data_size = u32::from_le_bytes([wav_data[40], wav_data[41], wav_data[42], wav_data[43]]);

    let bytes_per_sample = (bits_per_sample as u32 / 8) * num_channels as u32;
    if sample_rate == 0 || bytes_per_sample == 0 {
        return Err(AppError::Audio("invalid WAV header values".to_string()));
    }
    let total_samples = data_size / bytes_per_sample;
    Ok(total_samples as f64 / sample_rate as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::encoder;

    /// Build a valid WAV with the given number of PCM samples.
    fn make_wav(num_samples: usize) -> Vec<u8> {
        let samples: Vec<i16> = (0..num_samples).map(|i| (i % 1000) as i16).collect();
        encoder::pcm_to_wav(&samples)
    }

    #[test]
    fn should_return_single_chunk_for_small_file() {
        let wav = make_wav(100);
        let chunks = chunk_wav(&wav).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], wav);
    }

    #[test]
    fn should_split_large_file_into_chunks() {
        // Create a WAV larger than MAX_CHUNK_SIZE.
        // Each i16 sample = 2 bytes, so we need > (25MB - 44) / 2 samples.
        let samples_needed = MAX_CHUNK_SIZE; // 25M samples * 2 bytes > 25MB
        let wav = make_wav(samples_needed);
        assert!(wav.len() > MAX_CHUNK_SIZE);

        let chunks = chunk_wav(&wav).unwrap();
        assert!(chunks.len() >= 2);

        // Each chunk must be at most MAX_CHUNK_SIZE
        for chunk in &chunks {
            assert!(chunk.len() <= MAX_CHUNK_SIZE);
        }
    }

    #[test]
    fn should_preserve_wav_header_in_each_chunk() {
        let samples_needed = MAX_CHUNK_SIZE;
        let wav = make_wav(samples_needed);
        let chunks = chunk_wav(&wav).unwrap();

        for chunk in &chunks {
            assert_eq!(&chunk[0..4], b"RIFF");
            assert_eq!(&chunk[8..12], b"WAVE");
            assert_eq!(&chunk[12..16], b"fmt ");
            assert_eq!(&chunk[36..40], b"data");
        }
    }

    #[test]
    fn should_update_file_and_data_sizes_in_chunks() {
        let samples_needed = MAX_CHUNK_SIZE;
        let wav = make_wav(samples_needed);
        let chunks = chunk_wav(&wav).unwrap();

        for chunk in &chunks {
            let file_size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            assert_eq!(file_size, (chunk.len() - 8) as u32);

            let data_size = u32::from_le_bytes([chunk[40], chunk[41], chunk[42], chunk[43]]);
            assert_eq!(data_size, (chunk.len() - WAV_HEADER_SIZE) as u32);
        }
    }

    #[test]
    fn should_reject_too_short_data() {
        let result = chunk_wav(&[0u8; 10]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn should_reject_non_wav_data() {
        let result = chunk_wav(&[0u8; 44]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not RIFF/WAVE"));
    }

    #[test]
    fn should_calculate_wav_duration_correctly() {
        // 16000 Hz, mono, 16-bit = 32000 bytes/sec
        // 32000 samples * 2 bytes = 64000 bytes PCM = 2.0 seconds
        let wav = make_wav(32000);
        let duration = wav_duration_seconds(&wav).unwrap();
        assert!((duration - 2.0).abs() < 0.001);
    }
}
