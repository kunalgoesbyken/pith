use crate::budget::{estimate_tokens, room};
use crate::style::{Primer, STYLE_TOKENS};
use crate::suffix::fit;
use crate::{
    After, Completion, Config, FieldSnapshot, StopReason, clean_completion, split_partial,
    tail_chars,
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

fn preface<'a>(title: &'a str, cues: Vec<&'a str>) -> String {
    std::iter::once(title)
        .filter(|title| !title.is_empty())
        .chain(cues)
        .collect::<Vec<_>>()
        .join("\n")
}

fn cues<'a>(field: &'a FieldSnapshot, config: &Config, used_tokens: usize) -> Vec<&'a str> {
    if !config.uses_nearby() {
        return Vec::new();
    }
    let allowed = room(config, used_tokens).min(config.nearby_tokens as usize);
    field.nearby.fit(allowed)
}

fn examples(primer: &Primer, config: &Config, used_tokens: usize) -> String {
    if !config.uses_style() {
        return String::new();
    }
    let allowed = room(config, used_tokens).min(STYLE_TOKENS);
    primer.fit(allowed).join(
        "
",
    )
}

pub fn build(field: &FieldSnapshot, config: &Config, primer: &Primer) -> CompletionRequest {
    let context = tail_chars(&field.before, config.context_chars);
    let (head, partial) = split_partial(context);
    let title = field.window_title.trim();
    let used_tokens = estimate_tokens(title) + estimate_tokens(head);
    let preface = preface(title, cues(field, config, used_tokens));
    let used_tokens = estimate_tokens(&preface) + estimate_tokens(head);
    let lead = [examples(primer, config, used_tokens), preface]
        .into_iter()
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join(
            "

",
        );
    let prompt = if lead.is_empty() {
        head.to_string()
    } else {
        format!(
            "{lead}

{head}"
        )
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
    after: &After,
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
            let request = build(&field, &Config::default(), &Primer::default());
            assert_eq!(
                (request.prompt.as_str(), request.forced_prefix.as_str()),
                (prompt, forced_prefix),
                "{row:?}"
            );
        }
    }
}
