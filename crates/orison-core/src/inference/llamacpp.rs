//! `LlamaCppBackend`: in-process inference via `llama-cpp-2`, with
//! memory-mapped model loading.
//!
//! Built alongside `OllamaBackend` rather than after it, per the handoff:
//! this is the only backend that could ever run on a phone (mobile cannot
//! spawn an Ollama sidecar), and retrofitting it later would mean a
//! redesign rather than an addition. It removes the hard Ollama dependency
//! and the "is the server running?" class of onboarding failure.
//!
//! **Verification note.** This implementation is built directly against the
//! `llama-cpp-2` API (model loading, chat templates, batching, grammar
//! sampling) and compiles under `--features llama-cpp`, but this sandbox has
//! no GGUF model file and no network path to fetch one, so it has not been
//! run end-to-end against real weights. The conformance suite's
//! `LlamaCppBackend` cases are gated behind the `ORISON_TEST_GGUF_MODEL`
//! environment variable for exactly that reason — verify on a machine with a
//! real model before relying on this in place of `OllamaBackend`.
//!
//! **Architecture note.** `llama-cpp-2`'s `LlamaContext` holds raw pointers
//! into `llama.cpp` state and is neither `Send` nor `Sync`, so it cannot
//! live behind `&self` in an `async fn` the way `OllamaBackend`'s `reqwest`
//! client can. Model and context are instead owned by one dedicated OS
//! thread; `LlamaCppBackend` itself holds only a channel to that thread.
//! This also naturally serialises access to the one loaded model, which is
//! the right default for a single local GPU/CPU.
//!
//! **Native tool calling is not implemented here.** `capabilities().tools`
//! reports `false`, and a request that carries `req.tools` is rejected with
//! `InferenceError::Unsupported` rather than silently ignored — the same
//! "no quiet degradation" principle as B-15.

use std::num::NonZeroU32;
use std::path::PathBuf;

use async_trait::async_trait;
use futures_core::stream::BoxStream;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use tokenizers::Tokenizer;
use tokio::sync::{mpsc, oneshot};

use super::backend::InferenceBackend;
use super::error::InferenceError;
use super::types::{
    Capabilities, ChatDelta, ChatMessage, ChatRequest, ChatResponse, DoneReason, HealthStatus,
    ModelHealth, Role,
};

const MAX_NEW_TOKENS: usize = 1024;

enum Job {
    Chat {
        req: ChatRequest,
        respond_to: oneshot::Sender<Result<ChatResponse, InferenceError>>,
    },
    ChatStream {
        req: ChatRequest,
        tx: mpsc::UnboundedSender<Result<ChatDelta, InferenceError>>,
    },
}

pub struct LlamaCppBackend {
    jobs: mpsc::UnboundedSender<Job>,
    tokenizer: Tokenizer,
    context_length: usize,
    capabilities: Capabilities,
    model_label: String,
    _worker: std::thread::JoinHandle<()>,
}

impl LlamaCppBackend {
    /// Load `model_path` (a GGUF file, memory-mapped by `llama.cpp` rather
    /// than read wholesale) and start the dedicated worker thread. `tokenizer`
    /// is a `tokenizers`-crate tokenizer matching the GGUF's vocabulary,
    /// used for `crate::prompt::budget` token counts — generation itself
    /// tokenizes through `llama.cpp`'s own vocab, which is the model's real
    /// tokenizer and the authority on what it actually sees.
    pub fn load(
        model_path: impl Into<PathBuf>,
        tokenizer: Tokenizer,
        context_length: u32,
        n_gpu_layers: u32,
    ) -> Result<Self, InferenceError> {
        let model_path = model_path.into();
        let model_label = model_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| model_path.to_string_lossy().into_owned());

        let (tx, rx) = mpsc::unbounded_channel::<Job>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

        let worker = std::thread::spawn(move || {
            worker_loop(model_path, context_length, n_gpu_layers, rx, ready_tx);
        });

