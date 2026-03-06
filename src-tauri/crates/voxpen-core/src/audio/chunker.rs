use crate::error::AppError;

/// Maximum chunk size in bytes (25 MB, matching Groq/OpenAI limit).
pub const MAX_CHUNK_SIZE: usize = 25 * 1024 * 1024;

/// Minimum WAV header size (RIFF + WAVE + at least one sub-chunk header).
const MIN_WAV_SIZE: usize = 12;

/// Parsed WAV layout: positions of fmt and data chunks needed for chunking/duration.
struct WavLayout {
    /// Offset of the `fmt ` sub-chunk data (after the 8-byte sub-chunk header).
    fmt_data_offset: usize,
    /// Offset of the `data` sub-chunk's size field (the 4 bytes right after "data").
    data_size_offset: usize,
    /// Offset where PCM data begins (right after the data sub-chunk header).
    pcm_start: usize,
    /// Size of PCM data as declared in the data sub-chunk header.
    data_size: u32,
}

/// Scan WAV sub-chunks to locate `fmt ` and `data` positions.
///
/// Real WAV files may contain extra sub-chunks (LIST, INFO, bext, JUNK, fact, etc.)
/// between `fmt ` and `data`, so we cannot assume fixed offsets.
fn parse_wav_layout(wav_data: &[u8]) -> Result<WavLayout, AppError> {
    if wav_data.len() < MIN_WAV_SIZE {
        return Err(AppError::Audio("invalid WAV file: too short".to_string()));
    }
    if &wav_data[0..4] != b"RIFF" || &wav_data[8..12] != b"WAVE" {
        return Err(AppError::Audio(
            "invalid WAV file: not RIFF/WAVE".to_string(),
        ));
    }

    let mut pos = 12; // skip RIFF header + "WAVE"
    let mut fmt_data_offset: Option<usize> = None;
    let mut data_size_offset: Option<usize> = None;
    let mut pcm_start: Option<usize> = None;
    let mut data_size: Option<u32> = None;

    while pos + 8 <= wav_data.len() {
        let chunk_id = &wav_data[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            wav_data[pos + 4],
            wav_data[pos + 5],
            wav_data[pos + 6],
            wav_data[pos + 7],
        ]) as usize;

        if chunk_id == b"fmt " {
            fmt_data_offset = Some(pos + 8);
        } else if chunk_id == b"data" {
            data_size_offset = Some(pos + 4);
            pcm_start = Some(pos + 8);
            data_size = Some(chunk_size as u32);
            break; // data is always the last chunk we care about
        }

        // Advance to next sub-chunk (sizes are word-aligned in some files)
        pos += 8 + chunk_size;
    }

    let fmt_data_offset =
        fmt_data_offset.ok_or_else(|| AppError::Audio("WAV missing fmt chunk".to_string()))?;
    let data_size_offset =
        data_size_offset.ok_or_else(|| AppError::Audio("WAV missing data chunk".to_string()))?;
    let pcm_start =
        pcm_start.ok_or_else(|| AppError::Audio("WAV missing data chunk".to_string()))?;
    let data_size =
        data_size.ok_or_else(|| AppError::Audio("WAV missing data chunk".to_string()))?;

    Ok(WavLayout {
        fmt_data_offset,
        data_size_offset,
        pcm_start,
        data_size,
    })
}

/// Split a WAV file into chunks, each with a valid WAV header.
///
/// If the file is smaller than `MAX_CHUNK_SIZE`, returns it as-is in a single-element vec.
/// Each chunk gets a copy of the original header (everything before PCM data) with updated sizes.
///
/// Mirrors Android's `AudioChunker`.
pub fn chunk_wav(wav_data: &[u8]) -> Result<Vec<Vec<u8>>, AppError> {
    let layout = parse_wav_layout(wav_data)?;

    if wav_data.len() <= MAX_CHUNK_SIZE {
        return Ok(vec![wav_data.to_vec()]);
    }

    let header = &wav_data[..layout.pcm_start];
    let header_size = header.len();
    let pcm_data = &wav_data[layout.pcm_start..];
    let max_pcm_per_chunk = MAX_CHUNK_SIZE - header_size;

    let mut chunks = Vec::new();
    let mut offset = 0;

    while offset < pcm_data.len() {
        let end = (offset + max_pcm_per_chunk).min(pcm_data.len());
        let chunk_pcm = &pcm_data[offset..end];

        let mut chunk = Vec::with_capacity(header_size + chunk_pcm.len());
        chunk.extend_from_slice(header);

        // Update RIFF file size (bytes 4-7): total_size - 8
        let file_size = (header_size + chunk_pcm.len() - 8) as u32;
        chunk[4..8].copy_from_slice(&file_size.to_le_bytes());

        // Update data sub-chunk size at its actual position in the header
        let data_size = chunk_pcm.len() as u32;
        let ds_off = layout.data_size_offset;
        chunk[ds_off..ds_off + 4].copy_from_slice(&data_size.to_le_bytes());

        chunk.extend_from_slice(chunk_pcm);
        chunks.push(chunk);

        offset = end;
    }

    Ok(chunks)
}

