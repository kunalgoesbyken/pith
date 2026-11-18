use crate::{
    Completion, Config, FieldSnapshot, StopReason, clean_completion, split_partial, tail_chars,
};

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionRequest {
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
        prompt,
        forced_prefix: partial.to_string(),
        max_tokens: config.max_tokens,
        max_words: config.max_words,
        min_token_logprob: config.min_token_logprob,
    }
}

pub fn suggestion(completion: &Completion, before: &str, config: &Config) -> Option<String> {
    if completion.stop == StopReason::Cancelled
        || completion.mean_logprob.is_nan()
        || completion.mean_logprob < config.min_mean_logprob
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
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_partial_word_into_forced_prefix() {
        let field = FieldSnapshot {
            window_title: "Re: contract".into(),
            before: "Please sign the contr".into(),
            ..Default::default()
        };
        let request = build(&field, &Config::default());
        assert_eq!(request.prompt, "Re: contract\n\nPlease sign the");
        assert_eq!(request.forced_prefix, " contr");
    }

    #[test]
    fn trailing_space_becomes_forced_prefix() {
        let field = FieldSnapshot {
            before: "Hi there ".into(),
            ..Default::default()
        };
        let request = build(&field, &Config::default());
        assert_eq!(request.prompt, "Hi there");
        assert_eq!(request.forced_prefix, " ");
    }
}
