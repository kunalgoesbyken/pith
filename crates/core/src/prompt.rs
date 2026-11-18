use crate::{Config, FieldSnapshot, split_partial, tail_chars};

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionRequest {
    pub prompt: String,
    pub forced_prefix: String,
    pub max_tokens: usize,
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
        min_token_logprob: config.min_token_logprob,
    }
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
