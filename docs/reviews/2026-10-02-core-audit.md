# Core audit and correctness repairs — 2026-10-02

**IMPLEMENTED correctness repairs; model effectiveness UNKNOWN.** The owner
asked for a review of memory, system-prompt construction, tool parsing,
context windows and retry flow, then authorized applying the corrections
before their manual test and a later battery test. Audit parent:
`9debbaef6bb8c1e9d286ce14e429095772edfa0a` (`develop`). This is a maintainer
engineering audit, not an external review or an agent capability result.

## Conditions and evidence

The read-only audit compiled the current working tree and exercised the
public conversation engine, parsers, personal prompt, retrieval and wiki with
fake providers and real sandboxed fixture commands. No model weights or
inference were loaded. Tokenizer-only measurements included the actual
24-tool schema: Qwen3.6's base input used 4,216 tokens with tools versus 698
without; gpt-oss used 2,958 versus 740. A 4,096-token window can therefore be
exceeded before any conversation history, despite the character estimate.

Raw audit evidence: `private-evidence/logs/core-audit-20261002/`
(`REPORT.md`, manifest, parser/effect/data probes and tokenizer counts).
Repair evidence: `private-evidence/logs/core-fixes-20261002/`.
The manifest and baseline patch record pre-existing tool sandbox and research
changes; they are preserved and excluded from this correction commit.

## Findings and resulting behavior

| ID / priority | Before / demonstrated trigger | Repair and regression evidence |
|---|---|---|
| C01 / P1 | Stop dropped a local checkpoint; a later write reused intent 1 and its receipt erased the earlier uncertainty | `session::perform` takes the high-water identity from the durable journal and records it before effects. Restore raises the checkpoint to that identity and conservatively retains overlapping legacy intents. Stop → write → restore regression retains the first uncertain command |
| C02 / P1 | A command wrote a marker before Stop or timeout, but the turn reported `edited=false`, bypassing closing checks | Permitted commands conservatively count as potentially editing before awaiting them. Timeout/I/O/audit failures retain uncertain intents. Verification is triggered without claiming the command succeeded or rolled back |
| C03 / P1 | `printf x >> marker; printf NU1301 >&2; exit 1` appended twice when a missing network permission was granted | Conversation and scripted loops preserve the first outcome and never automatically replay the whole command. The tool result tells the model to inspect partial effects before a new proposal; marker remains `x`. A once-only grant is not carried to an unrelated future command |
| C04 / P1 | Two declared calls with an action budget of one produced one result; later templates received an incomplete exchange | Unexecuted calls receive explicit terminal results. Interrupted calls say completion is unknown. Legacy partial groups are repaired before the next user message is rendered; repeated repair is idempotent |
| C05 / P1 | Qwen XML content `Before </function> after` became the successful write `Before` | Ambiguous function delimiters and unconsumed XML bytes are refused. Bare/missing-envelope repair preserves the whole candidate so it cannot hide ambiguity. Explicit, bare and missing-closing-envelope adversarial fixtures pass |
| C06 / P1 | A fenced JSON example preceded by “do not execute it” became `run_command` | Fenced recovery requires a reply consisting entirely of call fences. Prose with fences remains prose; explicit tool envelopes and native structured calls remain available |
| C07 / P2 | A Mistral JSON array silently filtered its malformed member and executed the remainder | Every array member must decode; otherwise the entire batch is rejected. Family revisions become `qwen-v4` and `mistral-v2` so old parser evidence cannot silently certify the changed contract |
| C08 / P1 | Dense memory evicted AGENTS.md and project summaries exceeded the final block allowance | Repository guidance reserves space first, then optional profile/memory/project entries are admitted under the final 8,000-byte cap with an omission disclosure. The existing 6,000-byte instruction excerpt cap is explicit and directs reading the full file before changes; it is not a claim that arbitrarily long instructions fit |
| C09 / P2 | Missing profile disabled memory; JSON `{}` enabled it | A manual `Profile::default` shares the serde default (`memory_enabled=true`). Missing-file and empty-JSON regression agrees |
| C10 / P1 | Accepting a proposal from A while B was visible wrote B's memory | The notification carries its immutable originating `cwd`; acceptance targets that origin and the UI shows it. Deduplication includes origin and scope. A→B desktop test confirms the request still targets A; old workspace notifications with no origin cannot be saved silently |
| C11 / P2 | External edits after graph construction left summaries appearing current | Current-workspace queries rehash source files before judging summaries. Cross-workspace recall stays wiki-only and labels freshness unknown, without asserting files changed. Responses identify the snapshot time |
| C12 / P2 | Same-size, preserved-mtime edits paired new excerpt bytes with the cached old hash | Markdown and code retrieval hash the same file read used to create excerpts. Preserved-mtime regression returns the current hash |
| C13 / P1 | A README paragraph became future system-prompt text through the project registry | The system block contains quoted project identifiers, not README descriptions. Descriptions remain available through `recall_project` as repository reference data. Sentinel regression checks both absence from system rules and continued recall visibility |
| C14 / P1 | Rust estimated characters; MLX rendered schemas/templates and began prefill without enforcing the chosen window | Requests carry `context_tokens`. The sidecar checks the actual rendered token vector, including processed images, before prefill; it caps generation and forced reasoning-close tokens to remaining space. A typed `PromptTooLarge` error follows context recovery, never a pointless smaller-window reload. Fake-engine and conversation regressions cover refusal before prefill, generation bounds and irreducible ContextFull |

