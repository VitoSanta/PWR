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

/// A context extension a model's family publishes but its config leaves off.
///
/// Qwen2/2.5 and Qwen3's dense models are trained to 32,768 tokens and
/// documented to hold 131,072 with YaRN (factor 4), added to the config by
/// whoever wants it, since static YaRN costs a little on short text. The
/// published MLX conversions leave it off, so a 64 GB Mac that holds ~110k of
/// Qwen2.5-Coder-14B's context was given 32k. Offered only where the engine
/// applies it: mlx-lm builds YaRN for `qwen2` and `qwen3`, while `qwen3_moe`
/// uses a fixed RoPE and would ignore it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RopeExtension {
    pub factor: f64,
    /// The length it was trained to, which the extension starts from.
    pub original: u32,
    /// The length it reaches.
    pub extended: u32,
}

impl RopeExtension {
    /// The `rope_scaling` entry a config gets to use it.
    pub fn rope_scaling(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "yarn",
            "rope_type": "yarn",
            "factor": self.factor,
            "original_max_position_embeddings": self.original,
        })
    }
}

/// The extension a model's `config.json` could use, if its family has one and
/// the config does not already scale its positions.
pub fn rope_extension(config: &serde_json::Value) -> Option<RopeExtension> {
    let text = config.get("text_config").unwrap_or(config);
    if text
        .get("rope_scaling")
        .is_some_and(|scaling| !scaling.is_null())
    {
        return None;
    }
    let model_type = text
        .get("model_type")
        .or_else(|| config.get("model_type"))?
        .as_str()?;
    let trained = text.get("max_position_embeddings")?.as_u64()?;
    // Qwen3's configs say 40,960 -- its native 32,768 and room for the
    // answer -- while YaRN is still counted from 32,768 (Qwen3's card).
    let native = match (model_type, trained) {
        ("qwen2", 32_768) | ("qwen3", 32_768 | 40_960) => 32_768,
        _ => return None,
    };
    Some(RopeExtension {
        factor: 4.0,
        original: native,
        extended: 131_072,
    })
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
    /// The reply fell into a loop, writing the same passage over and over,
    /// and was stopped rather than left to run to its token cap.
    ///
    /// Distinct from `Truncated`, whose way out is doing less per turn: a
    /// model going round in circles needs to stop analysing and act. Measured
    /// 2026-09-26 (Qwen3.6-35B-A3B, reasoning off, a spreadsheet engine):
    /// 29,000 characters restating why `-2^2` parsed wrong, the same three
    /// paragraphs again and again, for eight minutes of a turn.
    #[error("the model's reply was going round in circles: {safe_context}")]
    Looping { safe_context: String },
}

/// How much of a reply's end is looked at for a loop, in bytes.
const LOOP_WINDOW: usize = 6_000;
/// A reply shorter than this is not judged at all: a short answer that says
/// one thing twice is not a loop.
const LOOP_MIN_TEXT: usize = 2_500;

/// Whether the end of `text` is going round in circles: a line of some
/// length written four times or more, or -- for text without line breaks --
/// fewer than two in five of its 80-byte stretches unlike the others.
///
/// Prose and reasoning, never tool arguments (the MLX adapter holds those
/// back), which is where legitimate repetition -- a table, a test file --
/// lives.
pub fn looping(text: &str) -> Option<String> {
    if text.len() < LOOP_MIN_TEXT {
        return None;
    }
    let mut start = text.len().saturating_sub(LOOP_WINDOW);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let window = &text[start..];
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for line in window
        .lines()
        .map(str::trim)
        .filter(|line| line.len() >= 40)
    {
        let count = counts.entry(line).or_default();
        *count += 1;
        if *count >= 4 {
            let shown: String = line.chars().take(80).collect();
            return Some(format!("the passage \"{shown}\" came back {count} times"));
        }
    }
    let bytes = window.as_bytes();
    if bytes.len() >= LOOP_MIN_TEXT {
        let mut seen = std::collections::HashSet::new();
        let mut total = 0usize;
        let mut index = 0;
        while index + 80 <= bytes.len() {
            seen.insert(&bytes[index..index + 80]);
            total += 1;
            index += 20;
        }
        if total > 0 && seen.len() * 5 < total * 2 {
            return Some(format!(
                "only {} of the last {total} stretches of its text were new",
                seen.len()
            ));
        }
    }
    None
}

