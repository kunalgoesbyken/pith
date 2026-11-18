use std::num::NonZeroU32;
use std::path::Path;

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::token::LlamaToken;
use nib_core::{Completer, Completion, CompletionRequest, StopReason};

use crate::sampling::{
    SENTENCE_ENDS, common_prefix_len, consistent_with_prefix, log_sum_exp, valid_utf8_prefix,
};

const BATCH: usize = 64;

pub struct LlamaModelHandle {
    model: LlamaModel,
    pieces: Vec<Vec<u8>>,
    blocked: Vec<bool>,
    backend: LlamaBackend,
}

impl LlamaModelHandle {
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut backend = LlamaBackend::init().map_err(|e| e.to_string())?;
        backend.void_logs();
        let model = LlamaModel::load_from_file(
            &backend,
            path,
            &LlamaModelParams::default().with_use_mmap(false),
        )
        .map_err(|e| e.to_string())?;
        let vocab = model.vocab();
        let mut pieces = Vec::new();
        let mut blocked = Vec::new();
        for token in vocab.tokens() {
            pieces.push(vocab.token_to_piece(token, false, None));
            blocked.push(vocab.is_control(token) && !vocab.is_eog(token));
        }
        Ok(Self {
            backend,
            model,
            pieces,
            blocked,
        })
    }

    pub fn session(&self, threads: usize, context_tokens: u32) -> Result<LlamaSession<'_>, String> {
        let threads = i32::try_from(threads.max(1)).unwrap_or(1);
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(context_tokens.max(256)))
            .with_n_batch(BATCH as u32)
            .with_n_ubatch(BATCH as u32)
            .with_type_k(KvCacheType::Q8_0)
            .with_type_v(KvCacheType::Q8_0)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let ctx = self
            .model
            .new_context(&self.backend, params)
            .map_err(|e| e.to_string())?;
        Ok(LlamaSession {
            handle: self,
            ctx,
            cached: Vec::new(),
            batch: LlamaBatch::new(BATCH, 1),
        })
    }
}

pub struct LlamaSession<'a> {
    handle: &'a LlamaModelHandle,
    ctx: LlamaContext<'a>,
    cached: Vec<LlamaToken>,
    batch: LlamaBatch<'static>,
}

impl LlamaSession<'_> {
    fn tokenize(&self, prompt: &str) -> Vec<LlamaToken> {
        let vocab = self.handle.model.vocab();
        let mut tokens = vocab.tokenize(prompt.as_bytes(), true, false);
        if tokens.is_empty() {
            tokens = vocab.tokenize(b"\n", false, false);
        }
        tokens
    }

    fn evaluate_prompt(
        &mut self,
        tokens: &[LlamaToken],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<usize>, String> {
        let mut keep = common_prefix_len(&self.cached, tokens);
        if keep == tokens.len() {
            keep -= 1;
        }
        self.ctx
            .clear_kv_cache_seq(Some(0), Some(keep as u32), None)
            .map_err(|e| e.to_string())?;
        self.cached.truncate(keep);
        let evaluated = tokens.len() - keep;
        for chunk_start in (keep..tokens.len()).step_by(BATCH) {
            if cancelled() {
                return Ok(None);
            }
            let chunk_end = (chunk_start + BATCH).min(tokens.len());
            self.batch.clear();
            for (position, token) in tokens.iter().enumerate().take(chunk_end).skip(chunk_start) {
                let last = position + 1 == tokens.len();
                self.batch
                    .add(*token, position as i32, &[0], last)
                    .map_err(|e| e.to_string())?;
            }
            self.ctx
                .decode(&mut self.batch)
                .map_err(|e| e.to_string())?;
            self.cached
                .extend_from_slice(&tokens[chunk_start..chunk_end]);
        }
        Ok(Some(evaluated))
    }

    fn pick(&self, logits: &[f32], remaining: &[u8]) -> Option<(LlamaToken, f32)> {
        let lse = log_sum_exp(logits);
        let vocab = self.handle.model.vocab();
        let mut best: Option<(usize, f32)> = None;
        for (id, logit) in logits.iter().enumerate() {
            if self.handle.blocked.get(id).copied().unwrap_or(true) {
                continue;
            }
            if !remaining.is_empty() {
                let piece = self
                    .handle
                    .pieces
                    .get(id)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                if !consistent_with_prefix(piece, remaining) || vocab.is_eog(LlamaToken(id as i32))
                {
                    continue;
                }
            }
            if best.is_none_or(|(_, b)| *logit > b) {
                best = Some((id, *logit));
            }
        }
        best.map(|(id, logit)| (LlamaToken(id as i32), logit - lse))
    }
}

