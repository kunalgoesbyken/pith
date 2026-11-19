use crate::suffix::fit;
use crate::{
    Completion, Config, FieldSnapshot, StopReason, clean_completion, split_partial, tail_chars,
};

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionRequest {
    pub field_id: u64,
    pub prompt: String,
    pub forced_prefix: String,
    pub max_tokens: usize,
    pub max_words: usize,
    pub min_token_logprob: f32,
}

pub fn build(field: &FieldSnapshot, config: &Config) -> CompletionRequest {
    let context = tail_chars(&field.before, config.context_chars);
    let (head, partial) = split_partial(context);
    let title = field.window_title.trim();
    let prompt = if title.is_empty() {
        head.to_string()
    } else {
        format!("{title}\n\n{head}")
    };
    CompletionRequest {
        field_id: field.field_id,
        prompt,
        forced_prefix: partial.to_string(),
        max_tokens: config.max_tokens,
        max_words: config.max_words,
        min_token_logprob: config.min_token_logprob,
    }
}

fn confident_enough(mean_logprob: Option<f32>, config: &Config) -> bool {
    let Some(mean) = mean_logprob else {
        return true;
    };
    mean >= config.min_mean_logprob
}

pub fn suggestion(
    completion: &Completion,
    before: &str,
    after: &str,
    config: &Config,
) -> Option<String> {
    if completion.stop == StopReason::Cancelled
        || !confident_enough(completion.mean_logprob, config)
    {
        return None;
    }
    let truncated = matches!(
        completion.stop,
        StopReason::MaxTokens | StopReason::LowConfidence
    );
    let text = clean_completion(&completion.text, config.max_words, truncated)?;
    let text = if before.ends_with(char::is_whitespace) {
        text.trim_start().to_string()
    } else {
        text
    };
    fit(&text, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_word_or_trailing_space_becomes_the_forced_prefix() {
        let table = [
            (
                "Re: contract",
                "Please sign the contr",
                "Re: contract\n\nPlease sign the",
                " contr",
            ),
            ("", "Hi there ", "Hi there", " "),
        ];
        for row in table {
            let (window_title, before, prompt, forced_prefix) = row;
            let field = FieldSnapshot {
                window_title: window_title.into(),
                before: before.into(),
                ..Default::default()
            };
            let request = build(&field, &Config::default());
            assert_eq!(
                (request.prompt.as_str(), request.forced_prefix.as_str()),
                (prompt, forced_prefix),
                "{row:?}"
            );
        }
    }
}