/// A handle that stops a reply in progress.
///
/// Cancellation was claimed and never demonstrated: `ProviderError::Cancelled`
/// was not constructed anywhere, and the capability probe judged it by reading
/// three chunks, dropping the stream, and calling `/api/ps` to see whether the
/// backend answered. A backend that answers is not a backend that stopped
/// generating.
///
/// HTTP backends release their response body on cancellation. A managed pipe
/// backend also watches this handle and sends its explicit cancel protocol.
/// Reader teardown alone is not evidence that a worker stopped generating.
/// The error is reported as `Cancelled`, rather than as a broken reply.
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
            // notify_waiters observes a Notified from its creation, even before
            // its first poll. Register before reading the flag to close the race.
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// Cancels an in-flight operation when its owning future is abandoned.
/// Disarm only after ownership passes to a guarded stream or work finishes.
pub struct CancelGuard(Option<Cancel>);
impl CancelGuard {
    pub fn disarm(mut self) {
        self.0 = None;
    }
}
impl Drop for CancelGuard {
    fn drop(&mut self) {
        if let Some(cancel) = &self.0 {
            cancel.cancel();
        }
    }
}
impl Cancel {
    pub fn drop_guard(&self) -> CancelGuard {
        CancelGuard(Some(self.clone()))
    }
}

// Field order matters: notify the worker before dropping its reply/ended sender.
struct CancellableReply {
    guard: CancelGuard,
    cancel: Cancel,
    stream: ModelStream,
}

/// Wraps a stream so cancelling the handle closes it.
///
/// Abandoning an unfinished stream also signals cancellation before dropping
/// its body, so managed workers can stop through their backend-specific watcher.
pub fn cancellable(cancel: Cancel, stream: ModelStream) -> ModelStream {
    let state = CancellableReply {
        guard: cancel.drop_guard(),
        cancel,
        stream,
    };
    Box::pin(futures_util::stream::unfold(
        Some(state),
        |state| async move {
            let mut state = state?;
            tokio::select! {
                biased;
                () = state.cancel.cancelled() => Some((Err(ProviderError::Cancelled), None)),
                next = state.stream.next() => {
                    match next {
                        None => { state.guard.disarm(); None },
                        Some(Ok(chunk)) => {
                            if chunk.done { state.guard.0 = None; }
                            Some((Ok(chunk), Some(state)))
                        }
                        Some(Err(error)) => Some((Err(error), None)),
                    }
                }
            }
        },
    ))
}

/// One response's configured wall bound, including opening and streaming.
pub async fn bounded_chat(
    provider: &dyn ModelProvider,
    request: ModelRequest,
    cancel: Cancel,
    timeout: std::time::Duration,
) -> Result<ModelStream, ProviderError> {
    let guard = cancel.drop_guard();
    let deadline = tokio::time::Instant::now() + timeout;
    let timed_out = || ProviderError::Timeout {
        safe_context: format!(
            "response exceeded its configured {}-second deadline",
            timeout.as_secs()
        ),
    };
    let stream = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(ProviderError::Cancelled),
        result = tokio::time::timeout_at(deadline, provider.chat_cancellable(request, cancel.clone())) => {
            match result { Ok(result) => result?, Err(_) => return Err(timed_out()) }
        }
    };
    let timer_cancel = cancel.clone();
    let timed = futures_util::stream::unfold(Some(stream), move |stream| {
        let cancel = timer_cancel.clone();
        async move {
            let mut stream = stream?;
            tokio::select! {
                biased;
                () = tokio::time::sleep_until(deadline) => {
                    cancel.cancel();
                    Some((Err(ProviderError::Timeout { safe_context: format!("response exceeded its configured {}-second deadline", timeout.as_secs()) }), None))
                }
                next = stream.next() => next.map(|chunk| (chunk, Some(stream))),
            }
        }
    });
    let stream = cancellable(cancel, Box::pin(timed));
    guard.disarm();
    Ok(stream)
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
    stream: ModelStream,
    observe: impl FnMut(&pwr_domain::ModelChunk),
) -> Result<ModelReply, ProviderError> {
    collect_reply_with_guard(stream, observe, None).await
}

