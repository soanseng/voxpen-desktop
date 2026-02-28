use crate::pipeline::state::Language;

/// Apply voice command substitutions to raw STT output.
pub fn apply(text: &str, _lang: &Language) -> String {
    // Ordered longest-pattern-first to avoid "new line" consuming "new paragraph".
    const COMMANDS: &[(&str, &str)] = &[
        // 2-word English — must come before 1-word
        ("new paragraph", "\n\n"),
        ("question mark", "?"),
        ("exclamation mark", "!"),
        ("exclamation point", "!"),
        ("full stop", "."),
        ("new line", "\n"),
        // 1-word English
        ("comma", ","),
        ("period", "."),
        // Traditional Chinese (multi-char before single)
        ("新段落", "\n\n"),
        ("疑問符", "?"),
        ("驚嘆號", "!"),
        ("新行", "\n"),
        ("逗號", ","),
        ("句號", "."),
        // Japanese (multi-char before single)
        ("新しい段落", "\n\n"),
        ("改行", "\n"),
        // Korean (multi-word before single)
        ("새 단락", "\n\n"),
        ("새 줄", "\n"),
        ("물음표", "?"),
        ("느낌표", "!"),
        ("마침표", "."),
        ("쉼표", ","),
    ];

    let mut out = text.to_string();
    for (pat, rep) in COMMANDS {
        out = replace_ci(&out, pat, rep);
    }
    normalize_spaces(&out)
}

/// Case-insensitive global string replacement.
///
/// For ASCII patterns (English keywords), matching is case-insensitive and
/// surrounding spaces are absorbed according to these rules:
///
/// - One leading space is always absorbed (the space before the keyword).
/// - One trailing space is absorbed only when the replacement is a newline
///   sequence (so the following word starts cleanly on a new line).
///   For punctuation replacements the trailing space is preserved so the
///   next word keeps its leading space: "hello, world" not "hello,world".
///
/// For non-ASCII patterns (CJK), exact match is used.
fn replace_ci(text: &str, pat: &str, rep: &str) -> String {
    if pat.is_ascii() {
        // Absorb trailing space only for newline replacements.
        let absorb_trailing = rep.starts_with('\n');

        let lower_text = text.to_lowercase();
        let lower_pat = pat.to_lowercase();
        let bytes = text.as_bytes();
        let mut result = String::with_capacity(text.len());
        let mut last = 0usize;
        let mut search = 0usize;

        while let Some(rel) = lower_text[search..].find(lower_pat.as_str()) {
            let match_start = search + rel;
            let match_end = match_start + lower_pat.len();

            // Absorb one leading space.
            let absorb_start = if match_start > last && bytes.get(match_start - 1) == Some(&b' ') {
                match_start - 1
            } else {
                match_start
            };

            // Conditionally absorb one trailing space.
            let absorb_end = if absorb_trailing && bytes.get(match_end) == Some(&b' ') {
                match_end + 1
            } else {
                match_end
            };

            result.push_str(&text[last..absorb_start]);
            result.push_str(rep);
            last = absorb_end;
            search = absorb_end;
        }
        result.push_str(&text[last..]);
        result
    } else {
        // Exact match for CJK
        text.replace(pat, rep)
    }
}

/// Collapse runs of spaces (but preserve newlines).
fn normalize_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(ch);
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    // Trim leading/trailing spaces (but not newlines)
    out.trim_matches(' ').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::state::Language;

    #[test]
    fn should_return_unchanged_text_with_no_commands() {
        assert_eq!(apply("hello world", &Language::English), "hello world");
    }

    #[test]
    fn should_replace_english_comma() {
        assert_eq!(apply("hello comma world", &Language::English), "hello, world");
    }

    #[test]
    fn should_replace_english_period() {
        assert_eq!(apply("done period", &Language::English), "done.");
    }

    #[test]
    fn should_replace_english_question_mark() {
        assert_eq!(apply("are you sure question mark", &Language::English), "are you sure?");
    }

    #[test]
    fn should_replace_english_new_line() {
        assert_eq!(apply("first new line second", &Language::English), "first\nsecond");
    }

    #[test]
    fn should_replace_english_new_paragraph() {
        assert_eq!(apply("intro new paragraph body", &Language::English), "intro\n\nbody");
    }

    #[test]
    fn should_replace_chinese_comma() {
        assert_eq!(apply("你好逗號世界", &Language::Chinese), "你好,世界");
    }

    #[test]
    fn should_replace_chinese_new_line() {
        assert_eq!(apply("第一行新行第二行", &Language::Chinese), "第一行\n第二行");
    }

    #[test]
    fn should_replace_chinese_new_paragraph() {
        assert_eq!(apply("介紹新段落正文", &Language::Chinese), "介紹\n\n正文");
    }

    #[test]
    fn should_be_case_insensitive_for_english() {
        assert_eq!(apply("Hello Comma World", &Language::English), "Hello, World");
        assert_eq!(apply("COMMA", &Language::English), ",");
    }

    #[test]
    fn should_replace_new_paragraph_before_new_line() {
        assert_eq!(apply("intro new paragraph body", &Language::English), "intro\n\nbody");
    }

    #[test]
    fn should_handle_multiple_commands_in_sequence() {
        let result = apply("first comma second new line third", &Language::English);
        assert_eq!(result, "first, second\nthird");
    }

    #[test]
    fn should_normalize_extra_spaces_after_replacement() {
        let result = apply("done period ", &Language::English);
        assert!(!result.contains("  "), "double space found: {:?}", result);
    }
}