        ready_rx
            .recv()
            .map_err(|_| InferenceError::Internal("worker thread exited before ready".into()))?
            .map_err(InferenceError::Internal)?;

        Ok(Self {
            jobs: tx,
            tokenizer,
            context_length: context_length as usize,
            capabilities: Capabilities {
                tools: false,
                structured_output: true,
                vision: false,
                streaming: true,
            },
            model_label,
            _worker: worker,
        })
    }
}

fn worker_loop(
    model_path: PathBuf,
    context_length: u32,
    n_gpu_layers: u32,
    mut rx: mpsc::UnboundedReceiver<Job>,
    ready_tx: std::sync::mpsc::Sender<Result<(), String>>,
) {
    let backend = match LlamaBackend::init() {
        Ok(b) => b,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("llama backend init failed: {e}")));
            return;
        }
    };

    // `with_use_mmap` defaults to true: the model is memory-mapped, not read
    // wholesale into the heap, which is the whole point of this backend
    // over shelling out to a server process.
    let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);
    let model = match LlamaModel::load_from_file(&backend, &model_path, &model_params) {
        Ok(m) => m,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("failed to load {model_path:?}: {e}")));
            return;
        }
    };

    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(context_length));
    let mut context = match model.new_context(&backend, ctx_params) {
        Ok(c) => c,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("failed to create context: {e}")));
            return;
        }
    };

    let chat_template = model.chat_template(None).unwrap_or_else(|_| {
        LlamaChatTemplate::new("chatml").expect("\"chatml\" is a valid template name")
    });

    let _ = ready_tx.send(Ok(()));

    while let Some(job) = rx.blocking_recv() {
        match job {
            Job::Chat { req, respond_to } => {
                let result = generate(&model, &mut context, &chat_template, req);
                let _ = respond_to.send(result);
            }
            Job::ChatStream { req, tx } => {
                generate_streaming(&model, &mut context, &chat_template, req, &tx);
            }
        }
    }
}

fn to_llama_messages(messages: &[ChatMessage]) -> Result<Vec<LlamaChatMessage>, InferenceError> {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            };
            LlamaChatMessage::new(role.to_string(), m.content.clone().unwrap_or_default())
                .map_err(|e| InferenceError::Internal(e.to_string()))
        })
        .collect()
}

/// Build the sampler chain for one request: grammar-constrained when a
/// `ResponseFormat` was requested (§2.4, via `super::grammar`), otherwise a
/// plain temperature/top-p chain.
fn build_sampler(model: &LlamaModel, req: &ChatRequest) -> Result<LlamaSampler, InferenceError> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(42);

    if let Some(format) = &req.response_format {
        let grammar = super::grammar::from_schema(&format.schema, "Response");
        let grammar_sampler = LlamaSampler::grammar(model, grammar.as_str(), &grammar.root_rule)
            .map_err(|e| InferenceError::Internal(format!("invalid grammar: {e:?}")))?;
        Ok(LlamaSampler::chain_simple([
            grammar_sampler,
            LlamaSampler::dist(seed),
        ]))
    } else {
        Ok(LlamaSampler::chain_simple([
            LlamaSampler::temp(req.sampling.temperature.max(0.01)),
            LlamaSampler::top_p(req.sampling.top_p, 1),
            LlamaSampler::dist(seed),
        ]))
    }
}

