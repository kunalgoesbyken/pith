use unicode_segmentation::UnicodeSegmentation;

fn is_word_like(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

fn is_inline_space(c: char) -> bool {
    c.is_whitespace() && c != '\n' && c != '\r'
}

const CLOSED_SENTENCE_ENDS: [char; 3] = ['。', '！', '？'];
const SPACED_SENTENCE_ENDS: [char; 7] = ['.', '!', '?', '।', '॥', '؟', '…'];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentenceEnd {
    Closed,
    NeedsSpace,
}

fn is_initialism(word: &str) -> bool {
    let letters: Vec<&str> = word.trim_end_matches('.').split('.').collect();
    letters.len() > 1
        && letters
            .iter()
            .all(|part| part.chars().count() == 1 && part.chars().all(char::is_alphabetic))
}

fn ends_with_initialism(text: &str) -> bool {
    let last = text.rsplit(char::is_whitespace).next().unwrap_or_default();
    last.ends_with('.') && is_initialism(last.trim_start_matches(|c: char| !c.is_alphanumeric()))
}

pub fn sentence_end(text: &str) -> Option<SentenceEnd> {
    let trimmed = text.trim_end();
    if !trimmed.chars().any(char::is_alphanumeric) || ends_with_initialism(trimmed) {
        return None;
    }
    if trimmed.ends_with(CLOSED_SENTENCE_ENDS) {
        Some(SentenceEnd::Closed)
    } else if trimmed.ends_with(SPACED_SENTENCE_ENDS) {
        Some(SentenceEnd::NeedsSpace)
    } else {
        None
    }
}

pub fn tail_chars(text: &str, window: usize) -> &str {
    let total = text.graphemes(true).count();
    if window == 0 || total <= window {
        return text;
    }
    let step = (window / 2).max(1);
    let start = (((total - window) / step + 1) * step).min(total - 1);
    let start_byte = text
        .grapheme_indices(true)
        .nth(start)
        .map_or(text.len(), |(i, _)| i);
    let rest = &text[start_byte..];
    match rest.find(char::is_whitespace) {
        Some(offset) if offset < rest.len() / 2 => trim_start_graphemes(&rest[offset..]),
        _ => rest,
    }
}

fn trim_start_graphemes(text: &str) -> &str {
    text.grapheme_indices(true)
        .find(|(_, grapheme)| !grapheme.starts_with(char::is_whitespace))
        .map_or("", |(start, _)| &text[start..])
}

pub fn continue_window(previous: &str, current: &str, lookback: usize) -> String {
    let joined = join_window(previous, current, lookback / 2);
    let limit = lookback.saturating_mul(2);
    if joined.len() <= limit || joined.graphemes(true).count() <= limit {
        return joined;
    }
    let cut = joined
        .grapheme_indices(true)
        .nth(lookback)
        .map_or(joined.len(), |(index, _)| index);
    joined[cut..].to_string()
}

fn join_window(previous: &str, current: &str, min_overlap: usize) -> String {
    if current.starts_with(previous) {
        return current.to_string();
    }
    let mut remaining = previous.chars().count();
    for (start, _) in previous.char_indices() {
        if remaining < min_overlap {
            break;
        }
        if current.starts_with(&previous[start..]) {
            return format!("{previous}{}", &current[previous.len() - start..]);
        }
        remaining -= 1;
    }
    current.to_string()
}

pub fn split_partial(before: &str) -> (&str, &str) {
    let mut cut = before.len();
    if let Some((start, segment)) = before.split_word_bound_indices().next_back()
        && is_word_like(segment)
    {
        cut = start;
    }
    let head = &before[..cut];
    let trimmed = head.trim_end_matches(is_inline_space);
    let cut = trimmed.len();
    (&before[..cut], &before[cut..])
}

pub fn next_word(suggestion: &str) -> &str {
    let mut end = 0;
    let mut seen_word = false;
    for (start, segment) in suggestion.split_word_bound_indices() {
        let blank = segment.chars().all(char::is_whitespace);
        if seen_word && (blank || is_word_like(segment)) {
            break;
        }
        if is_word_like(segment) {
            seen_word = true;
        }
        end = start + segment.len();
    }
    &suggestion[..end]
}

pub fn exceeds_words(text: &str, max_words: usize) -> bool {
    text.split_word_bounds()
        .filter(|segment| is_word_like(segment))
        .nth(max_words)
        .is_some()
}

pub fn clean_completion(raw: &str, max_words: usize, truncated: bool) -> Option<String> {
    let line = raw.split(['\n', '\r']).next().unwrap_or_default();
    let mut words = 0;
    let mut end = 0;
    let mut last_word_start = None;
    for (start, segment) in line.split_word_bound_indices() {
        if is_word_like(segment) {
            if words == max_words {
                break;
            }
            words += 1;
            last_word_start = Some(start);
        } else if words == max_words && segment.chars().all(char::is_whitespace) {
            break;
        }
        end = start + segment.len();
    }
    let mut text = &line[..end];
    if truncated
        && words > 1
        && end == line.len()
        && !line.ends_with(char::is_whitespace)
        && let Some(start) = last_word_start
    {
        text = &line[..start];
    }
    let text = text.trim_end();
    is_word_like(text).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_partial_cases() {
        let cases = [
            ("I'll send the contr", "I'll send the", " contr"),
            ("I'll send the ", "I'll send the", " "),
            ("Thanks.", "Thanks.", ""),
            ("", "", ""),
            ("line\n", "line\n", ""),
            ("line\nwor", "line\n", "wor"),
            ("你好世界", "你好世", "界"),
            ("नमस्ते", "", "नमस्ते"),
            ("hola señ", "hola", " señ"),
            ("a  b", "a", "  b"),
            ("emoji 👋", "emoji 👋", ""),
        ];
        for (input, head, partial) in cases {
            assert_eq!(split_partial(input), (head, partial), "{input:?}");
        }
    }

    #[test]
    fn next_word_cases() {
        let cases = [
            (" contract, please", " contract,"),
            ("act by Friday", "act"),
            (" 世界和平", " 世"),
            ("", ""),
            ("   ", "   "),
            (" don't stop", " don't"),
            (" नमस्ते दोस्त", " नमस्ते"),
            ("... ok", "... ok"),
        ];
        for (input, expected) in cases {
            assert_eq!(next_word(input), expected, "{input:?}");
        }
    }

    #[test]
    fn clean_completion_cases() {
        let cases = [
            (
                " the contract by Friday.\nBest",
                6,
                false,
                Some(" the contract by Friday."),
            ),
            (
                " one two three four five six seven",
                6,
                false,
                Some(" one two three four five six"),
            ),
            (" one two thr", 6, true, Some(" one two")),
            (" one", 6, true, Some(" one")),
            ("\nhello", 6, false, None),
            ("  ...", 6, false, None),
            ("", 6, false, None),
            (" 你好世界", 2, false, Some(" 你好")),
        ];
        for (raw, max, truncated, expected) in cases {
            assert_eq!(
                clean_completion(raw, max, truncated).as_deref(),
                expected,
                "{raw:?}"
            );
        }
    }

    #[test]
    fn exceeds_words_cases() {
        let cases = [
            ("one two three", 3, false),
            ("one two three four", 3, true),
            (" one, two. three", 3, false),
            ("3.5 e.g. www.example.com", 3, false),
            ("你好世界", 3, true),
            ("你好世", 3, false),
            ("", 1, false),
        ];
        for (text, max, expected) in cases {
            assert_eq!(exceeds_words(text, max), expected, "{text:?}");
        }
    }

    #[test]
    fn stopping_at_the_word_limit_never_changes_the_cleaned_text() {
        let raws = [
            " the contract by Friday and then we can sign it together",
            " costs 3.5 dollars, e.g. for www.example.com/docs and more",
            " 你好世界和平与爱，我们都在这里",
            " नमस्ते दोस्त, मैं कल सुबह दस बजे आऊँगा",
            " don't stop believing, it's 12/05/2026 at 9:30 a.m. sharp",
            " und dann können wir den Vertrag unterschreiben, bitte",
        ];
        for raw in raws {
            for max in 1..=6 {
                let boundaries = raw.char_indices().map(|(i, c)| i + c.len_utf8());
                let stop = boundaries
                    .into_iter()
                    .find(|end| exceeds_words(&raw[..*end], max))
                    .unwrap_or(raw.len());
                assert_eq!(
                    clean_completion(&raw[..stop], max, false),
                    clean_completion(raw, max, false),
                    "{raw:?} max {max}"
                );
            }
        }
    }

    #[test]
    fn tail_window_start_moves_rarely() {
        let base = "word ".repeat(100);
        let starts: Vec<usize> = (0..60)
            .map(|n| {
                let text = format!("{base}{}", "y".repeat(n));
                text.len() - tail_chars(&text, 100).len()
            })
            .collect();
        let changes = starts.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(changes <= 2, "{changes}");
    }

    #[test]
    fn tail_handles_short_and_multibyte() {
        assert_eq!(tail_chars("héllo", 10), "héllo");
        assert_eq!(tail_chars("", 10), "");
        let long = "日本語".repeat(50);
        let tail = tail_chars(&long, 40);
        assert!(tail.chars().count() <= 40);
    }

    #[test]
    fn sentence_end_cases() {
        assert_eq!(sentence_end(" by Friday."), Some(SentenceEnd::NeedsSpace));
        assert_eq!(sentence_end(" costs 3."), Some(SentenceEnd::NeedsSpace));
        assert_eq!(sentence_end("你好。"), Some(SentenceEnd::Closed));
        assert_eq!(sentence_end(" कल आऊँगा।"), Some(SentenceEnd::NeedsSpace));
        assert_eq!(sentence_end(" ..."), None);
        assert_eq!(sentence_end(" costs 3.5."), Some(SentenceEnd::NeedsSpace));
        assert_eq!(
            sentence_end(" on www.example.com."),
            Some(SentenceEnd::NeedsSpace)
        );
        assert_eq!(sentence_end(" at 9:30 a.m."), None);
        assert_eq!(sentence_end(" in the U.S."), None);
        assert_eq!(sentence_end(" (i.e."), None);
        assert_eq!(sentence_end(" or e.g."), None);
        assert_eq!(sentence_end(" see Plan B."), Some(SentenceEnd::NeedsSpace));
        assert_eq!(sentence_end(" it works."), Some(SentenceEnd::NeedsSpace));
        assert_eq!(sentence_end(" by Friday"), None);
    }

    #[test]
    fn window_continues_when_the_reader_slides_it() {
        let document: String = (0..400).map(|n| format!("w{n} ")).collect();
        let lookback = 200;
        let window = |end: usize| document[end - lookback..end].to_string();
        let mut kept = window(1200);
        for end in 1201..1400 {
            kept = continue_window(&kept, &window(end), lookback);
            assert_eq!(kept, document[1000..end], "{end}");
        }
    }

    #[test]
    fn window_never_joins_unrelated_text() {
        let lookback = 200;
        let first = "alpha ".repeat(60);
        for next in [
            "beta gamma ".repeat(30),
            "alpha ".repeat(10),
            String::new(),
            "alph".into(),
        ] {
            assert_eq!(continue_window(&first, &next, lookback), next);
        }
        assert_eq!(continue_window("", "hello", lookback), "hello");
        assert_eq!(continue_window("abc def", "def", 200), "def");
    }

    #[test]
    fn window_growth_is_bounded_and_keeps_the_tail_phase() {
        let lookback = 1200 * 16;
        let document: String = (0..16000).map(|n| format!("w{n} ")).collect();
        let mut kept = String::new();
        for end in (lookback..document.len()).step_by(97) {
            let next = document[end - lookback..end].to_string();
            kept = continue_window(&kept, &next, lookback);
            assert!(kept.len() <= lookback * 2);
            assert!(kept.ends_with(&next));
            assert_eq!((end - kept.len()) % lookback, 0, "{end}");
        }
    }

    #[test]
    fn tail_never_starts_inside_a_grapheme() {
        let text = "नमस्ते".repeat(40);
        for window in 1..30 {
            let tail = tail_chars(&text, window);
            assert!(
                !tail.starts_with(|c: char| ('ऀ'..='ः').contains(&c) || ('ा'..='्').contains(&c)),
                "{window}"
            );
        }
    }

    #[test]
    fn tail_window_of_one_keeps_the_last_grapheme() {
        assert_eq!(tail_chars("hello", 1), "o");
        assert_eq!(tail_chars("cafe\u{301}", 1), "e\u{301}");
    }

    #[test]
    fn tail_never_leaves_a_mark_whose_space_was_trimmed() {
        let text = format!("{}\u{2003}\u{fe0f}{}", "a".repeat(27), "b".repeat(32));
        assert_eq!(tail_chars(&text, 50), "b".repeat(32));
    }

    #[test]
    fn clean_completion_keeps_a_word_followed_by_space_when_truncated() {
        assert_eq!(
            clean_completion(" one two ", 6, true).as_deref(),
            Some(" one two")
        );
        assert_eq!(
            clean_completion(" one two", 6, true).as_deref(),
            Some(" one")
        );
    }
}
