use crate::api::groq::{WhisperSegment, WhisperVerboseResponse};

/// Result of a chunked transcription: full text + all segments with correct timestamps.
#[derive(Debug, Clone)]
pub struct ChunkedTranscriptionResult {
    pub text: String,
    pub segments: Vec<WhisperSegment>,
}

/// Merge segments from multiple chunks, offsetting timestamps by each chunk's audio duration.
pub fn merge_segments(
    chunk_results: &[(WhisperVerboseResponse, f64)],
) -> ChunkedTranscriptionResult {
    let mut all_segments = Vec::new();
    let mut full_text = String::new();
    let mut time_offset = 0.0_f64;

    for (response, chunk_duration) in chunk_results {
        if !full_text.is_empty() && !response.text.is_empty() {
            full_text.push(' ');
        }
        full_text.push_str(response.text.trim());

        for seg in &response.segments {
            all_segments.push(WhisperSegment {
                start: seg.start + time_offset,
                end: seg.end + time_offset,
                text: seg.text.clone(),
            });
        }

        time_offset += chunk_duration;
    }

    ChunkedTranscriptionResult {
        text: full_text,
        segments: all_segments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_merge_two_chunks_with_time_offset() {
        let chunk1 = WhisperVerboseResponse {
            text: "Hello.".to_string(),
            segments: vec![WhisperSegment { start: 0.0, end: 1.0, text: "Hello.".to_string() }],
        };
        let chunk2 = WhisperVerboseResponse {
            text: "World.".to_string(),
            segments: vec![WhisperSegment { start: 0.0, end: 1.5, text: "World.".to_string() }],
        };

        let result = merge_segments(&[(chunk1, 10.0), (chunk2, 10.0)]);

        assert_eq!(result.text, "Hello. World.");
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].start, 0.0);
        assert_eq!(result.segments[0].end, 1.0);
        assert_eq!(result.segments[1].start, 10.0);
        assert_eq!(result.segments[1].end, 11.5);
    }

    #[test]
    fn should_handle_single_chunk() {
        let chunk = WhisperVerboseResponse {
            text: "Hello.".to_string(),
            segments: vec![WhisperSegment { start: 0.0, end: 1.0, text: "Hello.".to_string() }],
        };
        let result = merge_segments(&[(chunk, 5.0)]);
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].start, 0.0);
    }

    #[test]
    fn should_handle_empty_chunks() {
        let result = merge_segments(&[]);
        assert_eq!(result.text, "");
        assert_eq!(result.segments.len(), 0);
    }
}