An additional core defect surfaced during the full suite (**C15 / P1**):
`identifiers_are_time_ordered_and_unique` failed with retained IDs and proptest
seed (`workspace-id-failure.log`). In the pinned uuid 1.26.0 ContextV7,
observing `(2 s, 1 ms)`, then `(1 s, 999 ms)`, then `(2 s, 2 ms)` yields
UUID timestamps 2001 → 2999 → 2002 ms. The deterministic test fails before
and passes after clamping the complete `(seconds, nanos)` pair before UUID
sequencing under one process-local lock. The exact original clock sequence
was not instrumented; the mechanism is directly reproduced, not inferred
from a green rerun. ID format, persistence schema and dependency pins stay
unchanged. See `id-clock-red.log` and `id-clock-green.log`, and the
[pinned upstream timestamp source](https://raw.githubusercontent.com/uuid-rs/uuid/v1.26.0/src/timestamp.rs).

Adjacent parser repair: numbered commands preserve quoted argument boundaries,
stdin, cwd, sandbox intent and unknown fields for validation. Invalid numbered
members, unknown numbered fields and duplicate numeric positions remain
refusals instead of disappearing. The extra guard's failing and passing
regressions are in `numbered-red.log` and `numbered-green.log`. This changes no
sandbox grant.

## Validation and limits

Parser and effect regressions were observed failing on the audited parent
before fixes. The two MLX window regressions also fail against a copied
parent sidecar, without changing the working tree (`red-sidecar.log`). Data and memory reproductions are recorded in the read-only
audit, with permanent regressions added here. The pinned sidecar suite tests
the actual engine request path with scripted generation and real tokenizer
fixtures, not loaded model weights. Full workspace, Clippy, formatting,
desktop tests/build and milestone checks are recorded in the repair logs.
Final counts and the prepared application path are in the completion entry
of the experiment log.

These are deterministic correctness results. They do not show improved task
completion, latency or long-context quality. Full rehashing on wiki queries
and journal identity recovery add work to measure in real sessions. An
instruction excerpt beyond its cap still requires opening the full file.
Labels and prompt separation do not guarantee immunity to prompt injection.
Hosted CI, a clean dependency install, Docker-dependent coverage and model
behavior remain separate evidence requirements.

## Manual test before a battery run

Use the newly prepared `PWR.app`, not an already running old process. Parser
and sidecar revisions change calibration scope; rerun Quick Calibration for
the chosen Qwen/Mistral model if prior evidence is now stale. Then use a
scratch workspace and the same model/settings as the earlier observation:

1. Ask for a small edit plus its project checks. Inspect the actual diff and
   check outcome, not just the model's final answer.
2. Stop a permitted command after it has written something, then resume.
   It should stop promptly, preserve uncertainty and inspect before repeating.
3. Propose a workspace memory in A, switch to B, then accept. Check A's memory
   and that B did not receive it.
4. Resume a longer saved conversation and inspect context usage, tool results
   and any compaction notice. Record time to first content and visible errors.

Record the app/core hashes, model, context window, reasoning, sampling and
exact prompts for comparison. The later battery uses the approved stack-matrix
`pwr serve` evaluator, full versus minimal, frozen split/provenance, one engine
at a time and owner-approved scheduling. No campaign is launched in this cycle.

## Primary references used in the audit

- [Qwen function calling](https://qwen.readthedocs.io/en/latest/framework/function_call.html): templates and parsing are protocol conventions, not guarantees of valid calls.
- [Anthropic context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) and [writing tools](https://www.anthropic.com/engineering/writing-tools-for-agents): bounded relevant context and explicit tool semantics.
- [Instruction Hierarchy](https://arxiv.org/abs/2404.13208): hierarchy is also a trained behavior; adding a label alone does not establish a safety guarantee.
- [mlx-lm v0.31.3 generation source](https://raw.githubusercontent.com/ml-explore/mlx-lm/v0.31.3/mlx_lm/generate.py): generation `max_tokens` does not by itself enforce the complete input window.
- [Lost in the Middle](https://arxiv.org/abs/2307.03172) and [RULER](https://arxiv.org/html/2404.06654v3): declared window size does not establish reliable effective context.

Tool reduction, new compaction strategy, structured active memory, aggregate
recovery policy and call-region constrained decoding remain preregistered
experiments after baseline; they are not introduced as measured improvements.
