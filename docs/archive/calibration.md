> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Calibration

Calibration measures stable operating points for a particular deployment and machine, producing a signed/hashed `CalibrationProfile`.

## Procedure

Warm the model; snapshot host/backend; run fixed prompts at a context ladder; measure first-token latency, generation rate, peak memory/backend reported allocation, errors, and recovery. Repeat at least three times, randomize order after warm-up, and record raw samples. A stable point meets configured success rate, latency, and no-pressure thresholds. The profile stores maxima with confidence/variance—not invented capacity.

## Invalidations

Invalidate on model digest, quantization, provider/backend version, relevant model parameters, hardware compatibility key, or calibration harness change. Fresh backend state can temporarily downgrade a profile without invalidating it.

Calibration has no task-success claim; task evaluation is separate.

## Current admission contract — 2026-09-06

`calibration-harness-v5` supersedes v4. Previously, needle recall, observed
occupancy and post-reply pressure were recorded without all affecting admission.
A successful HTTP/stream exchange could therefore admit an unusable tier.
Admission now requires **every measured sample** to return exactly the needle
(surrounding whitespace permitted), report prompt occupancy between 0.75 and
1.0 of the requested context, and finish with a terminal stream chunk. Missing
occupancy or needle evidence refuses the tier. Observed pressure before **or**
after generation rejects it unless pressure was explicitly allowed by criteria.
Unknown pressure remains unknown; the gate rejects observed pressure, it does
not establish its absence when the host probe is unavailable.

The 0.75 floor is a provisional workload requirement shared with prompt filling,
not a measured threshold for coding competence. Three successful samples of one
needle do not establish long-range reasoning, editing accuracy, or a reliable
success probability on repositories. No new model measurements accompanied this
change. Existing v4 profiles are historical evidence and cannot authorize a v5
run; a new calibration is required. Tests use synthetic providers, including
wrong answers, missing/under/over occupancy, early EOF and post-reply pressure.

## Implementation notes

Warm-up is per tier, not per run: a backend reloads the model when the context size changes, so one warm-up leaves every later tier's first sample carrying a reload. A sample the backend reports as having loaded the model disqualifies its tier; where no load duration is reported this stays unknown rather than assumed warm.

Generation rate uses backend-reported token counts and durations where available, falling back to a local chunk rate otherwise, and every sample records which source it used.

Profiles store the thresholds they were judged against, and validation refuses to hold a point that fails them. A refusal is persisted like a profile, carrying its samples and the criteria each tier failed.

A ladder of `num_ctx` values with a fixed short prompt measures allocation, not occupancy: it establishes that a tier can be served, not what a full context costs.

**Closed as of 2026-09-03.** The prompt now fills three quarters of the tier — not all of it, since the reply needs somewhere to go and a full context measures a refusal rather than a cost — with varied filler, because a run of identical tokens compresses in ways a real prompt does not. Pressure is sampled after the reply as well as before, since reading it before generating is the one moment the cost is guaranteed not to have been paid. A needle placed at the start is asked for at the end: a tier where it comes back held the context, and one where it does not was allocated and then not used. The occupancy the backend reports is recorded against the tier requested.

Every profile measured under the older ladder is invalidated by the harness revision, which is that gate working: they measured something else.

## Cache scope observed in the live pilot — 2026-09-06

The v5 pilot's Qwen backend log reports a cache hit of 30,847/30,848 tokens on
repeated 32K prompts. Those sampled first-token latencies describe cached
replies; they do not estimate ingestion of fresh repository text. The initial
warm-up included uncached prompt processing lasting minutes. Model warming and
prefix-cache warming are different conditions and must be reported separately.

Previously warm-up samples contributed hashes to `raw_artifact_hashes` but were
not included in either outcome variant's serialized data. New outcomes retain
`warm_ups` separately for both admission and refusal. This changes retention,
not the v5 admission criteria; existing profiles remain readable. It does not
turn one warm-up into a reliable uncached latency distribution. The first
pilot's missing warm-up records cannot be reconstructed from their hashes;
the retained backend log is separate supporting evidence.