fn generate(
    model: &LlamaModel,
    ctx: &mut LlamaContext,
    chat_template: &LlamaChatTemplate,
    req: ChatRequest,
) -> Result<ChatResponse, InferenceError> {
    if !req.tools.is_empty() {
        return Err(InferenceError::Unsupported(
            "LlamaCppBackend does not implement native tool calling yet".into(),
        ));
    }

    let llama_messages = to_llama_messages(&req.messages)?;
    let prompt = model
        .apply_chat_template(chat_template, &llama_messages, true)
        .map_err(|e| InferenceError::Internal(e.to_string()))?;

    let tokens = model
        .str_to_token(&prompt, AddBos::Always)
        .map_err(|e| InferenceError::Internal(e.to_string()))?;
    if tokens.is_empty() {
        return Err(InferenceError::Internal(
            "prompt tokenized to zero tokens".into(),
        ));
    }

    let mut sampler = build_sampler(model, &req)?;

    let mut batch = LlamaBatch::new(tokens.len(), 1);
    let last_idx = tokens.len() - 1;
    for (i, token) in tokens.iter().enumerate() {
        batch
            .add(*token, i as i32, &[0], i == last_idx)
            .map_err(|e| InferenceError::Internal(e.to_string()))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| InferenceError::Internal(e.to_string()))?;

    let mut n_past = tokens.len() as i32;
    let available_for_response = (ctx.n_ctx() as usize).saturating_sub(tokens.len());
    let max_new_tokens = available_for_response.min(MAX_NEW_TOKENS);

    let mut output = String::new();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut completion_tokens = 0usize;
    let mut done_reason = DoneReason::Length;

    for _ in 0..max_new_tokens {
        let next = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(next);

        if model.is_eog_token(next) {
            done_reason = DoneReason::Stop;
            break;
        }

        let piece = model
            .token_to_piece(next, &mut decoder, true, None)
            .map_err(|e| InferenceError::Internal(e.to_string()))?;
        output.push_str(&piece);
        completion_tokens += 1;

        batch.clear();
        batch
            .add(next, n_past, &[0], true)
            .map_err(|e| InferenceError::Internal(e.to_string()))?;
        ctx.decode(&mut batch)
            .map_err(|e| InferenceError::Internal(e.to_string()))?;
        n_past += 1;
    }

    Ok(ChatResponse {
        message: ChatMessage::assistant(output),
        done_reason,
        prompt_tokens: tokens.len(),
        completion_tokens,
    })
}

fn generate_streaming(
    model: &LlamaModel,
    ctx: &mut LlamaContext,
    chat_template: &LlamaChatTemplate,
    req: ChatRequest,
    tx: &mpsc::UnboundedSender<Result<ChatDelta, InferenceError>>,
) {
    // A from-scratch token-by-token loop identical to `generate`, except
    // each piece is sent as its own delta instead of being accumulated.
    // Duplicated rather than built on top of `generate` because streaming
    // needs to emit before the final token is known, not after.
    if !req.tools.is_empty() {
        let _ = tx.send(Err(InferenceError::Unsupported(
            "LlamaCppBackend does not implement native tool calling yet".into(),
        )));
        return;
    }

    let llama_messages = match to_llama_messages(&req.messages) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(Err(e));
            return;
        }
    };
    let prompt = match model.apply_chat_template(chat_template, &llama_messages, true) {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(Err(InferenceError::Internal(e.to_string())));
            return;
        }
    };
    let tokens = match model.str_to_token(&prompt, AddBos::Always) {
        Ok(t) if !t.is_empty() => t,
        Ok(_) => {
            let _ = tx.send(Err(InferenceError::Internal(
                "prompt tokenized to zero tokens".into(),
            )));
            return;
        }
        Err(e) => {
            let _ = tx.send(Err(InferenceError::Internal(e.to_string())));
            return;
        }
    };

    let mut sampler = match build_sampler(model, &req) {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(Err(e));
            return;
        }
    };

    let mut batch = LlamaBatch::new(tokens.len(), 1);
    let last_idx = tokens.len() - 1;
    for (i, token) in tokens.iter().enumerate() {
        if let Err(e) = batch.add(*token, i as i32, &[0], i == last_idx) {
            let _ = tx.send(Err(InferenceError::Internal(e.to_string())));
            return;
        }
    }
    if let Err(e) = ctx.decode(&mut batch) {
        let _ = tx.send(Err(InferenceError::Internal(e.to_string())));
        return;
    }

    let mut n_past = tokens.len() as i32;
    let available_for_response = (ctx.n_ctx() as usize).saturating_sub(tokens.len());
    let max_new_tokens = available_for_response.min(MAX_NEW_TOKENS);
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    for _ in 0..max_new_tokens {
        let next = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(next);

        if model.is_eog_token(next) {
            let _ = tx.send(Ok(ChatDelta {
                content: None,
                tool_calls: None,
                done: true,
                // In-process decoding keeps its own KV cache and never
                // re-evaluates a shared prefix, so there is no separate
                // "evaluated" count to report.
                evaluated_prompt_tokens: None,
            }));
            return;
        }

        match model.token_to_piece(next, &mut decoder, true, None) {
            Ok(piece) => {
                if tx
                    .send(Ok(ChatDelta {
                        content: Some(piece),
                        tool_calls: None,
                        done: false,
                        evaluated_prompt_tokens: None,
                    }))
                    .is_err()
                {
                    return; // receiver dropped; caller stopped listening.
                }
            }
            Err(e) => {
                let _ = tx.send(Err(InferenceError::Internal(e.to_string())));
                return;
            }
        }

        batch.clear();
        if let Err(e) = batch.add(next, n_past, &[0], true) {
            let _ = tx.send(Err(InferenceError::Internal(e.to_string())));
            return;
        }
        if let Err(e) = ctx.decode(&mut batch) {
            let _ = tx.send(Err(InferenceError::Internal(e.to_string())));
            return;
        }
        n_past += 1;
    }

    let _ = tx.send(Ok(ChatDelta {
        content: None,
        tool_calls: None,
        done: true,
        evaluated_prompt_tokens: None,
    }));
}

