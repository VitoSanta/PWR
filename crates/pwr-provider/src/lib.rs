//! Provider-neutral asynchronous model boundary.
use async_trait::async_trait;
use futures_util::StreamExt;
use pwr_domain::{
    BackendState, DeploymentDescriptor, GenerationMetrics, ModelChunk, ModelInspection,
    ModelRequest, ToolCall,
};
use std::pin::Pin;

/// A model that a backend can currently address.
///
/// This is deliberately inventory, not a PWR model profile: names, digests
/// and resource facts are observations made by one backend. Certification,
/// family behaviour and task policy remain outside the transport boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub model_ref: String,
    pub digest: Option<String>,
    pub context_limit: Option<u32>,
    pub size_bytes: Option<u64>,
    pub accelerator_size_bytes: Option<u64>,
}

/// What a backend can say about a model's shape, without loading it.
///
/// Raw facts, not a decision: the working window is computed from these by
/// `pwr_orchestrator::window`, which is the one place that interprets them.
/// Each field is `None` when the backend cannot say, and none of them is
/// guessed -- a window computed from a guessed cache cost is the problem this
/// replaces, in a different place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelFacts {
    /// A HuggingFace-style `config.json`, where the artifact carries one (MLX,
    /// safetensors).
    pub hf_config: Option<serde_json::Value>,
    /// GGUF metadata keys such as `qwen35moe.block_count`, where the backend
    /// exposes them (Ollama's `model_info`).
    pub gguf_metadata: Option<serde_json::Value>,
    /// The length the backend reports the model was trained to.
    pub trained_max: Option<u32>,
    /// The artifact's size, which is what its weights occupy once loaded.
    pub weights_bytes: Option<u64>,
    /// Bytes of attention scores one prefill step of the engine materialises:
    /// 0 when its attention is fused and keeps them on chip. `None` when the
    /// backend cannot say, and the window falls back to a coarser rule.
    pub prefill_scores_bytes: Option<u64>,
    /// Where the facts came from, for the artifact that records the window.
    pub source: String,
}

/// Transport features exposed by a backend, rather than inferred from its
/// name. A `false` answer means the backend cannot provide the feature; model
/// reliability is a separate profile/benchmark concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub model_discovery: bool,
    pub model_lifecycle: bool,
    pub streaming: bool,
    pub cancellation: bool,
    pub native_tools: bool,
    /// The backend constrains a requested tool call to the offered tool
    /// schema during generation, rather than merely asking the model to
    /// produce JSON that will be parsed afterwards.
    pub constrained_tool_calls: bool,
    pub generation_metrics: bool,
    /// Whether the caller can decide the context window a deployment serves.
    ///
    /// `false` is a real answer with consequences, not a missing feature: a
    /// backend that fixes the window elsewhere cannot be asked to run at a
    /// smaller one, so nothing can provoke its truncation behaviour and no
    /// context ladder can be measured on it. Callers that depend on either
    /// have to say so rather than discover it as a silent wrong number.
    pub context_window_control: bool,
}

/// Backend-facing extension of the existing generation boundary.
///
/// `ModelProvider` stays the narrow contract used by the current agent loop.
/// This companion trait supplies the lifecycle and inventory operations needed
/// by future selection/calibration without forcing the loop to know a backend
/// implementation. Backends that cannot explicitly load or unload a model
/// report that in `capabilities` and return an explanatory protocol error.
#[async_trait]
pub trait InferenceBackend: ModelProvider {
    fn backend_id(&self) -> &'static str;
    fn capabilities(&self) -> BackendCapabilities;
    async fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderError>;

