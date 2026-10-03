# Compatibility across model families

Status: reply selection IMPLEMENTED; the expanded artifact contract below is
PLANNED. Written 2026-10-03. Protocol support is not task certification.

PWR already has family reply adapters, model sampling profiles, an MLX backend,
and calibration evidence. Extend those boundaries rather than introducing a
second agent loop. The first change exposes `ReplyProtocol` in `pwr-compat`
and selects existing adapters through it. Architecture metadata takes precedence
over repository names. Verified Ornith 1.5 9B and 35B-A3B aliases use Qwen's
adapter when metadata is absent; unknown Ornith releases stay generic.
The Gemma OneThought sidecar guard blocks both re-opening and repeated closing
of the current reply's thought channel; historical markers remain ignored.
This fixes a measured close-marker repetition path, not overall task reliability.
Cache checkpoints include both tensors and MLX cache metadata. Restoring only
tensors left sliding-window offsets and ring indices at their generated values;
a regression with the real RotatingKVCache reproduces the stale position and
checks the next append against a fresh reference. Task-level impact is still
being measured.
Gemma native thought delimiters are recognized by both capability planning and
the sidecar reasoning tracker. Previously a Gemma thought could exhaust the
whole output cap while its thinking budget was not enforced; the ledger trace
on 34fef36b contains a 30K-character thought lasting about 327 seconds. The
existing reasoning budget and finalization path now apply to this protocol.
The Gemma thought guard also honors `thinking=false` after tool results: the
canonical template emits no closed thinking prefix there, so relying on the
prompt boundary alone allowed a new thought block despite the disabled mode.
Gemma's quoted-string parser change is now identified by `gemma4-v3`, so old
calibrations cannot certify the changed parser as the old revision.

## Four independent concerns

| Layer | Owner | Responsibilities |
|---|---|---|
| Runtime | `pwr-mlx` / provider | Supported architecture and kernels, weights, memory admission, cache, streaming, cancellation, backend-native structured calls |
| Message protocol | Backend template rendering + `pwr-compat` reply adapters | Roles, tool catalogue/call/result encoding, reasoning channels, turn endings; deterministic normalization |
| Artifact policy | `pwr-models` and exact deployment profiles | Pinned template, sampling by supported reasoning mode, context limits, quantization, capabilities, provenance |
| Agent loop | `pwr-orchestrator` | Permissions, tool schemas, edits, verification, recovery and stop decisions |

The brand is a discovery hint, not a full policy. `model_type` identifies an
execution architecture; a fine-tune can change its template without changing
that architecture. Therefore metadata-first reply selection is the current
baseline, not proof that every fine-tune has its base model's protocol.
A future template-aware resolver must validate known signatures or an explicit
profile and preserve unknown/conflicting evidence instead of silently guessing.

## Target coverage

| Models | Reply protocol | Independent requirements | Evidence scope |
|---|---|---|---|
| Qwen | Qwen JSON/XML and think spans | Exact generation/template version; reasoning-mode sampling; attention/cache implementation | Adapter regressions; brief task campaign remains incomplete |
| GPT-oss 20B / 120B | Harmony | Channels, handoff/end-of-turn tokens, reasoning effort, supported MoE quantization | Existing Harmony adapter; no new task certification |
| Hosted GPT | Provider-native structured calls | API capability negotiation, transport, limits | Keep separate from GPT-oss; no new hosted provider added here |
| Gemma 4 | Native Gemma call/thought markers | Template version; tool ids/results; thinking-history rules | Parser regressions; active 12B campaign, not certified |
| Nemotron 3.5 Lightning | Qwen XML for calls | Nemotron reasoning policy; hybrid Mamba/attention cache and MoE runtime | Existing call regression; full battery pending |
| Ornith 1.5 9B / 35B-A3B | Qwen XML | Ornith's own template and card settings; dense versus MoE runtime | New alias + call regressions; no inference claim |
| Other Ornith / older Gemma / unknown derivatives | Explicit metadata/profile or generic | Inspect actual config and template before adding aliases | Unknown until inspected |

Nemotron sharing the tool parser does not imply identical Qwen reasoning
policies. Nor does an OpenAI-compatible HTTP endpoint imply Harmony: it is a
transport envelope, and may already deliver authoritative structured calls.
Adapters keep those calls authoritative and must not duplicate them.

## Size, dense models and MoE

Parameter count does not determine message syntax. Different sizes can share
one protocol while differing in task reliability, latency, context cost and
training. Never infer that a larger model passes because a smaller one passed,
or add a parser per size without an observed wire-format difference.

