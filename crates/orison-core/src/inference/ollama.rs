//! `OllamaBackend`: the desktop-default [`InferenceBackend`], talking to a
//! local Ollama server over `/api/chat`.
//!
//! Every current call in the Godot build uses `/api/generate` with a single
//! concatenated prompt string (B-2). That bypasses the model's own chat
//! template: an instruction-tuned model receives its system/user/assistant
//! turns smashed into one blob, in a format it was never tuned on. This
//! backend uses `/api/chat` exclusively, which is not a style preference —
//! it is the reason this phase exists.

use async_trait::async_trait;
use futures_core::stream::BoxStream;
use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;

use super::backend::InferenceBackend;
use super::error::InferenceError;
use super::types::{
    Capabilities, ChatDelta, ChatMessage, ChatRequest, ChatResponse, DoneReason, HealthStatus,
    ModelHealth, Role, ToolCall,
};

#[derive(Debug)]
pub struct OllamaBackend {
    client: reqwest::Client,
    base_url: String,
    model: String,
    tokenizer: Tokenizer,
    context_length: usize,
    capabilities: Capabilities,
}

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OllamaTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<serde_json::Value>,
    options: OllamaOptions,
    keep_alive: super::types::KeepAlive,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OllamaMessage {
    role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OllamaToolCall {
    #[serde(default)]
    id: Option<String>,
    function: OllamaToolCallFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OllamaToolCallFunction {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

#[derive(Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OllamaToolFunction,
}

#[derive(Serialize)]
struct OllamaToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    top_p: f32,
    num_ctx: usize,
}

fn to_ollama_message(m: &ChatMessage) -> OllamaMessage {
    OllamaMessage {
        role: match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
        .to_string(),
        content: m.content.clone(),
        tool_calls: m.tool_calls.as_ref().map(|calls| {
            calls
                .iter()
                .map(|c| OllamaToolCall {
                    id: c.id.clone(),
                    function: OllamaToolCallFunction {
                        name: c.name.clone(),
                        arguments: c.arguments.clone(),
                    },
                })
                .collect()
        }),
        tool_call_id: m.tool_call_id.clone(),
    }
}

fn from_ollama_message(m: OllamaMessage) -> ChatMessage {
    let role = match m.role.as_str() {
        "system" => Role::System,
        "user" => Role::User,
        "tool" => Role::Tool,
        _ => Role::Assistant,
    };
    ChatMessage {
        role,
        content: m.content,
        tool_calls: m.tool_calls.map(|calls| {
            calls
                .into_iter()
                .map(|c| ToolCall {
                    id: c.id,
                    name: c.function.name,
                    arguments: c.function.arguments,
                })
                .collect()
        }),
        tool_call_id: m.tool_call_id,
        name: None,
    }
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    prompt_eval_count: usize,
    #[serde(default)]
    eval_count: usize,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagEntry>,
}

#[derive(Deserialize)]
struct OllamaTagEntry {
    name: String,
}

#[derive(Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    model_info: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    capabilities: Vec<String>,
}

impl OllamaBackend {
    /// Connect to `base_url` for `model`, loading `tokenizer` (the model's
    /// real tokenizer, per §2.5 — never a `length / 4` estimate) and
    /// querying the backend for its actual context length (never hardcoded,
    /// per B-1).
    pub async fn connect(
        base_url: impl Into<String>,
        model: impl Into<String>,
        tokenizer: Tokenizer,
    ) -> Result<Self, InferenceError> {
        let base_url = base_url.into();
        let model = model.into();
        let client = reqwest::Client::new();

        let show_url = format!("{}/api/show", base_url.trim_end_matches('/'));
        let resp = client
            .post(&show_url)
            .json(&serde_json::json!({ "model": &model }))
            .send()
            .await
            .map_err(|source| InferenceError::Unreachable {
                endpoint: base_url.clone(),
                source,
            })?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(InferenceError::ModelNotFound { model });
        }
        if !resp.status().is_success() {
            return Err(InferenceError::BackendError {
                status: resp.status(),
                body: resp.text().await.unwrap_or_default(),
            });
        }

        let show: OllamaShowResponse = resp.json().await?;
        let context_length = show
            .model_info
            .iter()
            .find(|(k, _)| k.ends_with(".context_length"))
            .and_then(|(_, v)| v.as_u64())
            .unwrap_or(4096) as usize;

        let capabilities = Capabilities {
            tools: show.capabilities.iter().any(|c| c == "tools"),
            structured_output: true,
            vision: show.capabilities.iter().any(|c| c == "vision"),
            streaming: true,
        };

        Ok(Self {
            client,
            base_url,
            model,
            tokenizer,
            context_length,
            capabilities,
        })
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.base_url.trim_end_matches('/'))
    }

    fn build_request(&self, req: &ChatRequest, stream: bool) -> OllamaChatRequest<'_> {
        OllamaChatRequest {
            model: &self.model,
            messages: req.messages.iter().map(to_ollama_message).collect(),
            stream,
            tools: req
                .tools
                .iter()
                .map(|t| OllamaTool {
                    kind: "function",
                    function: OllamaToolFunction {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters.clone(),
                    },
                })
                .collect(),
            format: req.response_format.as_ref().map(|f| f.schema.clone()),
            options: OllamaOptions {
                temperature: req.sampling.temperature,
                top_p: req.sampling.top_p,
                num_ctx: self.context_length,
            },
            keep_alive: req.keep_alive,
        }
    }
}