    async fn load_model(&self, _model_ref: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Protocol {
            safe_context: "this backend does not expose explicit model loading".into(),
        })
    }

    async fn unload_model(&self, _model_ref: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Protocol {
            safe_context: "this backend does not expose explicit model unloading".into(),
        })
    }

    /// Whether the deployment's model is in the backend's memory now.
    async fn is_resident(&self, deployment: &DeploymentDescriptor) -> Result<bool, ProviderError> {
        Ok(self
            .runtime_state()
            .await?
            .loaded_models
            .iter()
            .any(|loaded| loaded == &deployment.model_ref))
    }

    /// Frees the memory the deployment's model holds in the backend.
    ///
    /// A command that loaded a model and exited left it there: LM Studio keeps
    /// what it loads until something unloads it, and a 20 GB model stayed
    /// resident after the command that needed it had finished, taking memory
    /// the next workload was then measured without. Reported by the person
    /// running the machine, 2026-09-13.
    async fn release(&self, _deployment: &DeploymentDescriptor) -> Result<(), ProviderError> {
        Err(ProviderError::Protocol {
            safe_context: "this backend cannot be asked to release a model".into(),
        })
    }

    /// What the backend can say about the deployment's model without loading it.
    ///
    /// The default knows only what inventory reports; a backend that can read
    /// the model's config or metadata overrides it.
    async fn model_facts(
        &self,
        deployment: &DeploymentDescriptor,
    ) -> Result<ModelFacts, ProviderError> {
        let found = self
            .discover_models()
            .await?
            .into_iter()
            .find(|model| model.model_ref == deployment.model_ref);
        Ok(ModelFacts {
            prefill_scores_bytes: None,
            trained_max: found.as_ref().and_then(|model| model.context_limit),
            weights_bytes: found.as_ref().and_then(|model| model.size_bytes),
            source: format!("{} inventory", self.backend_id()),
            ..ModelFacts::default()
        })
    }

    /// The backend's own build version, where the backend publishes one.
    ///
    /// `None` is an answer, not a failure: it means a result measured here
    /// cannot be scoped to a backend build, so certification must refuse
    /// rather than attribute evidence to an unknown one. A backend upgrade
    /// that silently kept a certification badge is the failure this exists to
    /// prevent.
    async fn backend_version(&self) -> Result<Option<String>, ProviderError> {
        Ok(None)
    }
}

pub type ModelStream =
    Pin<Box<dyn futures_core::Stream<Item = Result<ModelChunk, ProviderError>> + Send>>;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider unavailable: {safe_context}")]
    Unavailable { safe_context: String },
    #[error("provider protocol error: {safe_context}")]
    Protocol { safe_context: String },
    #[error("provider operation timed out: {safe_context}")]
    Timeout { safe_context: String },
    #[error("provider context limit exceeded: {safe_context}")]
    ContextLimit { safe_context: String },
    #[error("provider operation cancelled")]
    Cancelled,
    /// The reply stopped without the backend saying it was finished.
    ///
    /// A short answer and an abandoned one look identical in the assembled
    /// text, so this is the difference between a deployment that answered
    /// briefly and a connection that died mid-generation. Treating the second
    /// as the first is how a truncated reply becomes a recorded result.
    #[error("provider reply was truncated: {safe_context}")]
    Truncated { safe_context: String },
    /// The backend could not parse what the model produced.
    ///
    /// Distinct from `Protocol`, which is the transport or the backend
    /// misbehaving. This is the deployment emitting a tool call the backend's
    /// own template parser rejects -- measured: `XML syntax error on line 3:
    /// unexpected end element </function>` returned in a 200 body.
    ///
    /// It is the same class of thing as a malformed tool call, and belongs in
    /// the same place: told to the deployment and retried under the same
    /// bound, not ending a sixty-action run over one bad generation.
    #[error("deployment produced output the backend could not parse: {safe_context}")]
    ModelOutput { safe_context: String },
    /// The thinking phase reached its budget, the engine closed it once, and
    /// no answer or action followed (the model reopened its reasoning, or
    /// stopped). Distinct from `Truncated`: the reply was bounded on purpose,
    /// and what it lacks is the transition, not an ending.
    #[error("the model's reasoning reached its budget without an answer: {safe_context}")]
    ReasoningUnfinished { safe_context: String },
}

