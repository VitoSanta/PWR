> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Model compatibility layer

## Contract

Canonical input is `AgentRequest { goal, task_profile, constraints,
context_package, tools, execution_policy, verification_policy }`; canonical
output is `AgentResponse { narrative, actions, finish_intent, diagnostics,
generation_metrics }`. Actions are typed `ToolCall`, `Plan`, `Answer`,
`Abort`, `Retry` or `Replan`, not provider JSON. Reasoning text is optional
diagnostic data and never required for control flow.

`ModelBehaviorAdapter` translates requests and normalizes responses for one
model/profile. `FamilyAdapter` supplies common Qwen/Granite conventions,
templates and quirks. A profile selects the family adapter and overrides only
evidence-backed details. A generic adapter presents conservative JSON/native
tools and treats unsupported claims as unknown.

Example: the core asks for `replace_text`; a Qwen adapter may emit native tool
definitions, while a future backend adapter serializes a JSON schema. Both
return the same canonical `ToolCall`; `pwr-tools` is unchanged.

Adapter responsibilities: role/template conversion, tool rendering, reasoning
control, structured-output choice, response parsing/normalization, malformed
output classification, retry/recovery wording and context order. It must not
select files, perform an edit, decide verification or own backend lifecycle.

`ModelRequest`, structural `ToolCall`, `ModelChunk.thinking` and
`ReasoningControl` are useful CURRENT seeds. Raw `tools: serde_json::Value`,
CLI `reasoning_*` helpers and parser coupling in `orchestrator/lib.rs` are
REFACTOR targets.

Unknown model mode is allowed only after backend discovery: explain that it is
Compatible/Experimental, use the generic adapter, disable unproven parallel
tools/thinking assumptions, reduce autonomy, validate every action, and invite
an inspection/benchmark. It never impersonates certification.

## What is implemented (AR-007, AR-008 first half)

`pwr-compat` owns response normalization and nothing else. `CanonicalReply`
carries `narrative`, `thinking`, `tool_calls`, `diagnostics`, `chunks` and
`metrics`; the loop reads actions from it instead of from `ModelReply`.
`GenericAdapter` is the identity and is the default in `RunTuning`, so a run
with no recognised family behaves exactly as it did before this layer existed.

`QwenFamilyAdapter` absorbs two conventions the family emits regardless of
backend: a call rendered as a `<tool_call>` block inside the answer text, and
`<think>` reasoning inline where the transport has no separate channel for it.
An unterminated `<think>` span is read as reasoning that was cut off, which is
what makes a turn that spent its budget reasoning classify as `thinking_only`
rather than as prose where a call belonged. Recovery is never invention: a
block that does not decode stays visible in the narrative, and a reply whose
calls the backend already parsed is not second-guessed.

The adapter is chosen by `adapter_for(family, model_ref)` from the family a
backend reported, falling back to the model reference and then to the generic
adapter. Every normalization is recorded on `RunEvent::TurnGenerated` as
`normalizations`, so a run says whether the adapter carried it.

Still outstanding: canonical `AgentRequest`/`AgentResponse` with typed
`Plan`/`Answer`/`Abort`/`Retry`/`Replan` actions and `finish_intent`. Actions
remain `ActionProposal` in the orchestrator. Tool rendering deliberately stays
at the backend boundary: the envelope is a property of a backend's protocol,
not of a model family, and Ollama's native-tools shape and LM Studio's
OpenAI-compatible shape are the same for every family they serve.