impl Completer for LlamaSession<'_> {
    fn complete(
        &mut self,
        request: &CompletionRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Completion, String> {
        let mut tokens = self.tokenize(&request.prompt);
        let budget = (self.ctx.n_ctx() as usize)
            .saturating_sub(request.max_tokens + 16)
            .max(1);
        if tokens.len() > budget {
            tokens.drain(..tokens.len() - budget);
        }
        let cancelled_result = |evaluated| Completion {
            text: String::new(),
            mean_logprob: 0.0,
            tokens: 0,
            prompt_tokens_evaluated: evaluated,
            stop: StopReason::Cancelled,
        };
        let Some(evaluated) = self.evaluate_prompt(&tokens, cancelled)? else {
            return Ok(cancelled_result(0));
        };
        let mut remaining: Vec<u8> = request.forced_prefix.as_bytes().to_vec();
        let mut output: Vec<u8> = Vec::new();
        let mut logprob_sum = 0.0f32;
        let mut scored = 0usize;
        let mut produced = 0usize;
        let mut logits_index = self.batch.n_tokens() - 1;
        let stop = loop {
            if cancelled() {
                return Ok(cancelled_result(evaluated));
            }
            if produced >= request.max_tokens + 8 || (scored >= request.max_tokens) {
                break StopReason::MaxTokens;
            }
            let logits = self.ctx.get_logits_ith(logits_index);
            let Some((token, logprob)) = self.pick(logits, &remaining) else {
                break StopReason::EndOfText;
            };
            if self.handle.model.vocab().is_eog(token) {
                break StopReason::EndOfText;
            }
            let piece = self.handle.pieces[token.0 as usize].clone();
            let forced = !remaining.is_empty();
            if forced {
                if piece.len() >= remaining.len() {
                    output.extend_from_slice(&piece[remaining.len()..]);
                    remaining.clear();
                } else {
                    remaining.drain(..piece.len());
                }
            } else {
                if scored > 0 && logprob < request.min_token_logprob {
                    break StopReason::LowConfidence;
                }
                output.extend_from_slice(&piece);
            }
            if !forced || remaining.is_empty() && !output.is_empty() {
                logprob_sum += logprob;
                scored += 1;
            }
            produced += 1;
            let text = valid_utf8_prefix(&output);
            if let Some(newline) = text.find(['\n', '\r']) {
                output.truncate(newline);
                break StopReason::Newline;
            }
            if text.trim_end().ends_with(SENTENCE_ENDS) && text.chars().any(char::is_alphanumeric) {
                break StopReason::Sentence;
            }
            self.batch.clear();
            self.batch
                .add(token, self.cached.len() as i32, &[0], true)
                .map_err(|e| e.to_string())?;
            self.ctx
                .decode(&mut self.batch)
                .map_err(|e| e.to_string())?;
            self.cached.push(token);
            logits_index = 0;
        };
        let text = valid_utf8_prefix(&output).to_string();
        let mean_logprob = if scored == 0 {
            f32::NEG_INFINITY
        } else {
            logprob_sum / scored as f32
        };
        Ok(Completion {
            text,
            mean_logprob,
            tokens: produced,
            prompt_tokens_evaluated: evaluated,
            stop,
        })
    }
}
