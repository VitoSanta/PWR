> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Context Management

Context is a budget across system instructions, task, repository excerpts, tool outputs, model response, and safety reserve. The scheduler maintains token estimates plus actual provider-reported values when present.

## Requirement

Context capacity is selected only from compatible `CalibrationProfile` evidence, model inspection metadata, and current backend state. If no safe evidence exists, use the conservative bootstrap profile and label it uncalibrated; do not extrapolate from machine RAM.

## Algorithm

1. Reject incompatible calibration (model digest, backend/version, quantization, hardware compatibility key).
2. Bound requested context by inspected provider/model capability and measured stable calibration points.
3. Reserve output and tool-result headroom; apply repository retrieval quotas by symbol/file relevance.
4. Compact only at explicit checkpoints: persist a factual task ledger, retain source references and hashes, then replace bulky history with summary plus retrievable evidence.
5. On context/backend failure, reduce one calibrated tier, retry once if idempotent, and event it.

Token counting is provider-specific and an estimate unless the backend reports exact counts.

## Measured boundary behaviour

A configured context limit is not a limit the backend can be trusted to enforce. Measured across seven local deployments at the same boundary, three distinct contracts appeared: the limit ignored and the whole prompt accepted; the prompt silently truncated with no error and content lost; and a clean typed rejection. The division is not by runtime -- two deployments on the same backend format behaved differently.

The scheduler therefore enforces the budget before sending, and checks the backend's reported prompt token count against what it believed it sent. A deployment's measured `context_boundary` observation says which contract it offers; a `truncated_silently` deployment gives no other signal that context was dropped.

## Compaction, as implemented

Compaction happens at an explicit checkpoint between actions, when the estimated history exceeds half the context budget — never mid-action, when the history is incomplete.

**The ledger is built from the audit, not from the deployment's recollection.** A summary a model writes about its own work can be wrong about what it did; the event log cannot. The ledger lists files read with their artifact hashes, files changed with their current hashes, commands run with exit codes, actions that were refused and why, and the state of the repository checks after the last change.

Carrying hashes through matters: an edit planned before compaction is still valid after it. Carrying refusals matters for the opposite reason — without them a deployment retries a denied action from a blank memory.

The system prompt and the original task survive compaction because they are the instruction and the goal. Everything between them is reconstructible from the audit and is not worth its tokens.

Token counts here are estimates at four characters per token, labelled as such in the `context.compacted` event. A backend that reports real counts is used where it does; this is not one of those places.

## Resolution, as implemented

There is one context number and it is the calibrated one. Until 2026-09-03 there were two: calibration produced `execution.context_tokens`, the run recorded that in `run.started`, and the request builder then substituted the static default declared for the tag in `strategies/models.json` — 262144 for four of the seven deployments. A profile calibrated at 32768 could therefore authorise a quarter-million-token request, and every log line, retrieval budget and compaction threshold downstream described a limit the backend never saw. The resolved execution profile is now what the request carries, and no model profile may overwrite it.

Where a provider failure looks like a context failure, the retry steps down to the next **measured** calibration tier below the current one. Where no measured tier is lower, the run stops rather than halving a number to see what happens: an uncalibrated context is exactly what requirement 4 prohibits, and it is no more acceptable as a fallback than as a default.

Compaction now identifies the messages it keeps by what they are rather than by their position. It previously kept the first two on the assumption that they were the system prompt and the task; on a session resumed with `--session`, the second message is the session ledger, so compaction preserved the ledger and discarded the goal — on a long run, at the moment context was most under pressure.

**The estimate is now checked against the backend.** Every turn compares the reported `prompt_eval_count` with what the budget believed it sent and with the authorised context, and events `context.delivery_diverged` when the difference is too large to be the estimate's own looseness — reading far less than was sent is the silent-truncation signature, and reading more than was authorised means the limit was not enforced. The check is deliberately loose in both directions: a finding that fires every turn is one nobody looks at. The estimate itself is still four characters per token. Repository excerpts and the task also still share one user message rather than being separately budgeted sections, and tool calls and their results travel as serialised JSON text rather than as the protocol's own typed messages.

## Compiled, not concatenated — 2026-09-03

A prompt was built by concatenating strings at the call site. The repository excerpts and the task shared one user message, so nothing downstream could tell them apart, and the budget was a fraction rather than an accounting.