MoE and dense are independent of attention architecture. A model can combine
MoE experts with recurrent or linear-attention layers. Track total parameters
and active parameters separately: active experts influence computation, while
resident weights include inactive experts unless the runtime explicitly
supports offloading. A 30B-A3B artifact is not a 3B memory allocation. Actual
weight bytes, quantization metadata, cache/state buffers and measured memory
remain the admission evidence. Context cost also depends on layer types and
cache implementation, not only on dense/MoE or total parameters.

Do not automatically lower output limits, raise penalties or change acceptance
criteria for small models. The 4096-token cap experiment failed to improve the
short campaign and truncated legitimate code. Alternative policies require
isolated measurements on the same tasks and explicit provenance.

## Planned artifact contract and rollout

Extend existing profile/certification records with these independently sourced
facts; do not derive missing facts from a marketing name:

- Artifact repository and revision; config/tokenizer/template fingerprints.
- Backend and version, sidecar/binary revision, protocol adapter revision.
- Execution architecture, layer/cache topology, dense/MoE, total/active
  parameters when published, expert counts and routing when configured.
- Actual quantization and resident bytes, supported context versus configured
  context, supported reasoning modes and mode-specific sampling provenance.
- Capabilities individually measured: template rendering, tool call/result
  round trip, read/write/edit/command, cancellation and multi-step tasks.

First use deterministic protocol fixtures (including malformed and truncated
calls). Then serial micro-probes for read, edit and command/result round trips.
Then require the six existing short cases green on the same binary, sidecar
and template/profile identity. Preserve failed and interrupted traces. Only
then extend to Nemotron, GPT-oss and Ornith and longer tasks; each artifact
gets its own evidence. A unit test passing establishes parser behavior only.

Record time to first token, average decode throughput, end-to-end task time,
verification result, repetitions and truncations. A reported peak of 80 tokens
per second for Nemotron is useful motivation, not a reproducible task result.
Classify failures by observed evidence: timeout, runtime/core defect,
protocol/template mismatch or unresolved model behavior. Do not label every
failed task a model limitation.

## Primary sources checked 2026-10-03

- [Ornith 1.5 9B card](https://huggingface.co/ornith-ai/Ornith-1.5-9B),
  [config](https://huggingface.co/ornith-ai/Ornith-1.5-9B/raw/main/config.json)
  and [template](https://huggingface.co/ornith-ai/Ornith-1.5-9B/raw/main/chat_template.jinja).
- [Ornith 1.5 35B-A3B config](https://huggingface.co/ornith-ai/Ornith-1.5-35B-A3B/raw/main/config.json)
  and [template](https://huggingface.co/ornith-ai/Ornith-1.5-35B-A3B/raw/main/chat_template.jinja).
- [Nemotron 3.5 Lightning vendor deployment instructions](https://huggingface.co/nvidia/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-NVFP4):
  separate Nemotron reasoning parser, Qwen3-Coder tool parser and Mamba backend.
  NVIDIA serving settings are not automatically applicable to MLX.
- [GPT-oss model card](https://huggingface.co/openai/gpt-oss-20b): Harmony,
  active versus total parameters, reasoning effort and MXFP4.
- [Gemma thinking](https://ai.google.dev/gemma/docs/capabilities/thinking)
  and [Gemma 4 function calling](https://ai.google.dev/gemma/docs/capabilities/text/function-calling-gemma4).

These live sources justify the separation; deployed profiles must pin source
revisions and actual local artifact hashes before certification.

## Reviewer instruction consistency

The CLI review system instruction now agrees with the executor's rule-by-rule
MET/NOT MET checklist. Previously it demanded only violations and prohibited
reporting any met rule, contradicting the checklist. Review remains required;
checks, budgets, findings extraction and acceptance gates are unchanged. The
35B ledger trace on db012699 had passing independent checks but timed out
after review feedback, including a questionable claim that changing a locally
created Date mutated inputs. Aligning instructions removes the contradiction;
it does not certify every reviewer finding or guarantee shorter completion.

## Command schema and execution contract

`run_command` now declares `additionalProperties: false`, matching the decoder's rejection of unknown request fields. The description repeats the accepted
request keys because native templates may render only properties and required
fields. Gemma goal traces supplied `ts` and `exec_os_error`; neither is a command
argument. Existing tolerant argument normalization, permission checks and
execution limits are unchanged. Other tools are not presumed to have the same
closed-object policy.

The regression failed on the previous open schema and passes with the corrected
contract; it also checks that valid commands still decode and the observed
diagnostic fields stay rejected. This establishes schema consistency, not
six-case model certification or proof that the mismatch caused all task failures.
See [JSON Schema additional properties](https://json-schema.org/understanding-json-schema/reference/object#additionalproperties).
