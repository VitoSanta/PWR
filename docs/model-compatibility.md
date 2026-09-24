# Model compatibility, calibration and Reasoning Effort

**Written 2026-09-24** against the revision that introduced it. It describes
what is implemented; where something is not, it says so.

Code: `crates/pwr-models/src/profile.rs` (states, provenance, reuse rules),
`crates/pwr-models/src/calibration.rs` (Quick Calibration),
`crates/pwr-domain/src/reasoning.rs` (Reasoning Effort),
`crates/pwr-mlx/sidecar/pwr_mlx.py` (the engine's thinking budget),
`crates/pwr-orchestrator/src/converse.rs` (the turn).

## Model profile states

A model PWR has never tested is **not** an unsupported model. The status
separates "not tested" from "not compatible":

| Status | Meaning | What the person can do |
|---|---|---|
| **Verified** | PWR's own controlled evaluation of this exact artifact and backend ships with PWR (`crates/pwr-models/verified-models.json`). | Everything. |
| **Locally calibrated** | Quick Calibration ran on this machine and every check agent mode depends on passed. | Everything. Confidence is *preliminary*. |
| **Provisional** | Nothing measured. Where every new model starts. | Everything, with conservative defaults. |
| **Limited** | Calibration found that something agent mode depends on -- a tool call, valid arguments, continuing after a tool result, a reply that ends -- did not work. | Chat without a workspace. Agent tasks in a workspace are refused with the reason. |
| **Incompatible** | Concrete evidence the artifact cannot be used: it could not be read, it ships no chat template (MLX), or no calibration request produced a reply. | Nothing; the reason is shown. |

Beside the status, **confidence**: *established* (verified evidence that
applies in full), *preliminary* (a quick local check), *reduced* (evidence
from a setup that differs in a way that may matter), *untested*.

**No model is Verified in v0.1.0-alpha.** The registry is empty on purpose:
the evaluations on record (`docs/roadmap.md`) ran on earlier engine builds and
without the artifact provenance an entry requires. Claiming them would
attach old evidence to artifacts it was not gathered on.

## What happens when PWR meets an untested model

1. The engine inspects the folder: `config.json`, the chat template (from
   `chat_template.jinja`, `chat_template.json` or `tokenizer_config.json`),
   the tokenizer files and the weight file names and sizes. All of it is read
   as data; nothing in the folder is executed.
2. Static checks. Only concrete incompatibility stops the model here: an
   unreadable folder, or an MLX model with no chat template (a base model --
   PWR cannot format a conversation for it). A missing profile, an unknown
   architecture name or an unusual template are **not** reasons to refuse.
3. The model is **Provisional**. The app shows *New model detected -- this
   model has not yet been tested by PWR* with **Run Quick Calibration** and
   **Use Conservative Defaults**. Neither is required: the model can be used
   straight away.
4. **Conservative defaults** mean:
   - sampling: whatever the model's own template and the backend default to;
     PWR invents no family-specific values;
   - reasoning: the capability read from the template, with the
     conservative budget mapping (below); a model whose template shows no
     reasoning markers gets no budget at all;
   - context: the window computed from this machine's memory and the model's
     declared maximum, as for every model, and **not** lowered further. The
     project's measurements say a starved window is itself the most common
     cause of failure (`docs/context-management.md`), so a smaller default
     would make an untested model look worse than it is. The window is
     labelled *Context: provisional* -- Quick Calibration does not measure long
     context.
   - "Use Conservative Defaults" is remembered for that model in the
     workspace (`acknowledged_provisional` in `.pwr/chat-config.json`) so
     the choice is not offered again; the status stays Provisional.

## Quick Calibration

A bounded, deterministic check of whether PWR can safely *operate* the
model. Not a benchmark.

| Check | How it is scored | Agent-critical |
|---|---|---|
| `termination` | the reply ended by itself (no truncation, no timeout) | yes |
| `instruction_following` | the reply is exactly `READY` (whitespace, one pair of quotes/backticks and a final period ignored) | no |
| `structured_output` | parses as JSON; exactly the two requested fields and values | no |
| `code_understanding` | exactly `42` | no |
| `repository_file_selection` | exactly `src/parser.rs` from a three-file fixture | no |
| `tool_selection` | exactly one call, to `read_file` | yes |
| `tool_arguments` | arguments validate against the tool's JSON Schema and name the file | yes |
| `tool_result_continuation` | given a fixed transcript with a tool result, the answer uses it (`1337`) | yes |
| `answer_after_reasoning` | with thinking allowed (budget 1,024), the answer is `51` | no |
| reasoning budget | with a 16-token budget, the engine closes the phase and an answer follows | no (sets the reasoning profile) |

- **Bounded:** at most nine requests; each capped at 512 answer tokens (plus
  the probe's thinking budget) and 120 s. On the maintainer's M2 Max,
  Qwen3-14B 4-bit took 21 s.
- **Deterministic:** fixed prompts, temperature 0, fixed seed.
- **Mechanical:** every check is a comparison, a JSON parse or a schema
  validation. No model judges another.
- **Cancellable:** Cancel stops the generation in progress (the engine
  receives a cancel and stops within a token) and nothing is recorded. A
  request that times out fails its check and the calibration moves on.
- **Safe:** fixture text only; no files are read or written, no command runs.
- **Result:** Locally calibrated when every agent-critical check passes;
  Limited otherwise; Incompatible when no request produced a reply at all.
  Reasoning results never limit a model: a failed forced close only means no
  budget is enforced on it.

**What it does not prove:** that the model codes well; that it works at long
context (never tested); that it follows instructions across a long
conversation; or anything about another quantization, backend, template or
engine version (see the reuse rules).

## Provenance and the reuse rules

A model name is not an identity. Evidence records:

model reference, Hub revision (written as `.pwr-revision` by PWR's
downloader), artifact path (not part of identity), the backend's artifact
digest (MLX: config + weight index; GGUF: header + metadata), a fingerprint of
the weight files' names and sizes, artifact size, quantization, format,
architecture, tokenizer fingerprint, chat-template fingerprint, backend,
backend version (MLX: `mlx-lm <v>; mlx <v>; sidecar <hash>`; llama.cpp: the
server's version), pwr version, calibration suite version, a coarse
hardware class (`macos-arm64-apple-m2-64gb`), and the time. A fact that cannot
be read is recorded as unknown -- never guessed. The parameter count is
recorded only where the backend reports one; neither engine does today.

Deterministic rules (`pwr_models::profile::compare`):

| What differs | Outcome |
|---|---|
| backend, format, artifact digest, weight files, artifact size, quantization, revision, tokenizer, chat template, architecture | **stale** -- not applied; Provisional, "recalibrate" |
| calibration suite version | **stale** |
| backend major version (for 0.x versions, the minor: `mlx-lm 0.31 → 0.32`) | **stale** |
| backend minor/patch version, the engine script's hash | **reduced** -- applied, confidence lowered, reason shown |
| hardware class | **reduced** |
| a behaviour-determining fact known on one side only | **reduced** |
| pwr version, artifact path, observation time | applies |

Precedence: static incompatibility; then a local calibration that found the
model Limited or Incompatible (a failure on this machine outranks a pass
elsewhere); then verified evidence that applies; then local evidence that
applies; then Provisional. Stale evidence is never applied and is named in
the reasons ("the last calibration no longer applies: quantization changed").

**Storage.** Verified evidence ships with the build
(`crates/pwr-models/verified-models.json`). Local calibrations live outside
any repository: `~/.pwr/model-evidence/<hash>.json` (or
`$PWR_EVIDENCE_DIR`), one file per backend and model, named by a hash so a
model reference never becomes a path. An application update leaves them in
place; the rules above decide what still applies. Nothing generated by
calibration is written inside a workspace or the checkout.

## Reasoning Effort

A setting with three levels -- **Low**, **Medium** (default), **High** --
chosen in the model popover and saved per workspace
(`reasoning_effort` in `.pwr/chat-config.json`). It is a typed enum from
the client to the engine (`pwr_domain::ReasoningEffort`); any other value
is refused.

It controls **the budget of a model's explicit thinking phase** in one
generation: how many tokens the model may spend reasoning before it has to
move on to its answer or its next tool call. It does **not** change how many
searches, tool calls or retries the agent makes, and it does not promise a
better answer.

### How models expose reasoning, and what PWR does

The capability is read from the model's own chat template (as text) and
refined by calibration -- never from a family name
(`pwr_domain::TemplateReasoning`, `ReasoningCapability`):

| Capability | How it is recognised | What Low/Medium/High do |
|---|---|---|
| `native_budget` | template has a thinking-delimiter pair **and** reads `thinking_budget` (Seed-OSS) | the template is told the budget (in 512-token steps) **and** the engine enforces the exact budget |
| `explicit_thinking_stream` | template uses `<think>…</think>` or `<seed:think>…</seed:think>` | the engine counts thinking tokens with the model's tokenizer and closes the phase at the budget |
| `template_controlled` | template reads `reasoning_effort` (harmony, gpt-oss) | the model's **native** low / medium / high levels: the chosen level is passed to the template. It is a level, not a token budget -- the engine does not count or close harmony's analysis channel |
| `observable_only` | llama.cpp with a thinking template | reasoning is shown, but llama-server takes no per-request budget, so the setting has no effect |
| `none` | calibration saw no reasoning and the template had no markers | no effect |
| `unknown` | no markers, not calibrated | no effect; the template's defaults apply |

A profile that turns thinking off wins at every level -- except on a model
that cannot switch reasoning off at all. gpt-oss's packaged profile says
`think: false`, but its least is `low`; for it (and any template that only
takes levels) the three native levels apply and Medium is the default, which
is also gpt-oss's own default. Before 2026-09-24 that profile made it always
run at `low`. When the setting cannot act, the app
disables the selector and says why, e.g. *"This model does not expose a
separately controllable reasoning phase that PWR recognizes. PWR uses
compatible generation defaults."*

### Budgets

| Mapping | Low | Medium | High | When |
|---|---|---|---|---|
| declared | from the profile | | | a profile naming this exact artifact (digest) or deployment -- never a family or bare tag -- with strictly increasing values |
| calibrated | 2,048 | 6,144 | 12,288 | Quick Calibration saw an answer follow a forced close |
| conservative | 1,024 | 4,096 | 8,192 | otherwise (Provisional models) |

Why these numbers: 6,144 was the engine's chat default from 2026-09-22
(backlog D.E2E-20), kept as the calibrated Medium; ~4,000 is where backlog
C.15 proposed stopping a reasoning phase that takes no action, after Qwen3.6
reasoned for 56,059 characters in one turn, and is the conservative Medium.
Low and High are about half and double. They are budgets, measured in the
model's own tokens -- not quality settings.

### Clamping: the effective budget

Before every generation -- after any compaction -- the turn computes the
envelope (`pwr_domain::plan_reasoning`):

```
room            = window − prompt − safety margin (max(512, window/32))
answer reserve  = min(4,096, room)          # the answer / tool call keeps this, always
effective       = min(requested, room − answer reserve)   # 0 if under 256
max_tokens      = min(effective + answer allowance (16,384), room)
```

The prompt is counted as the engine's own count for what it has seen plus a
dense estimate (three characters a token) for what was appended since, so
the estimate errs toward less reasoning, never less answer. If the requested
budget does not fit it is **clamped** (and recorded as such); with less than
256 tokens of room thinking is switched off for that generation (or given a
zero budget if the template cannot switch it off). Tests pin: effective ≤
requested, `max_tokens` ≤ room, and the answer keeps its reserve whatever
reasoning uses (property test over windows, prompts and efforts).

### Reaching the limit

For `explicit_thinking_stream` and `native_budget`:

1. The engine streams reasoning on its own channel and counts one token per
   generated token inside the block.
2. At the budget it stops generating, appends the template's own closing
   delimiter and a blank line -- nothing else, no model-family prose -- and
   continues generating the answer inside what is left of `max_tokens` (the
   reserve). It announces `finalizing`. It does this **once per request**.
3. If the model reopens its thinking or stops without an answer, the request
   ends `reasoning_unfinished` -- never a "successful" reply holding only
   reasoning or half a tool call.
4. The turn then retries **once** with thinking off (or a zero budget). A
   second failure stops the turn with a stated reason
   (`StopReason::ReasoningUnfinished`). The bound is
   `REASONING_FINALIZATION_RETRIES = 1`; it cannot loop.

A model whose forced close failed during calibration gets no budget at all
(the selector says so), rather than a control that cannot work.

### Compaction

Reasoning is never written back into the conversation (the assistant message
keeps only the answer and its calls), so a large budget cannot grow the
history. The budget is recomputed after each compaction from the room then
left, and the per-turn compaction bound (two) is unchanged, so a large
budget cannot cause compaction cycles.

### Token accounting

| Figure | Source | Exact? |
|---|---|---|
| prompt, generated (MLX) | the engine, one count per token of the loaded model's tokenizer | exact |
| reasoning / answer split (MLX) | the engine, inside/outside the tracked delimiters | exact; `null` for templates without delimiters (harmony), never 0 |
| prompt, generated (llama.cpp) | the server's usage block | exact for its tokenizer |
| reasoning split (llama.cpp) | `completion_tokens_details.reasoning_tokens` where the server reports it | otherwise unknown |
| tool-call tokens | characters ÷ 4 | estimate, labelled `tool_call_tokens_estimated` |
| context composition | characters ÷ 4 | estimate, labelled |

Each generation's metrics carry `token_accounting` (`engine_tokenizer`,
`server_usage`, `estimated`). The context panel shows the last reply's
reasoning and answer tokens with `~` before any estimate.

### Audit

Recorded in `.pwr/state.sqlite` for the conversation:
`generation.started` (model, backend, profile status, effort, capability,
directive, requested and effective budget, clamped, budget source, whether
it is a finalization retry, `max_tokens`, answer reserve),
`reasoning.started` (elapsed time only), `reasoning.budget_reached`,
`generation.finalizing`, `reasoning.completed` (tokens, accounting),
`reasoning.finalization_failed`, `generation.completed` (tokens by phase,
accounting, duration). **The reasoning text is never logged**; a test
asserts that no event payload contains it. (The app still streams a model's
reasoning live to the person, as before; it is not stored with the
conversation.)

## Security

- Model folders are data. The sidecar loads with `trust_remote_code` set to
  false explicitly, forces the Hub offline (`HF_HUB_OFFLINE=1`), and renders
  chat templates through transformers' sandboxed Jinja environment. PWR
  never runs `.py` files from a model repository, and the downloader never
  fetches them.
- A relative model reference cannot contain `..` (it names a folder below the
  models root); an absolute path is taken as the person's explicit choice.
- Downloads: sizes and checksums are required for every file, paths must stay
  inside the model folder, existing files are never overwritten, and the
  revision marker is written only if it is a full commit id.
- Evidence files are named by a hash of backend and model reference; nothing
  from model metadata becomes a path or a command.

## Limitations

- No Verified models yet (see above).
- Quick Calibration is nine short prompts: a pass means PWR can operate
  the model, not that it is good at the work.
- The budget cannot be enforced on harmony (gpt-oss) models or on llama.cpp;
  the UI says so.
- Seed-OSS's native budget is passed in 512-token steps; a budget under 512
  puts its template in no-thinking mode (the engine's exact cap still
  applies otherwise).
- The finalization fallback depends on the model continuing sensibly after
  its closing delimiter; calibration tests it, once, at a tiny budget.
- Reasoning Effort and the compatibility gate apply to conversations (the
  app, `pwr serve`, the console). The scripted research loops (`PWR
  run`, `eval run`) keep their own reasoning policy and are unchanged.
- Parameter count is not recorded (neither engine reports it).
- Hardware class is coarse by design.