/// Bound an agent reply that has already spent substantial reasoning and then
/// keeps emitting plain answer text without a tool call. Tool-call bodies are
/// held back by the MLX adapter and remain free to be as long as needed.
pub async fn collect_reply_with_guard(
    mut stream: ModelStream,
    mut observe: impl FnMut(&pwr_domain::ModelChunk),
    unstructured_limit: Option<(usize, usize)>,
) -> Result<ModelReply, ProviderError> {
    let mut reply = ModelReply::default();
    let mut done = false;
    let mut checked_at = 0usize;
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
        // Judged every kilobyte or so rather than every chunk: a chunk is a
        // token or two, and the window is six thousand bytes.
        if unstructured_limit.is_some() && !chunk.done {
            let written = reply.content.len() + reply.thinking.len();
            if written / 1024 != checked_at / 1024 {
                checked_at = written;
                if let Some(said) = looping(&reply.content).or_else(|| looping(&reply.thinking)) {
                    return Err(ProviderError::Looping { safe_context: said });
                }
            }
        }
        if let Some((min_thinking, max_content)) = unstructured_limit
            && reply.thinking.len() >= min_thinking
            && reply.content.len() >= max_content
            && reply.tool_calls.is_empty()
            && !chunk.done
        {
            return Err(ProviderError::Truncated {
                safe_context: format!(
                    "the model kept writing unstructured answer text after reasoning \
                     ({max_content} bytes) without a tool call"
                ),
            });
        }
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
        let guard = cancel.drop_guard();
        let stream = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(ProviderError::Cancelled),
            result = self.chat(request) => result?,
        };
        let stream = cancellable(cancel, stream);
        guard.disarm();
        Ok(stream)
    }
}

#[cfg(test)]
mod rope_tests {
    #[test]
    fn qwen_dense_models_trained_to_32k_can_reach_128k_with_yarn() {
        let qwen25 = serde_json::json!({"model_type": "qwen2", "max_position_embeddings": 32768, "rope_scaling": null});
        let extension = crate::rope_extension(&qwen25).unwrap();
        assert_eq!((extension.original, extension.extended), (32_768, 131_072));
        assert_eq!(extension.rope_scaling()["type"], "yarn");
        assert!(
            crate::rope_extension(
                &serde_json::json!({"model_type": "qwen3", "max_position_embeddings": 32768})
            )
            .is_some()
        );
        // Qwen3-14B's config says 40960; YaRN still starts from 32768.
        let qwen3 = crate::rope_extension(
            &serde_json::json!({"model_type": "qwen3", "max_position_embeddings": 40960}),
        )
        .unwrap();
        assert_eq!((qwen3.original, qwen3.extended), (32_768, 131_072));
        for config in [
            serde_json::json!({"model_type": "qwen2", "max_position_embeddings": 32768, "rope_scaling": {"type": "yarn", "factor": 4.0}}),
            serde_json::json!({"model_type": "qwen2", "max_position_embeddings": 131072}),
            serde_json::json!({"model_type": "qwen3_moe", "max_position_embeddings": 32768}),
            serde_json::json!({"model_type": "llama", "max_position_embeddings": 32768}),
        ] {
            assert!(crate::rope_extension(&config).is_none(), "{config}");
        }
    }
}

#[cfg(test)]
mod abandonment_tests {
    #[test]
    fn dropping_a_managed_stream_signals_worker_cancellation() {
        let cancel = super::Cancel::new();
        let stream = super::cancellable(cancel.clone(), Box::pin(futures_util::stream::pending()));
        drop(stream);
        assert!(
            cancel.is_cancelled(),
            "dropping the reader left the worker running"
        );
    }
}

