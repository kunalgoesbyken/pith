mod common;

use common::{Rng, is_boundary, text};
use nib_core::{
    Config, FieldSnapshot, SentenceEnd, build, clean_completion, next_word, sentence_end,
    split_partial, tail_chars,
};
use unicode_segmentation::UnicodeSegmentation;

const CASES: usize = 4000;

fn is_word_like(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

fn word_count(text: &str) -> usize {
    text.split_word_bounds().filter(|s| is_word_like(s)).count()
}

#[test]
fn tail_chars_is_a_clean_bounded_suffix() {
    let mut rng = Rng::new(1);
    for _ in 0..CASES {
        let input = text(&mut rng, 60);
        let window = rng.below(40);
        let tail = tail_chars(&input, window);
        let start = input.len() - tail.len();
        assert!(input.ends_with(tail), "{input:?} {window}");
        assert!(is_boundary(&input, start), "{input:?} {window} {tail:?}");
        if window > 0 {
            assert!(
                tail.graphemes(true).count() <= window,
                "{input:?} {window} {tail:?}"
            );
        }
        let ends_in_text = input
            .graphemes(true)
            .next_back()
            .is_some_and(|g| !g.starts_with(char::is_whitespace));
        if window > 0 && ends_in_text {
            assert!(!tail.is_empty(), "{input:?} {window}");
        }
    }
}

#[test]
fn tail_chars_keeps_the_last_window_for_every_window_size() {
    for window in 1..8 {
        let input = "abcdefghij";
        let tail = tail_chars(input, window);
        assert!(!tail.is_empty(), "{window}");
        assert!(input.ends_with(tail), "{window}");
    }
}

#[test]
fn split_partial_reassembles_on_clean_boundaries() {
    let mut rng = Rng::new(2);
    for _ in 0..CASES {
        let input = text(&mut rng, 30);
        let (head, partial) = split_partial(&input);
        assert_eq!(format!("{head}{partial}"), input);
        assert!(is_boundary(&input, head.len()), "{input:?} {head:?}");
        assert!(
            !head.ends_with(|c: char| c.is_whitespace() && c != '\n' && c != '\r'),
            "{input:?} {head:?}"
        );
        let word =
            partial.trim_start_matches(|c: char| c.is_whitespace() && c != '\n' && c != '\r');
        assert!(
            word.split_word_bounds().count() <= 1,
            "{input:?} {partial:?}"
        );
    }
}

#[test]
fn next_word_consumes_whole_text_in_clean_steps() {
    let mut rng = Rng::new(3);
    for _ in 0..CASES {
        let input = text(&mut rng, 30);
        let mut rest = input.as_str();
        let mut rebuilt = String::new();
        let mut steps = 0;
        while !rest.is_empty() {
            let word = next_word(rest);
            assert!(!word.is_empty(), "{input:?} {rest:?}");
            assert!(rest.starts_with(word));
            assert!(
                is_boundary(&input, input.len() - rest.len() + word.len()),
                "{input:?} {word:?}"
            );
            assert!(word_count(word) <= 1, "{input:?} {word:?}");
            rebuilt.push_str(word);
            rest = &rest[word.len()..];
            steps += 1;
            assert!(steps <= input.len(), "no progress {input:?}");
        }
        assert_eq!(rebuilt, input);
    }
}

#[test]
fn clean_completion_returns_a_clean_single_line_prefix() {
    let mut rng = Rng::new(4);
    for _ in 0..CASES {
        let raw = text(&mut rng, 30);
        let max_words = 1 + rng.below(8);
        let truncated = rng.chance(50);
        let Some(cleaned) = clean_completion(&raw, max_words, truncated) else {
            continue;
        };
        assert!(raw.starts_with(&cleaned), "{raw:?} {cleaned:?}");
        assert!(is_boundary(&raw, cleaned.len()), "{raw:?} {cleaned:?}");
        assert!(!cleaned.contains(['\n', '\r']), "{raw:?} {cleaned:?}");
        assert_eq!(cleaned.trim_end(), cleaned, "{raw:?}");
        assert!(
            word_count(&cleaned) >= 1 && word_count(&cleaned) <= max_words,
            "{raw:?} {cleaned:?}"
        );
        if truncated {
            let full = clean_completion(&raw, max_words, false).unwrap_or_default();
            assert!(full.starts_with(&cleaned), "{raw:?} {cleaned:?} {full:?}");
        }
    }
}

#[test]
fn clean_completion_only_drops_a_word_that_may_be_cut() {
    assert_eq!(
        clean_completion(" one two ", 6, true).as_deref(),
        Some(" one two")
    );
    assert_eq!(
        clean_completion(" one two", 6, true).as_deref(),
        Some(" one")
    );
}

#[test]
fn sentence_end_ignores_trailing_whitespace_and_matches_the_last_character() {
    let mut rng = Rng::new(5);
    for _ in 0..CASES {
        let input = text(&mut rng, 20);
        let padded = format!("{input} \t\n");
        assert_eq!(sentence_end(&input), sentence_end(&padded), "{input:?}");
        let expected = match input.trim_end().chars().next_back() {
            _ if !input.chars().any(char::is_alphanumeric) => None,
            Some('。' | '！' | '？') => Some(SentenceEnd::Closed),
            Some('.' | '!' | '?' | '।' | '॥' | '؟' | '…') => Some(SentenceEnd::NeedsSpace),
            _ => None,
        };
        assert_eq!(sentence_end(&input), expected, "{input:?}");
    }
}

#[test]
fn build_prompt_is_clean_for_any_text_and_window() {
    let mut rng = Rng::new(6);
    for _ in 0..CASES {
        let field = FieldSnapshot {
            before: text(&mut rng, 40),
            window_title: text(&mut rng, 3),
            ..Default::default()
        };
        let config = Config {
            context_chars: 1 + rng.below(40),
            ..Config::default()
        };
        let request = build(&field, &config);
        let context = format!("{}{}", request.prompt, request.forced_prefix);
        assert!(context.contains(&request.forced_prefix));
        assert!(
            field.before.ends_with(request.forced_prefix.as_str()),
            "{field:?}"
        );
        if field
            .before
            .graphemes(true)
            .next_back()
            .is_some_and(|g| !g.starts_with(char::is_whitespace))
        {
            assert!(
                !request.forced_prefix.is_empty() || !request.prompt.is_empty(),
                "{field:?}"
            );
        }
    }
}