/// Calculate the duration of a WAV file in seconds from its header.
pub fn wav_duration_seconds(wav_data: &[u8]) -> Result<f64, AppError> {
    let layout = parse_wav_layout(wav_data)?;

    // fmt chunk layout: offset 0=audio_format(2), 2=num_channels(2), 4=sample_rate(4),
    //                   8=byte_rate(4), 12=block_align(2), 14=bits_per_sample(2)
    let fmt = layout.fmt_data_offset;
    if fmt + 16 > wav_data.len() {
        return Err(AppError::Audio("fmt chunk too short".to_string()));
    }
    let num_channels = u16::from_le_bytes([wav_data[fmt + 2], wav_data[fmt + 3]]);
    let sample_rate = u32::from_le_bytes([
        wav_data[fmt + 4],
        wav_data[fmt + 5],
        wav_data[fmt + 6],
        wav_data[fmt + 7],
    ]);
    let bits_per_sample = u16::from_le_bytes([wav_data[fmt + 14], wav_data[fmt + 15]]);

    let bytes_per_sample = (bits_per_sample as u32 / 8) * num_channels as u32;
    if sample_rate == 0 || bytes_per_sample == 0 {
        return Err(AppError::Audio("invalid WAV header values".to_string()));
    }
    let total_samples = layout.data_size / bytes_per_sample;
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
        let layout = parse_wav_layout(&wav).unwrap();
        let chunks = chunk_wav(&wav).unwrap();

        for chunk in &chunks {
            let file_size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            assert_eq!(file_size, (chunk.len() - 8) as u32);

            let ds_off = layout.data_size_offset;
            let data_size =
                u32::from_le_bytes([chunk[ds_off], chunk[ds_off + 1], chunk[ds_off + 2], chunk[ds_off + 3]]);
            assert_eq!(data_size, (chunk.len() - layout.pcm_start) as u32);
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

    /// Build a WAV with an extra JUNK sub-chunk between fmt and data,
    /// simulating real-world WAV files that don't have a 44-byte header.
    fn make_wav_with_extra_chunks(num_samples: usize) -> Vec<u8> {
        let samples: Vec<i16> = (0..num_samples).map(|i| (i % 1000) as i16).collect();
        let standard = encoder::pcm_to_wav(&samples);

        // Split standard WAV: [RIFF(4) + size(4) + WAVE(4) + fmt_chunk(24) | data_chunk_header(8) + pcm]
        let riff_wave_fmt = &standard[..36]; // everything up to "data"
        let data_header = &standard[36..44]; // "data" + size
        let pcm = &standard[44..];

        // Insert a JUNK sub-chunk (12 bytes: "JUNK" + size(4) + 4 bytes of padding)
        let junk_data = [0u8; 4];
        let junk_size = (junk_data.len() as u32).to_le_bytes();

        let new_total = riff_wave_fmt.len() + 8 + junk_data.len() + data_header.len() + pcm.len();
        let mut wav = Vec::with_capacity(new_total);
        wav.extend_from_slice(riff_wave_fmt);
        wav.extend_from_slice(b"JUNK");
        wav.extend_from_slice(&junk_size);
        wav.extend_from_slice(&junk_data);
        wav.extend_from_slice(data_header);
        wav.extend_from_slice(pcm);

        // Fix RIFF file size (bytes 4-7)
        let file_size = (wav.len() - 8) as u32;
        wav[4..8].copy_from_slice(&file_size.to_le_bytes());
        wav
    }

    #[test]
    fn should_handle_wav_with_extra_subchunks() {
        let wav = make_wav_with_extra_chunks(32000);
        // data chunk is NOT at byte 36 anymore
        assert_ne!(&wav[36..40], b"data");

        let duration = wav_duration_seconds(&wav).unwrap();
        assert!((duration - 2.0).abs() < 0.001);

        let chunks = chunk_wav(&wav).unwrap();
        assert_eq!(chunks.len(), 1);
        // Verify the chunk is valid
        let dur2 = wav_duration_seconds(&chunks[0]).unwrap();
        assert!((dur2 - 2.0).abs() < 0.001);
    }
}