#[cfg(test)]
mod response_deadline_tests {
    use super::*;
    use futures_util::StreamExt;
    struct Worker {
        opening: bool,
        tokens: std::sync::Mutex<Vec<Cancel>>,
    }
    #[async_trait]
    impl ModelProvider for Worker {
        async fn inspect(
            &self,
            _: &DeploymentDescriptor,
        ) -> Result<ModelInspection, ProviderError> {
            unreachable!()
        }
        async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
            unreachable!()
        }
        async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
            if self.opening {
                std::future::pending::<()>().await;
            }
            Ok(Box::pin(futures_util::stream::unfold((), |()| async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                Some((Ok(pwr_domain::ModelChunk::default()), ()))
            })))
        }
        async fn chat_cancellable(
            &self,
            request: ModelRequest,
            cancel: Cancel,
        ) -> Result<ModelStream, ProviderError> {
            self.tokens.lock().unwrap().push(cancel.clone());
            let guard = cancel.drop_guard();
            let stream = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(ProviderError::Cancelled),
                result = self.chat(request) => result?,
            };
            let stream = cancellable(cancel, stream);
            guard.disarm();
            Ok(stream)
        }
    }
    fn request() -> ModelRequest {
        ModelRequest {
            deployment: DeploymentDescriptor {
                schema_version: 1,
                id: pwr_domain::new_id(),
                provider: "fake".into(),
                endpoint: "http://localhost/".into(),
                model_ref: "fake".into(),
                backend_options: Default::default(),
                auth_ref: None,
            },
            messages: vec![],
            context_tokens: 8192,
            tools: None,
            seed: None,
            sampling: Default::default(),
        }
    }
    #[tokio::test(start_paused = true)]
    async fn configured_response_deadline_covers_opening_and_active_progress() {
        for opening in [true, false] {
            let worker = Worker {
                opening,
                tokens: Default::default(),
            };
            let cancel = Cancel::new();
            let start = tokio::time::Instant::now();
            let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                let mut stream = bounded_chat(
                    &worker,
                    request(),
                    cancel.clone(),
                    std::time::Duration::from_secs(1),
                )
                .await?;
                while let Some(chunk) = stream.next().await {
                    chunk?;
                }
                Ok::<(), ProviderError>(())
            })
            .await;
            assert!(
                matches!(outcome, Ok(Err(ProviderError::Timeout { .. }))),
                "opening={opening}: {outcome:?}"
            );
            assert_eq!(start.elapsed(), std::time::Duration::from_secs(1));
            assert!(cancel.is_cancelled());
            assert_eq!(worker.tokens.lock().unwrap().len(), 1);
        }
    }
    #[tokio::test(start_paused = true)]
    async fn dropping_an_opening_response_cancels_its_worker() {
        let worker = Worker {
            opening: true,
            tokens: Default::default(),
        };
        let cancel = Cancel::new();
        let mut response = Box::pin(bounded_chat(
            &worker,
            request(),
            cancel.clone(),
            std::time::Duration::from_secs(30),
        ));
        tokio::select! { biased; _ = &mut response => panic!("opening completed"), _ = tokio::time::sleep(std::time::Duration::from_millis(1)) => {} }
        drop(response);
        assert!(cancel.is_cancelled());
        assert_eq!(worker.tokens.lock().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod cancellation_race_tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn simultaneous_cancellation_does_not_strand_waiters() {
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..10_000 {
            let cancel = Cancel::new();
            let gate = std::sync::Arc::new(tokio::sync::Barrier::new(2));
            let waiting = cancel.clone();
            let waiting_gate = gate.clone();
            tasks.spawn(async move {
                waiting_gate.wait().await;
                waiting.cancelled().await;
            });
            tasks.spawn(async move {
                gate.wait().await;
                cancel.cancel();
            });
        }
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while let Some(result) = tasks.join_next().await {
                result.unwrap();
            }
        })
        .await
        .expect("a simultaneous cancellation lost its notification");
    }
}