Sections are typed now — system, model suffix, session ledger, repository excerpts, task — and each carries its estimated cost and the hash of what was sent rather than of what was offered, so a truncated section is not mistaken for the whole one. The compilation is recorded as `context.compiled`.

Fitting has an order and a floor. Required sections are never cut. Excerpts give way before the ledger, because excerpts are a starting point the agent can rebuild with `search` and `read_file` while the ledger is the only account of what earlier runs did and cannot be recovered from the workspace. A section is dropped rather than cut to a stub: half an excerpt reads like a whole file. Output headroom is reserved before anything is fitted, since a prompt that fills the context leaves the deployment nowhere to answer.

The output reserve is a quarter of the context — a starting value, not a measured one, kept as a single constant so that it can be measured at all.

## Task identity through compaction — 2026-09-06

Compiled messages now retain a harness-only `purpose`: task, session ledger,
or repository excerpts. The Ollama adapter does not send this metadata.
Compaction pins the actual task and preceding instructions/ledger, discarding
initial excerpts before dropping the task. The old “first user message” rule
pinned the ledger or excerpts in production and discarded the later request.
Legacy callers without purpose metadata retain the first-user fallback.

H04 now builds its input through the production context compiler, with separate
system, ledger, excerpts and task sections, and checks retention across repeated
compaction. Serialized tool-call arguments also contribute to history cost.
This fixes deterministic loss of the task; it does not prove that a model uses
it. The four-characters-per-token estimate, tool-schema overhead and a required
prefix too large to fit remain limits of the budget mechanism. Do not interpret
retention as a hard guarantee that every provider request fits its token limit.

## The window the backend serves, not the one the model advertises — 2026-09-09

The chat took its context from the model's advertised ceiling, bounded by any
declared profile, and recorded that number as fact. It never asked the backend
to load that window, so nothing checked whether the backend was serving it.

Measured on `qwen3-next-80b-a3b-instruct-ream-mlx` under LM Studio: the tag
advertises 262,144 tokens, the profile declared 65,536, and the chat ran at
65,536 while the instance was loaded at 16,384. A prompt that does not fit is
not refused. The MLX engine applies `TruncateMiddle`:

```
TruncateMiddle policy activated, pre-processing the '42227' token prompt
by removing '35137' tokens from the middle, starting at token idx (n_keep) '2203'
```

It keeps the head — system prompt and task — and the tail, and deletes
everything between them: every tool result, file body and build log the turn
had gathered. The reply comes back normal. Across one run it did this 65 times.

The signature is in the reported prompt cost, which counts what survived the
cut rather than what was sent, so it stops moving:

```
2203, 3063, 3134, 4806, 4851, 6417, 6789, 11244, 13130, 13505,
7090, 7090, 7090, 7090, 7090, 7090, 7090, 7090, 7090, 7090, 7090, ...
```

42,227 − 35,137 = 7,090, and 7,090 plus the 9,294 tokens the engine reserved for
generation is 16,384 exactly — the loaded window.

The cost was three runs of an 80B recorded as a model that could not converge.
It re-read files it had just read and reintroduced errors it had just fixed,
which is what anyone does when the middle of the conversation is removed
between turns. Nothing about the deployment's ability was measured by them.

Two changes. `maximise_context` now calls `prepare_context`, which loads the
window and reads back what the backend actually grants, and the conversation is
measured against that number — a backend serving less than was asked is a fact
the operator can act on, while a backend serving less than was assumed corrupts
every turn silently. And the conversation loop carries the same divergence check
the run path already had, in the form the chat can afford: two different prompts
do not tokenise to the same length by chance, so a reported cost that repeats
while messages are being added is reported once as silent truncation, naming the
remedy. LM Studio's parallel slots are worth checking there — an instance loaded
with four of them was the reason the window was a quarter of what was asked for.

## The context panel and one compaction for both triggers — 2026-09-23

The conversation's automatic compaction and the app's "Compact now" are one
function, `pwr_orchestrator::compaction::compact`, recorded as
`context.compacted` with `trigger`, `model`, `session_id` and estimated tokens
before and after. The record now keeps the first request verbatim, changed
files with their hashes, paths, open errors and the last check verdict, and
merges an earlier record instead of nesting it. The threshold is settable per
workspace (50–90 %, default 75 %). The context panel shows window, used,
remaining and an estimated composition. Current description:
[`models-and-context.md`](models-and-context.md).
