use std::num::NonZeroU32;
use std::path::Path;

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::token::LlamaToken;
use pith_core::{
    Completer, Completion, CompletionRequest, LINE_BREAKS, SentenceEnd, StopReason,
    average_logprob, exceeds_words, sentence_end,
};

use crate::sampling::{consistent_with_prefix, log_sum_exp, reusable_prefix, valid_utf8_prefix};
use crate::slots::Slots;

const BATCH: usize = 64;
const FIELD_SLOTS: usize = 2;
const DEFAULT_FIELD: u64 = 0;

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
        let mut slots = Slots::new(FIELD_SLOTS);
        slots.acquire(DEFAULT_FIELD, || self.slot(threads, context_tokens))?;
        Ok(LlamaSession {
            handle: self,
            threads,
            context_tokens,
            slots,
            batch: LlamaBatch::new(BATCH, 1),
        })
    }

    fn slot(&self, threads: usize, context_tokens: u32) -> Result<Slot<'_>, String> {
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
        Ok(Slot {
            ctx,
            cached: Vec::new(),
        })
    }

    fn tokenize(&self, prompt: &str) -> Vec<LlamaToken> {
        let vocab = self.model.vocab();
        let mut tokens = vocab.tokenize(prompt.as_bytes(), true, false);
        if tokens.is_empty() {
            tokens = vocab.tokenize(b"\n", false, false);
        }
        tokens
    }

    fn pick(&self, logits: &[f32], remaining: &[u8]) -> Option<(LlamaToken, f32)> {
        let lse = log_sum_exp(logits);
        let vocab = self.model.vocab();
        let mut best: Option<(usize, f32)> = None;
        for (id, logit) in logits.iter().enumerate() {
            if self.blocked.get(id).copied().unwrap_or(true) {
                continue;
            }
            if !remaining.is_empty() {
                let piece = self.pieces.get(id).map(Vec::as_slice).unwrap_or_default();
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

struct Slot<'a> {
    ctx: LlamaContext<'a>,
    cached: Vec<LlamaToken>,
}

impl Slot<'_> {
    fn evaluate(
        &mut self,
        batch: &mut LlamaBatch<'static>,
        tokens: &[LlamaToken],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<usize>, String> {
        let mut keep = reusable_prefix(&self.cached, tokens);
        let trimmed = self
            .ctx
            .clear_kv_cache_seq(Some(0), Some(keep as u32), None)
            .map_err(|e| e.to_string())?;
        if !trimmed {
            self.ctx.clear_kv_cache();
            keep = 0;
        }
        self.cached.truncate(keep);
        let evaluated = tokens.len() - keep;
        for chunk_start in (keep..tokens.len()).step_by(BATCH) {
            if cancelled() {
                return Ok(None);
            }
            let chunk_end = (chunk_start + BATCH).min(tokens.len());
            batch.clear();
            for (position, token) in tokens.iter().enumerate().take(chunk_end).skip(chunk_start) {
                let last = position + 1 == tokens.len();
                batch
                    .add(*token, position as i32, &[0], last)
                    .map_err(|e| e.to_string())?;
            }
            self.ctx.decode(batch).map_err(|e| e.to_string())?;
            self.cached
                .extend_from_slice(&tokens[chunk_start..chunk_end]);
        }
        Ok(Some(evaluated))
    }

    fn append(&mut self, batch: &mut LlamaBatch<'static>, token: LlamaToken) -> Result<(), String> {
        batch.clear();
        batch
            .add(token, self.cached.len() as i32, &[0], true)
            .map_err(|e| e.to_string())?;
        self.ctx.decode(batch).map_err(|e| e.to_string())?;
        self.cached.push(token);
        Ok(())
    }
}

pub struct LlamaSession<'a> {
    handle: &'a LlamaModelHandle,
    threads: usize,
    context_tokens: u32,
    slots: Slots<Slot<'a>>,
    batch: LlamaBatch<'static>,
}

impl LlamaSession<'_> {
    pub fn count_tokens(&self, text: &str) -> usize {
        self.handle.tokenize(text).len()
    }
}

impl Completer for LlamaSession<'_> {
    fn complete(
        &mut self,
        request: &CompletionRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Completion, String> {
        let Self {
            handle,
            threads,
            context_tokens,
            slots,
            batch,
        } = self;
        let slot = slots.acquire(request.field_id, || handle.slot(*threads, *context_tokens))?;
        let mut tokens = handle.tokenize(&request.prompt);
        let budget = (slot.ctx.n_ctx() as usize)
            .saturating_sub(request.max_tokens + 16)
            .max(1);
        if tokens.len() > budget {
            let step = (budget / 2).max(1);
            let excess = tokens.len() - budget;
            tokens.drain(..excess.div_ceil(step) * step);
        }
        let Some(evaluated) = slot.evaluate(batch, &tokens, cancelled)? else {
            return Ok(Completion::cancelled(0));
        };
        let mut remaining: Vec<u8> = request.forced_prefix.as_bytes().to_vec();
        let mut output: Vec<u8> = Vec::new();
        let mut logprob_sum = 0.0f32;
        let mut scored = 0usize;
        let mut produced = 0usize;
        let mut logits_index = batch.n_tokens() - 1;
        let mut awaiting_space = false;
        let stop = loop {
            if cancelled() {
                return Ok(Completion::cancelled(evaluated));
            }
            if produced >= request.max_tokens + 8 || scored >= request.max_tokens {
                break if awaiting_space {
                    StopReason::Sentence
                } else {
                    StopReason::MaxTokens
                };
            }
            let logits = slot.ctx.get_logits_ith(logits_index);
            let Some((token, logprob)) = handle.pick(logits, &remaining) else {
                break StopReason::EndOfText;
            };
            if handle.model.vocab().is_eog(token) {
                break StopReason::EndOfText;
            }
            let piece = handle.pieces[token.0 as usize].clone();
            if awaiting_space && piece.first().is_some_and(u8::is_ascii_whitespace) {
                break StopReason::Sentence;
            }
            awaiting_space = false;
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
            if let Some(newline) = text.find(LINE_BREAKS) {
                output.truncate(newline);
                break StopReason::Newline;
            }
            match sentence_end(text) {
                Some(SentenceEnd::Closed) => break StopReason::Sentence,
                Some(SentenceEnd::NeedsSpace) => awaiting_space = true,
                None => {}
            }
            if exceeds_words(text, request.max_words) {
                break StopReason::WordLimit;
            }
            slot.append(batch, token)?;
            logits_index = 0;
        };
        let text = valid_utf8_prefix(&output).to_string();
        Ok(Completion {
            text,
            mean_logprob: average_logprob(logprob_sum, scored),
            tokens: produced,
            prompt_tokens_evaluated: evaluated,
            stop,
        })
    }
}