#[async_trait]
impl InferenceBackend for OllamaBackend {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, InferenceError> {
        let body = self.build_request(&req, false);
        let resp = self
            .client
            .post(self.chat_url())
            .json(&body)
            .send()
            .await
            .map_err(|source| InferenceError::Unreachable {
                endpoint: self.base_url.clone(),
                source,
            })?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(InferenceError::ModelNotFound {
                model: self.model.clone(),
            });
        }
        if !resp.status().is_success() {
            return Err(InferenceError::BackendError {
                status: resp.status(),
                body: resp.text().await.unwrap_or_default(),
            });
        }

        let parsed: OllamaChatResponse = resp.json().await?;
        let done_reason = if parsed
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|c| !c.is_empty())
        {
            DoneReason::ToolCalls
        } else if parsed.done {
            DoneReason::Stop
        } else {
            DoneReason::Other
        };

        Ok(ChatResponse {
            message: from_ollama_message(parsed.message),
            done_reason,
            prompt_tokens: parsed.prompt_eval_count,
            completion_tokens: parsed.eval_count,
        })
    }

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatDelta, InferenceError>>, InferenceError> {
        use futures::StreamExt;

        let body = self.build_request(&req, true);
        let resp = self
            .client
            .post(self.chat_url())
            .json(&body)
            .send()
            .await
            .map_err(|source| InferenceError::Unreachable {
                endpoint: self.base_url.clone(),
                source,
            })?;

        if !resp.status().is_success() {
            return Err(InferenceError::BackendError {
                status: resp.status(),
                body: resp.text().await.unwrap_or_default(),
            });
        }

        // Ollama streams newline-delimited JSON objects, one per chunk, not
        // an SSE `event:`/`data:` framed stream. Each byte chunk from the
        // socket can straddle a JSON object boundary, so a line buffer is
        // carried across chunks exactly like `LLMClient._process_chunk_bytes`
        // did — the difference is this is driven by `tokio`, not
        // `_process(delta)` (B-5).
        let byte_stream = resp.bytes_stream();
        let mut buffer = String::new();

        let delta_stream = byte_stream.flat_map(move |chunk_result| {
            let mut deltas: Vec<Result<ChatDelta, InferenceError>> = Vec::new();
            match chunk_result {
                Ok(bytes) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(newline_idx) = buffer.find('\n') {
                        let line: String = buffer.drain(..=newline_idx).collect();
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<OllamaChatResponse>(line) {
                            Ok(parsed) => deltas.push(Ok(ChatDelta {
                                content: parsed.message.content.clone(),
                                tool_calls: parsed.message.tool_calls.map(|calls| {
                                    calls
                                        .into_iter()
                                        .map(|c| ToolCall {
                                            id: c.id,
                                            name: c.function.name,
                                            arguments: c.function.arguments,
                                        })
                                        .collect()
                                }),
                                done: parsed.done,
                            })),
                            Err(e) => deltas.push(Err(e.into())),
                        }
                    }
                }
                Err(e) => deltas.push(Err(e.into())),
            }
            futures::stream::iter(deltas)
        });

        Ok(Box::pin(delta_stream))
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, InferenceError> {
        #[derive(Serialize)]
        struct EmbedRequest<'a> {
            model: &'a str,
            input: &'a [String],
        }
        #[derive(Deserialize)]
        struct EmbedResponse {
            #[serde(default)]
            embeddings: Vec<Vec<f32>>,
        }

        let url = format!("{}/api/embed", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(&EmbedRequest {
                model: &self.model,
                input: texts,
            })
            .send()
            .await
            .map_err(|source| InferenceError::Unreachable {
                endpoint: self.base_url.clone(),
                source,
            })?;

        if !resp.status().is_success() {
            return Err(InferenceError::BackendError {
                status: resp.status(),
                body: resp.text().await.unwrap_or_default(),
            });
        }

        let parsed: EmbedResponse = resp.json().await?;
        Ok(parsed.embeddings)
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
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(source) => {
                return Ok(ModelHealth {
                    model: self.model.clone(),
                    status: HealthStatus::Unreachable {
                        detail: source.to_string(),
                    },
                    context_length: None,
                });
            }
        };

        if !resp.status().is_success() {
            return Ok(ModelHealth {
                model: self.model.clone(),
                status: HealthStatus::Unreachable {
                    detail: format!("HTTP {}", resp.status()),
                },
                context_length: None,
            });
        }

        let tags: OllamaTagsResponse = resp.json().await?;
        let installed = tags
            .models
            .iter()
            .any(|m| m.name == self.model || m.name.split(':').next() == Some(self.model.as_str()));

        Ok(ModelHealth {
            model: self.model.clone(),
            status: if installed {
                HealthStatus::Available
            } else {
                HealthStatus::ModelNotInstalled
            },
            context_length: Some(self.context_length),
        })
    }
}
