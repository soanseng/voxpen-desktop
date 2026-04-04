pub mod chunker;
pub mod ducking;
pub mod encoder;
pub mod recorder;

/// Default RMS threshold below which audio is considered silent.
///
/// For 16-bit PCM (range -32768..32767):
/// - Dead silence: RMS ≈ 0
/// - Quiet room background: RMS ≈ 20–80
/// - Quiet speech (laptop mic): RMS ≈ 80–500
/// - Normal speech: RMS ≈ 500–8000
///
/// 50 is conservative — catches genuine dead silence while allowing quiet
/// laptop mics and low-gain devices (e.g. Meteor Lake sof-hda-dsp via
/// PipeWire F32→I16 path).
const SILENCE_RMS_THRESHOLD: f64 = 50.0;

/// Check if PCM audio data is effectively silent (no speech detected).
///
/// Uses RMS (root mean square) energy. Returns `true` if the audio energy
/// is below the silence threshold, indicating the user didn't speak.
pub fn is_silent(pcm_data: &[i16]) -> bool {
    if pcm_data.is_empty() {
        return true;
    }
    let sum_sq: f64 = pcm_data.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / pcm_data.len() as f64).sqrt();
    rms < SILENCE_RMS_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_detect_empty_audio_as_silent() {
        assert!(is_silent(&[]));
    }

    #[test]
    fn should_detect_zero_samples_as_silent() {
        assert!(is_silent(&[0; 16000]));
    }

    #[test]
    fn should_detect_low_noise_as_silent() {
        // Simulate very low background noise (RMS ≈ 15)
        let data: Vec<i16> = (0..16000).map(|i| (i % 30) as i16 - 15).collect();
        assert!(is_silent(&data));
    }

    #[test]
    fn should_detect_speech_level_audio_as_not_silent() {
        // Simulate speech-level signal (RMS ≈ 3000)
        let data: Vec<i16> = (0..16000)
            .map(|i| (3000.0 * (i as f64 * 0.1).sin()) as i16)
            .collect();
        assert!(!is_silent(&data));
    }

    #[test]
    fn should_detect_quiet_speech_as_not_silent() {
        // Simulate quiet speech (RMS ≈ 500)
        let data: Vec<i16> = (0..16000)
            .map(|i| (500.0 * (i as f64 * 0.1).sin()) as i16)
            .collect();
        assert!(!is_silent(&data));
    }
}