/// A handle that stops a reply in progress.
///
/// Cancellation was claimed and never demonstrated: `ProviderError::Cancelled`
/// was not constructed anywhere, and the capability probe judged it by reading
/// three chunks, dropping the stream, and calling `/api/ps` to see whether the
/// backend answered. A backend that answers is not a backend that stopped
/// generating.
///
/// What actually stops a local backend is the connection closing, so that is
/// the mechanism here rather than a message: cancelling drops the underlying
/// stream, which drops the HTTP body, which closes the socket. The error is
/// then reported as `Cancelled` rather than as a broken stream, so abandoning a
/// reply is never recorded as the deployment failing.
#[derive(Clone, Default)]
pub struct Cancel {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    notify: std::sync::Arc<tokio::sync::Notify>,
}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stops the reply this handle guards. Idempotent.
    pub fn cancel(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Resolves once cancelled, and immediately if it already was.
    pub async fn cancelled(&self) {
        loop {
            if self.is_cancelled() {
                return;
            }
            self.notify.notified().await;
        }
    }
}

/// Wraps a stream so cancelling the handle closes it.
///
/// The drop is the point. A flag that only stops the reader leaves the backend
/// generating into a socket nobody reads, which is the resource this is meant
/// to release.
pub fn cancellable(cancel: Cancel, stream: ModelStream) -> ModelStream {
    Box::pin(futures_util::stream::unfold(
        (Some(stream), cancel),
        |(stream, cancel)| async move {
            let mut stream = stream?;
            if cancel.is_cancelled() {
                // Dropped by leaving scope with `None` as the next state; the
                // connection closing is what stops the backend.
                return Some((Err(ProviderError::Cancelled), (None, cancel)));
            }
            tokio::select! {
                next = stream.next() => next.map(|item| (item, (Some(stream), cancel))),
                () = cancel.cancelled() => {
                    drop(stream);
                    Some((Err(ProviderError::Cancelled), (None, cancel)))
                }
            }
        },
    ))
}

/// One model reply, assembled from every chunk of its stream.
///
/// Reading a stream's first chunk is not reading a reply: a reasoning
/// deployment opens with `thinking` chunks whose content is empty, and its
/// answer and any tool call arrive later -- as late as the final chunk. Every
/// consumer goes through here so that mistake has one place to not be made.
#[derive(Debug, Clone, Default)]
pub struct ModelReply {
    /// Assembled answer text, excluding the reasoning channel.
    pub content: String,
    pub thinking: String,
    pub tool_calls: Vec<ToolCall>,
    pub chunks: usize,
    pub metrics: Option<GenerationMetrics>,
}

/// Upper bound on chunks read from one reply, guarding against a deployment
/// that never stops emitting.
///
/// A chunk is about a token, so this bounds how long one reply may be. It was
/// 16,384, chosen when a turn meant one tool call, and a deployment that emits
/// several calls at once legitimately needs more: measured on a 35B writing a
/// stylesheet and four components together, 16,384 cut the reply off mid-work.
///
/// Raising it to 131,072 then showed the other half of the picture. The same
/// model, given a larger task, streamed for seventy minutes without stopping
/// and reached that bound too -- around 126,000 tokens in one reply, which is
/// not a long turn but a runaway one. The bound was catching a real pathology,
/// not only honest work.
///
/// So it sits between the two: enough for several files in one turn, far short
/// of an hour of generation. Reaching it is no longer fatal either -- the
/// conversation tells the deployment its reply ran away and asks for less per
/// turn, which is the only remedy that does not throw away the work already
/// done.
pub const MAX_REPLY_CHUNKS: usize = 32_768;

/// How long a reply that has already begun may go silent before it is treated
/// as stopped.
///
/// The transport already bounds a reply, but only in total, so a stream that
/// died and a stream that is merely slow arrive the same way: after the whole
/// budget has elapsed. Measured: a backend stopped serving mid-generation and
/// the run noticed fifteen minutes later, at the client timeout, with the
/// error naming the timeout rather than the silence.
///
/// It applies only **after the first chunk**, and that distinction is the
/// whole of it. Before the first chunk the deployment is reading the prompt,
/// which produces nothing by definition and can take minutes: measured on a
/// 27B whose backend wrote 2.5 GB of prompt cache over three minutes while
/// this bound, applied from the start, cut it off and reported a healthy
/// backend as gone. Waiting for a first token and falling silent halfway are
/// different situations, and only the second is evidence of anything.
pub const REPLY_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// Reads a stream to completion and returns the assembled reply.
pub async fn collect_reply(stream: ModelStream) -> Result<ModelReply, ProviderError> {
    collect_reply_with(stream, |_| {}).await
}

