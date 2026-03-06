use crate::api::groq::WhisperSegment;

/// Format seconds into SRT timestamp: HH:MM:SS,mmm
fn format_timestamp(seconds: f64) -> String {
    let total_ms = (seconds * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

/// Format segments into SRT subtitle format.
pub fn format_srt(segments: &[WhisperSegment]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n",
            i + 1,
            format_timestamp(seg.start),
            format_timestamp(seg.end),
            seg.text.trim(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_format_timestamp_correctly() {
        assert_eq!(format_timestamp(0.0), "00:00:00,000");
        assert_eq!(format_timestamp(1.5), "00:00:01,500");
        assert_eq!(format_timestamp(61.123), "00:01:01,123");
        assert_eq!(format_timestamp(3661.5), "01:01:01,500");
    }

    #[test]
    fn should_format_single_segment_srt() {
        let segments = vec![WhisperSegment {
            start: 0.0,
            end: 1.5,
            text: "Hello world.".to_string(),
        }];
        let srt = format_srt(&segments);
        assert_eq!(srt, "1\n00:00:00,000 --> 00:00:01,500\nHello world.\n");
    }

    #[test]
    fn should_format_multiple_segments_srt() {
        let segments = vec![
            WhisperSegment { start: 0.0, end: 1.5, text: "Hello.".to_string() },
            WhisperSegment { start: 1.5, end: 3.0, text: "World.".to_string() },
        ];
        let srt = format_srt(&segments);
        let expected = "1\n00:00:00,000 --> 00:00:01,500\nHello.\n\n2\n00:00:01,500 --> 00:00:03,000\nWorld.\n";
        assert_eq!(srt, expected);
    }

    #[test]
    fn should_return_empty_string_for_no_segments() {
        assert_eq!(format_srt(&[]), "");
    }
}
