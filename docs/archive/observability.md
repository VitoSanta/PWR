> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Observability

Emit structured `tracing` events keyed by run, task, model digest, deployment, profile, and repository snapshot. Trace: discovery, selection rationale, provider calls, token estimates/reports, tools, state transitions, verification, error taxonomy, and resource samples.

Metrics include latency histogram, generated tokens/sec, context-tier selections, tool success, policy denials, verification outcomes, recovery count, and calibration stability. Logs must support a replay report without retaining source contents by default. Sampling/redaction rules are configuration and testable. Export local JSONL first; OpenTelemetry is an optional adapter.

## Implementation status — 2026-09-03

**Almost none of the above exists.** `pwr-observe` is seven lines that hash a payload, and no crate in the runtime depends on it. There is no JSONL export, no replay report, no latency histogram, no resource sampling, and no configurable sampling or redaction.

What does exist is the event log, which carries more than this document credits it with: every run appends typed events under one identifier inside a hash chain — provenance, state transitions, every tool attempt allowed or denied, verification results, compaction, plans, named loops, recovery decisions — and since 2026-09-03 each turn records the backend's own counters, prompt and generated tokens and both halves of the duration. `pwr report` reads it back, and an evaluation counts events by type into its outcomes.

**The layer exists as of 2026-09-03.** The events are typed, `report --format jsonl` writes the trail as one record per line, and `replay` folds it into counts: actions by outcome, turns, prompt and generated tokens, backend generation time separately from wall clock, named loops and non-progress, compactions, context downgrades, delivery divergences, turns under memory pressure, and the outcome. Everything is counted from events rather than asserted. A line this build cannot read is skipped and counted; a record whose type it does not know is left out of the export with the count stated.

Memory pressure is sampled after each turn rather than once at admission, so a run that starts on a quiet machine and ends on a saturated one now records the difference.

An export carries the stored hash as an identifier, not as something it can re-verify: the hash covers the event's id, run, payload, timestamp and the link before it, and an export carries only some of those. Whether the chain holds is the store's question, and `report` asks it there.

Still absent: a latency histogram, and a retention policy with configurable sampling and redaction. OpenTelemetry remains an optional adapter nobody has needed.

## Naming what a log already shows — 2026-09-07

Six runs of 2026-09-06/07 were read row by row to find out why each ended where
it did, and every answer turned out to be a pattern over events the log already
carried: turns thrown away for asking to read four files at once, thirty-six
re-reads of files the harness itself had marked unchanged, nine reads of a path
that was never in the workspace, turns that reasoned and never answered with the
prompt at 97% of the authorised context, and a deployment told four times that
its build was broken which read on until the stall guard stopped it.

The gap was never capture, again. `pwr diagnose <run-id>` reads the log and
names the pathologies, each with a count and the specifics grouped the way it
would be acted on. It costs no model time and is deterministic, so two runs can
be compared by their findings rather than by someone remembering what the last
one looked like.

Three real logs are committed as fixtures, and every detector is tested against
the run that taught it. That makes them regression tests for the fixes as well:
`batchable_turns_rejected` going to zero on a new run is the evidence that read
batching works, without anyone reading timestamps.

Two things they are not. They find nothing new — a detector exists because a
person found the pathology first, and the next unknown one still needs someone
to look; the property worth having is that nobody finds the same one twice. And
they are only as honest as their definitions: the first version of
`thinking_only_turns` counted every turn with no content and reported sixty-six
silent turns in a run that had one, because a deployment that reasons and then
answers through a tool call has an empty content field on every successful turn.
A detector that pairs each turn with what the loop did next reports one. The
hand count it replaced was wrong in the other direction, calling twelve turns
batchable where the log supports nine.
