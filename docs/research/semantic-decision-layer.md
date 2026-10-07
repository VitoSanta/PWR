# Semantic Decision Layer / Semantic-Guided Generation

**HYPOTHESIS — proposed 2026-10-01; not implemented, not measured.** This is
a model-agnostic research proposal, not a promised feature, an implementation
plan or an adopted architectural decision. It changes neither the current
agent loop nor model routing. No model campaign has been run for this proposal.

## Motivation and hypothesis

A small local generative model may produce a useful solution among several
proposals without reliably choosing it through standard decoding. Separating
proposal generation from semantic selection could improve agentic quality,
provided the selection benefit exceeds the cost of generating and scoring
alternatives. Selection cannot repair a candidate pool with no useful proposal.

The hypothesis is that an 8B or 14B generator combined with semantic ranking
and deterministic verification could outperform the same deployment with
standard decoding, and potentially compete with larger vanilla agents at a
lower hardware or computational cost. These benefits are unknown.

The principal experimental question is:

> “8B + semantic decision layer + deterministic verifier può superare
> 14B/30B vanilla agent a parità o minor costo computazionale?”

Parameter count alone does not establish cost or capability; exact artifacts,
quantization, active parameters for MoE models, backend and hardware must be
reported. A cross-family comparison tests deployments, not size in isolation.

## Conceptual architecture

**Generation ≠ Selection ≠ Verification.**

```text
Generator
  → candidate proposals
  → semantic scorer / decision model
  → rank + prune
  → deterministic verifier
  → continue / regenerate
                  ↳ failure evidence returns to the generator
```

- **Generation:** the local model proposes multiple solutions, actions,
  plans, patches or continuation chunks from the same task and observed state.
- **Selection:** a specialized scoring, ranking or structured decision model
  assesses relevance, constraint alignment and likely usefulness. PWR would
  retain the best candidates and discard less promising ones under a bounded
  policy. Scores are fallible estimates, never correctness verdicts.
- **Verification:** tests, compiler, lint, type-checking and explicitly defined
  predicates over tool output check actual effects. A successful command or
  schema-valid call alone does not establish task completion. A failed check
  returns its evidence to the generation loop within the total retry budget.

For actions and plans, deterministic checks can reject invalid or forbidden
proposals before execution, but actual effects and final acceptance still need
verification. Incomplete chunks may support only incremental checks; final
correctness is assessed on the assembled solution. Candidate execution would
need isolated workspace state and the existing permission/sandbox boundaries;
ranking never grants permission to run a tool or treats speculative effects as
observed results.

The scorer is model-agnostic: compare suitable semantic scoring, ranking and
decision models rather than prescribing one. Jev may be an inspiration for
models oriented toward ranking or structured decisions; it is not a required
dependency, selected scorer or evidence of effectiveness in PWR.

## Levels of experimentation

| Level | Candidate unit | Question |
|---|---|---|
| 1 | Actions / tool calls | Does ranking improve tool choice and state-dependent arguments before execution? |
| 2 | Plans / next steps | Does selection reduce dead ends without adding unnecessary planning? |
| 3 | Patches / complete solutions | Does selection retain patches that pass independent acceptance checks? |
| 4 | Generation chunks | Does incremental ranking improve the completed output without destroying coherence? |
| 5 | Semantic beam search | Does retaining multiple trajectories beat simpler selection after all branch costs are counted? |

Start any future study with one unit and bounded candidate/retention counts.
These are experimental levels, not roadmap commitments. Chunk ranking and
semantic beam search require separate studies of partial-state handling,
branch diversity, cache sharing and pruning errors.

## Possible advantages, risks and trade-offs

Possible advantages include better choices from a small generator, fewer
unproductive tool calls or retries, and lower peak memory than a larger
generator. None follows automatically from improved ranking accuracy.

Multiple proposals add generation tokens, prefill and latency. A second model
adds loading time, memory, cache pressure and scoring compute; sequential
loading may trade lower resident memory for higher latency. Beam search may
multiply all these costs. A scorer can prefer plausible but wrong candidates,
share the generator's errors, overfit the corpus, or prematurely prune the
only successful branch. Low candidate diversity limits any possible gain.

Incomplete verifier coverage permits false acceptance; semantic confidence
must never substitute for independent acceptance. Candidate text and repository
content can also steer a scorer through prompt injection. Calibration,
abstention, tie-breaking and the handling of all-low-scoring candidates need
explicit policies. Record negative results and cases where selection hurts.

## Benchmarks and controls

