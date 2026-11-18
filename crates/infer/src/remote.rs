use std::time::Duration;

use nib_core::{Completer, Completion, CompletionRequest, StopReason};
use serde::Deserialize;

pub struct RemoteCompleter {
    url: String,
    model: Option<String>,
    agent: ureq::Agent,
}

#[derive(Deserialize)]
struct Response {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    text: String,
    finish_reason: Option<String>,
    logprobs: Option<Logprobs>,
}

#[derive(Deserialize)]
struct Logprobs {
    token_logprobs: Option<Vec<Option<f32>>>,
}

impl RemoteCompleter {
    pub fn new(url: &str, model: Option<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(3)))
            .build()
            .into();
        Self {
            url: url.trim_end_matches('/').to_string(),
            model,
            agent,
        }
    }
}

impl Completer for RemoteCompleter {
    fn complete(
        &mut self,
        request: &CompletionRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Completion, String> {
        let mut body = serde_json::json!({
            "prompt": format!("{}{}", request.prompt, request.forced_prefix),
            "max_tokens": request.max_tokens,
            "temperature": 0,
            "stop": ["\n"],
            "logprobs": 1,
        });
        if let Some(model) = &self.model {
            body["model"] = serde_json::Value::String(model.clone());
        }
        let response: Response = self
            .agent
            .post(&format!("{}/v1/completions", self.url))
            .send_json(&body)
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_json()
            .map_err(|e| e.to_string())?;
        if cancelled() {
            return Ok(Completion::cancelled(0));
        }
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or("empty response")?;
        let logprobs: Vec<f32> = choice
            .logprobs
            .and_then(|l| l.token_logprobs)
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .collect();
        let mean_logprob = if logprobs.is_empty() {
            0.0
        } else {
            logprobs.iter().sum::<f32>() / logprobs.len() as f32
        };
        let stop = match choice.finish_reason.as_deref() {
            Some("length") => StopReason::MaxTokens,
            Some("stop") => StopReason::Newline,
            _ => StopReason::EndOfText,
        };
        Ok(Completion {
            text: choice.text,
            mean_logprob,
            tokens: logprobs.len(),
            prompt_tokens_evaluated: 0,
            stop,
        })
    }
}