#[async_trait]
impl InferenceBackend for LlamaCppBackend {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, InferenceError> {
        let (respond_to, rx) = oneshot::channel();
        self.jobs
            .send(Job::Chat { req, respond_to })
            .map_err(|_| InferenceError::Internal("llama.cpp worker thread is gone".into()))?;
        rx.await.map_err(|_| {
            InferenceError::Internal("llama.cpp worker thread dropped the response".into())
        })?
    }

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatDelta, InferenceError>>, InferenceError> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.jobs
            .send(Job::ChatStream { req, tx })
            .map_err(|_| InferenceError::Internal("llama.cpp worker thread is gone".into()))?;

        let stream = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });
        Ok(Box::pin(stream))
    }

    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, InferenceError> {
        // Embedding requires a second context built with pooling enabled,
        // which this backend does not yet set up. Reporting this as a typed
        // error is the honest choice: a chat-tuned GGUF fed through the
        // wrong pooling mode would return numbers that merely look like
        // embeddings, which is a worse failure than refusing outright.
        Err(InferenceError::Unsupported(
            "LlamaCppBackend does not implement embed() yet".into(),
        ))
    }

    fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    fn context_length(&self) -> usize {
        self.context_length
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    async fn health(&self) -> Result<ModelHealth, InferenceError> {
        // The model is loaded (mmap'd) synchronously in `load()`; by the
        // time a `LlamaCppBackend` value exists, the worker thread has
        // already confirmed it loaded successfully. There is no separate
        // server to be unreachable, so health here is really "is the worker
        // thread still alive" (B-15's failure mode for this backend would be
        // the thread having panicked, not a dead HTTP endpoint).
        if self.jobs.is_closed() {
            return Ok(ModelHealth {
                model: self.model_label.clone(),
                status: HealthStatus::Unreachable {
                    detail: "worker thread has exited".into(),
                },
                context_length: None,
            });
        }
        Ok(ModelHealth {
            model: self.model_label.clone(),
            status: HealthStatus::Available,
            context_length: Some(self.context_length),
        })
    }
}