/// The same, showing each chunk to `observe` as it arrives, so a front end can
/// render a reply while it is being generated rather than minutes later.
pub async fn collect_reply_with(
    mut stream: ModelStream,
    mut observe: impl FnMut(&pwr_domain::ModelChunk),
) -> Result<ModelReply, ProviderError> {
    let mut reply = ModelReply::default();
    let mut done = false;
    loop {
        // Bounded only once the reply has started. Reading the prompt produces
        // nothing however healthy the backend is, so a bound applied before
        // the first chunk measures prefill and calls it death; the transport's
        // own total timeout is what covers that stretch.
        let next = if reply.chunks == 0 {
            stream.next().await
        } else {
            match tokio::time::timeout(REPLY_IDLE_TIMEOUT, stream.next()).await {
                Ok(next) => next,
                // Said as silence rather than as a timeout, because they call
                // for different things: a backend that stopped talking is not
                // a reply that took too long, and an operator told the second
                // goes looking for a slow model instead of a dead one.
                Err(_) => {
                    return Err(ProviderError::Truncated {
                        safe_context: format!(
                            "the deployment fell silent for {} seconds after it had begun \
                             replying; the backend may have stopped serving this model",
                            REPLY_IDLE_TIMEOUT.as_secs()
                        ),
                    });
                }
            }
        };
        let Some(next) = next else { break };
        let chunk = next?;
        observe(&chunk);
        reply.chunks += 1;
        reply.content.push_str(&chunk.content);
        if let Some(thinking) = &chunk.thinking {
            reply.thinking.push_str(thinking);
        }
        reply.tool_calls.extend(chunk.tool_calls);
        if chunk.metrics.is_some() {
            reply.metrics = chunk.metrics;
        }
        if chunk.done {
            done = true;
            break;
        }
        if reply.chunks >= MAX_REPLY_CHUNKS {
            return Err(ProviderError::Truncated {
                safe_context: "reply exceeded the chunk bound before the backend finished".into(),
            });
        }
    }
    if reply.chunks == 0 {
        return Err(ProviderError::Protocol {
            safe_context: "provider returned an empty stream".into(),
        });
    }
    if !done {
        // The stream ended without a terminal chunk. What was assembled may be
        // a whole answer or the first half of one, and nothing here can tell
        // the two apart -- so it is not returned as an answer.
        return Err(ProviderError::Truncated {
            safe_context: "stream ended without a terminal chunk".into(),
        });
    }
    Ok(reply)
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn inspect(
        &self,
        deployment: &DeploymentDescriptor,
    ) -> Result<ModelInspection, ProviderError>;
    async fn runtime_state(&self) -> Result<BackendState, ProviderError>;
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError>;

    /// Make the deployment serve one context window, and report the window
    /// actually in force.
    ///
    /// Backends where the window is a per-request option honour whatever the
    /// request carries, so the default answers with the number it was given
    /// and does nothing. Backends where the window is fixed when the model is
    /// loaded have to reload to change it, and what they return is what the
    /// deployment will really use.
    ///
    /// The return value exists because those two can differ. A caller that
    /// asked for one tier, was served another, and recorded the tier it asked
    /// for has produced a measurement of something that never happened -- and
    /// a whole calibration ladder measured that way looks like several tiers
    /// while being one.
    async fn prepare_context(
        &self,
        _deployment: &DeploymentDescriptor,
        context_tokens: u32,
    ) -> Result<u32, ProviderError> {
        Ok(context_tokens)
    }

    /// The same reply, abandonable.
    ///
    /// Defaulted rather than required because the mechanism is transport-level
    /// and the same for every provider that streams over a connection: closing
    /// it is what stops the backend. A provider whose backend needs telling
    /// explicitly overrides this.
    async fn chat_cancellable(
        &self,
        request: ModelRequest,
        cancel: Cancel,
    ) -> Result<ModelStream, ProviderError> {
        Ok(cancellable(cancel, self.chat(request).await?))
    }
}