Use the [evaluation rules](../evaluation.md#rules-for-a-claim): frozen,
repository-disjoint development and held-out splits, repeated task/seed pairs,
independent hidden acceptance checks, every assigned trial in the denominator,
and reported human/oracle interventions. Hidden checks must be inaccessible to
both generator and scorer; only visible verifier evidence enters recovery.

Necessary benchmark classes are stateful tool/action choice, multi-step
planning, repository repair with tests, small-project generation, and longer
tasks with recovery. Existing `corpus/small-apps-v1.json`, repair/generation
corpora and the [stack matrix](../evaluation.md#the-stack-matrix) are starting
points, not evidence that these levels are already evaluated. Include failing
and adversarial candidates, distractors, incomplete chunks, correlated model
errors and tasks with weak visible checks but strong hidden checks.

Compare these proposed experimental conditions, not existing CLI arm names:

1. Same 8B/14B generator, vanilla decoding, with the same verification and
   recovery contract.
2. Same generator with multiple candidates and simple/random selection or
   deterministic filtering, without semantic scoring: controls for added search.
3. Same generator and candidate pool with semantic ranking and deterministic
   verification: isolates the value of selection. Sweep candidate count,
   retained count and scorer deployment on development data, then freeze them.
4. 14B/30B vanilla agents without a semantic layer, with the same tools,
   context access, permissions and verification/recovery contract.

Use offline replay of identical candidate pools to diagnose ranking quality,
then end-to-end runs to measure downstream effects. An oracle choice using
hidden checks may estimate pool headroom offline only; it is not a deployable
selector and its feedback must not enter agent runs.

Pin harness, loop, corpus, templates, sampling, reasoning effort, artifacts,
scorer prompt and scoring policy. Report scripted and product-path results
separately because [their semantics currently differ](../agent-loop.md#the-scripted-run).
Do not assume the current evaluation harness implements this experiment.

## Metrics and success criteria

| Metric | Required accounting |
|---|---|
| Task success rate | Independent hidden acceptance; unattended first-cycle and eventual success separately; paired differences and intervals over tasks |
| Verifier pass rate | Visible and hidden checks separately, first attempt and final; false acceptance explicitly |
| Retry count | Generation retries, verifier-triggered recovery and branch attempts separately |
| Generated tokens | All proposals, rejected branches, reasoning and scorer output; also input/prefill tokens per component |
| Latency | Proposal generation, scoring, verification and model loading; median and tail latency |
| Tokens/sec | Generator and generative scorer separately; prefill/decode separately, not an aggregate across tokenizers |
| RAM/VRAM | Peak total and component residency, KV/branch state, offload/swap; unified memory counted once |
| Tool call count | All executed calls, including speculative and failed branches; proposed calls separately |
| Total time | End-to-end wall time including loading, scoring, checks and every failed attempt; cold and warm runs |
| Scorer computational cost | Calls, candidates scored, CPU/GPU time, memory and energy when measurable; unavailable measurements marked unknown |
| Relative quality/cost | Same-size vanilla and larger vanilla controls; total cost and cost per successful task, including unsuccessful trials |

Computational cost is not interchangeable with parameter count, generated
tokens or wall time. Before a confirmatory campaign, predeclare the primary
resource budget (for example total accelerator time, with a peak-memory cap),
measurement method and hardware. Report the other resource dimensions so a
memory saving cannot conceal an unbounded time or energy cost.

Proposed decision criteria, to freeze before held-out evaluation:

- **Selection benefit:** task-success uplift over same-size vanilla and the
  no-semantic multi-candidate control at matched total resource budgets, with
  the paired interval excluding zero. A gain obtained only by spending more
  compute establishes a search trade-off, not greater efficiency.
- **Principal question:** the 8B condition exceeds each claimed 14B/30B vanilla
  control's task success, with a positive paired interval, while staying at or
  below its predeclared computational cost and memory cap. Noninferiority at
  lower cost is a separate useful result; fix its margin in advance and do not
  call it superiority.
- **Guardrails:** no increase in false acceptance or policy violations; count
  timeouts and backend/scorer failures as outcomes. Predeclare latency and
  memory limits and the sample size required for the chosen minimum useful
  effect. An underpowered or mixed result remains inconclusive.

No numeric acceptance threshold or product gate is adopted by this proposal.
Publish per-deployment results, artifacts and ablations, including failure
cases; record any future campaign in the [experiment log](../experiment-log.md).

## Open questions and relation to existing ideas

- Which scorer predicts actual success rather than persuasive prose, and how
  well does it transfer across generators, languages and repositories?
- Which candidate unit offers the best benefit per unit of total compute?
- How many diverse proposals are needed, and when should the scorer abstain
  or request regeneration rather than select a weak candidate?
- What task/context/verifier evidence should scoring see, at what context cost?
- Can partial chunks be ranked reliably without evaluating a full continuation?
- Does beam search add value beyond bounded patch or action ranking?
- How do loading, shared caches and resident memory affect the break-even point?
- Can pruning errors and scorer failures be detected without relying on the
  same scorer to judge itself?

This extends the verification/recovery feedback described in
[Goal mode](../agent-loop.md#goal-mode) and
[verification](../verification.md), but proposes a distinct selection phase
before verification. The current same-model review round is not evidence for
multi-candidate semantic ranking. [Model calibration](../models.md#quick-calibration)
checks compatibility, not selection quality or agentic uplift.

The archived [research hypotheses H6 and H7](https://github.com/VitoSanta/PWR/blob/6da514ab/docs/archive/local-agent-research.md)
concern adaptive policy selection and scoped workers; they share the need for
strong fixed controls and complete overhead accounting, but do not establish
this mechanism. Archived [model selection/routing](https://github.com/VitoSanta/PWR/blob/6da514ab/docs/archive/adaptive-runtime/MODEL_SELECTION_AND_ROUTING.md)
selects a deployment, whereas this proposal selects proposals produced by a
generator. The [current routing decision](../decisions.md#d-2026-09-30-3--what-is-not-built-now)
remains unchanged; this research note neither enables routing nor requires it.
