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

/// Parse an SRT timestamp (`HH:MM:SS,mmm` or `HH:MM:SS.mmm`) into seconds.
fn parse_timestamp(ts: &str) -> Result<f64, String> {
    let ts = ts.trim();
    let (hms, frac) = ts
        .split_once([',', '.'])
        .ok_or_else(|| format!("invalid SRT timestamp: {ts}"))?;
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("invalid SRT timestamp: {ts}"));
    }
    let hours: u64 = parts[0]
        .parse()
        .map_err(|_| format!("invalid SRT timestamp hours: {ts}"))?;
    let minutes: u64 = parts[1]
        .parse()
        .map_err(|_| format!("invalid SRT timestamp minutes: {ts}"))?;
    let seconds: u64 = parts[2]
        .parse()
        .map_err(|_| format!("invalid SRT timestamp seconds: {ts}"))?;
    // Fractional part as decimal seconds (`,5` → 0.5s, `,50` → 0.50s, `,500` → 0.500s).
    let frac_digits: String = frac.chars().filter(|c| c.is_ascii_digit()).take(3).collect();
    let frac_secs: f64 = if frac_digits.is_empty() {
        0.0
    } else {
        format!("0.{frac_digits}")
            .parse()
            .map_err(|_| format!("invalid SRT timestamp millis: {ts}"))?
    };
    Ok(hours as f64 * 3600.0 + minutes as f64 * 60.0 + seconds as f64 + frac_secs)
}

/// Parse a timing line: `00:00:00,000 --> 00:00:01,500` (optional trailing position coords).
fn parse_timing_line(line: &str) -> Result<(f64, f64), String> {
    let line = line.trim();
    let (left, right) = line
        .split_once("-->")
        .ok_or_else(|| format!("invalid SRT timing line: {line}"))?;
    // Right side may include position: `00:00:01,500  X1:0 ...`
    let end_token = right
        .split_whitespace()
        .next()
        .ok_or_else(|| format!("invalid SRT timing line: {line}"))?;
    Ok((parse_timestamp(left)?, parse_timestamp(end_token)?))
}

fn is_index_line(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// Parse SRT subtitle content into timestamped segments.
///
/// Supports multi-line cue text, comma/dot millis separators, optional BOM,
/// and optional cue index lines. Empty cues are skipped.
pub fn parse_srt(content: &str) -> Result<Vec<WhisperSegment>, String> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");

    let mut segments = Vec::new();
    let mut lines = normalized.lines().peekable();

    while lines.peek().is_some() {
        // Skip blank lines between cues
        while matches!(lines.peek(), Some(l) if l.trim().is_empty()) {
            lines.next();
        }
        if lines.peek().is_none() {
            break;
        }

        // Optional index line; timing line may appear first.
        let first = lines.next().unwrap_or("").trim();
        if first.is_empty() {
            continue;
        }

        let timing_line = if first.contains("-->") {
            first.to_string()
        } else if is_index_line(first) {
            while matches!(lines.peek(), Some(l) if l.trim().is_empty()) {
                lines.next();
            }
            let next = lines
                .next()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .ok_or_else(|| "SRT cue missing timing line after index".to_string())?;
            if !next.contains("-->") {
                return Err(format!(
                    "expected SRT timing line after index, got: {next}"
                ));
            }
            next.to_string()
        } else {
            return Err(format!(
                "expected SRT index or timing line, got: {first}"
            ));
        };

        let (start, end) = parse_timing_line(&timing_line)?;

        // Collect text lines until blank line or EOF
        let mut text_lines: Vec<String> = Vec::new();
        while let Some(peek) = lines.peek() {
            if peek.trim().is_empty() {
                lines.next();
                break;
            }
            text_lines.push(lines.next().unwrap().to_string());
        }

        let text = text_lines
            .iter()
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        if text.is_empty() {
            continue;
        }

        segments.push(WhisperSegment { start, end, text });
    }

    if segments.is_empty() {
        return Err("SRT file contains no subtitle cues".to_string());
    }

    Ok(segments)
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

/// Join segment texts into a plain paragraph (space-separated).
pub fn segments_to_text(segments: &[WhisperSegment]) -> String {
    segments
        .iter()
        .map(|s| s.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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
            WhisperSegment {
                start: 0.0,
                end: 1.5,
                text: "Hello.".to_string(),
            },
            WhisperSegment {
                start: 1.5,
                end: 3.0,
                text: "World.".to_string(),
            },
        ];
        let srt = format_srt(&segments);
        let expected =
            "1\n00:00:00,000 --> 00:00:01,500\nHello.\n\n2\n00:00:01,500 --> 00:00:03,000\nWorld.\n";
        assert_eq!(srt, expected);
    }

    #[test]
    fn should_return_empty_string_for_no_segments() {
        assert_eq!(format_srt(&[]), "");
    }

    #[test]
    fn should_parse_basic_srt_roundtrip() {
        let original = "1\n00:00:00,000 --> 00:00:01,500\nHello world.\n\n2\n00:00:01,500 --> 00:00:03,000\nSecond cue.\n";
        let segments = parse_srt(original).unwrap();
        assert_eq!(segments.len(), 2);
        assert!((segments[0].start - 0.0).abs() < 1e-9);
        assert!((segments[0].end - 1.5).abs() < 1e-9);
        assert_eq!(segments[0].text, "Hello world.");
        assert_eq!(segments[1].text, "Second cue.");
        assert_eq!(format_srt(&segments), original);
    }

    #[test]
    fn should_parse_multiline_cue_and_dot_millis() {
        let srt = "1\n00:00:00.000 --> 00:00:02.500\nLine one\nLine two\n";
        let segments = parse_srt(srt).unwrap();
        assert_eq!(segments.len(), 1);
        assert!((segments[0].end - 2.5).abs() < 1e-9);
        assert_eq!(segments[0].text, "Line one\nLine two");
    }

    #[test]
    fn should_parse_bom_and_crlf() {
        let srt = "\u{feff}1\r\n00:00:00,000 --> 00:00:01,000\r\nHi\r\n";
        let segments = parse_srt(srt).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hi");
    }

    #[test]
    fn should_error_on_empty_srt() {
        let err = parse_srt("\n\n").unwrap_err();
        assert!(err.contains("no subtitle cues"));
    }

    #[test]
    fn should_join_segments_to_text() {
        let segments = vec![
            WhisperSegment {
                start: 0.0,
                end: 1.0,
                text: "Hello".to_string(),
            },
            WhisperSegment {
                start: 1.0,
                end: 2.0,
                text: "world".to_string(),
            },
        ];
        assert_eq!(segments_to_text(&segments), "Hello world");
    }
}
