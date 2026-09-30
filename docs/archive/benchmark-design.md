> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Benchmark Design — the capability suite PWR needs

Source: a design guide written by the project owner on 2026-09-04, recorded here
in full so it survives the conversation it arrived in. Sections marked
**Assessment** are this project's response, including where the guide is not
being followed and why. The guide is not treated as settled: a reader who
disagrees with an assessment should be able to re-open it from what is written
here.

## The thesis

A benchmark that scores only the final code cannot separate three different
failures:

1. limits of the local model,
2. limits of the harness,
3. limits of the tools and context handed to the model.

An agent can fail a bugfix because the model does not understand the code — or
because the harness gave it the wrong files, compacted the context badly, would
not let it re-run the tests, or truncated the shell output.

**Assessment: accepted, and already measured.** Six campaigns on 2026-09-04
produced four findings of exactly this shape, each one a count that could not be
acted on: `tool_failures: 9` could not say whether a tool broke or a test the
agent ran on purpose exited non-zero; `malformed: 20` could not say whether the
deployment mis-used a schema or was refusing an attack; `["NOTES.md"]` could not
say whether it obeyed an injected instruction or neutralised it. The first
harness torture test written under this guide found a fifth within ten minutes.

## Macro-areas

| ID | Area | ID | Area |
|---|---|---|---|
| A | Instruction following | N | Database |
| B | Italian / natural language | O | Concurrency / async |
| C | Code comprehension | P | Performance |
| D | Repository navigation | Q | Security |
| E | Bug fixing | R | DevOps / infrastructure |
| F | Root cause analysis | S | Dependency management |
| G | Code generation | T | Git awareness |
| H | Feature modification | U | Tool use |
| I | Refactoring | V | Error recovery |
| J | Testing | W | Long context |
| K | Multi-file / multi-module | X | Ambiguity |
| L | API / backend | Y | Planning |
| M | Frontend | Z | Autonomous long-horizon |

Coverage as of 2026-09-05: C, E, G, I, K and Q, plus **J**, **F**, **V**, **P**
and **D** added that day. Fifteen of twenty-six areas remain with no task.

**J — testing.** `cover-the-contract` is the first task whose product is a check
rather than a change: the implementation is correct and must not be touched, and
the work is writing the tests that cover its contract. Coverage is judged by
inspecting what was written rather than by mutation, because a verifier is one
command and mutation needs a second build of the crate under a changed source.
That is a weaker check and it is recorded as one in the task's provenance.

**D — navigation, with distractors.** `which-implementation-runs` puts four
definitions of one function in a tree of thirty modules: a superseded copy, an
experiment, one a fixture uses, and the one the reader is actually built from.
The defect is in the last. Rewriting all four passes the stated contract and
fails the hidden check, because the work is finding which one runs and a change
to the other three is a change made without knowing.

**V — error recovery.** `the-fix-breaks-the-build` states a one-line rename and
breaks the build in two files the statement never mentions. The edit is trivial;
the work is reading the compiler and finding what it broke. Nothing else in the
corpus exercises the recovery loop, which has a budget and a cycle limit that
have never been measured against a task.

**P — performance.** `quadratic-duplicate-scan` is judged by an operation
counter, not by wall clock: a timing threshold on a shared machine is a flaky
verifier and a flaky verifier is worse than none. The hidden check also refuses
the obvious shortcut — a run that stops calling the counter has not made the
scan cheaper, so a scan of two thousand distinct values reporting zero
comparisons fails.

**F — root cause analysis.** `diagnose-do-not-fix` is the first task whose
correct action is to change nothing. A failing test names `summary.rs`; the
defect is one call away in `window.rs`. It is scored on the rationale naming the
right file and on the workspace being untouched, so its visible and hidden
verifiers are the same failing test — it must still fail at the end.

It is also the first task to stop discriminating, and how it stopped is the
lesson: five files that all reach the prompt, a comment above the defect naming
it, and one substring — `window` — deciding the answer. A deployment can resolve
it having read nothing. `diagnose-two-calls-away` is the same shape built to
avoid that. Thirty-three files, so lexical retrieval's five passages cannot
carry the tree; the defect two calls from the symptom the statement names, in a
function nothing in the statement mentions; no comment above it; three plausible
wrong answers on or beside the path; and an answer that must contain the file
*and* the function, so naming the module is not enough. The requirement is a
conjunction, which is what `ExpectedAnswer::All` is for. It is still lexical,
and a rationale that names the right function while misdescribing it still
resolves.

Measured on 2026-09-06, it does not discriminate either: six of six on the three
MVP deployments. It is harder — every trial spent three to seven actions and
read the file, where the old task had a trial resolve in one action having read
nothing — but the rate is the same, and a task all three pass separates nothing.
The reason is visible in the retrieval, and it is not the deployments: the five
lexical passages ranked against the statement include `src/rollup/daily.rs`,
which is the intermediate hop, and its six lines name `money::scale` and
`to_cents`. Two calls from the symptom collapses to one read from the prompt.
So `diagnose-past-the-delegation` keeps the name off the retrieved path: the
hop calls `money::convert::to_minor`, which delegates to `money::scale::to_cents`,
and the retrieval was reproduced beforehand to confirm that none of the five
passages it returns contains `scale` or `to_cents`.

Six of six again, and the traces say why. Every trial listed the tree, then
opened `src/money/convert.rs` and `src/money/scale.rs` directly. The names are
the answer: a thirty-four-file tree is enumerated in one action, and a module
called `money/scale` is where a deployment looks for a rounding defect whether
or not anything pointed at it. Depth is not the barrier. What the indirection
did change is cost — five to fifteen tool actions and seven to twenty-one turns,
against three to seven actions before — so the pair measures what an indirection
costs, not who can cross it.

So the third attempt left the synthetic tree entirely.
`external-running-min-stability` is set in more-itertools at the parent of the
upstream fix `d992be0`: `running_min` disagrees with `min()` on values that
compare equal, and the defect is a strict comparison in the private helper
`_windowed_running_min`, whose name appears nowhere outside a 1621-line file
that does not arrive in one read. The harness's own fairness check established
the shape before the task was measured — the project's suite passes at the
pinned commit, the declared hidden regression fails there and passes at the fix.

Six of six again, and this time the traces show the work being done: every
trial searched `def running_min`, read the region of `recipes.py` it returned,
and answered in three to five actions. Three task shapes — the answer in the
prompt, the answer in the module names, the answer behind a search in a real
repository — and the same rate each time.

**Area F is saturated on resolution for this cohort, and what separates these
deployments is cost.** For the same correct answer to the same task: 33–43
seconds and about 700 generated tokens for one deployment, 175–316 seconds and
1,800–4,000 for another, 168–172 seconds and 4,800–5,800 for a third, with peak
prompts from 4,628 to 21,226 tokens. Two trials each is a range, not a ranking,
and it is a wider spread than resolution shows at all. Score this area on
actions, tokens and evidence carried to the same verified answer; keep
resolution as the gate that says the answer was reached.

It is a spread of this task, though, and the same day said so: priced across
three repair tasks, the deployment dearest here is the cheapest there, and the
ratio between dearest and cheapest falls from 6.9× to between 1.8× and 3.3×.
Cost separates deployments; which deployment it separates out depends on what
is being asked, so a cost measured in one area is not a property of the cohort.

## The difficulty ladder

| Level | What the task gives |
|---|---|
| EASY | File named, error named, test named |
| MEDIUM | Feature named, file unknown, repository to explore |
| HARD | User-language description, no file, no test, cross-module |
| AGENTIC | Ambiguous problem, large repo, tool failure, dirty working tree, partly failing tests, stale documentation, several cycles needed |

The expected shape of a result is a curve, not a number: `Bugfix Easy 94% /
Medium 81% / Hard 59% / Agentic 32%`. Where it falls off is where the harness
has to compensate for the model.

**Assessment: adopted, and it explains a measured failure.** `m6-hard-v1` was
authored to be harder and resolved 7 of 7. Read against this ladder the seven
tasks are EASY and MEDIUM — every one is a repair inside a well-formed workspace
with a red test pointing near the defect. The axis that discriminates is HARD
and AGENTIC, and nothing in either corpus reaches it.

## Failure classification

Do not record `FAIL`. Classify it:

```
MODEL_REASONING_FAILURE          TOOL_SELECTION_FAILURE
MODEL_CODE_KNOWLEDGE_FAILURE     TOOL_SCHEMA_FAILURE
MODEL_INSTRUCTION_FAILURE        TOOL_RESULT_INTERPRETATION_FAILURE
CONTEXT_MISSING                  PLANNING_FAILURE
CONTEXT_OVERLOAD                 PATCH_FAILURE
CONTEXT_LOST                     VERIFICATION_FAILURE
SEARCH_FAILURE                   RECOVERY_FAILURE
RETRIEVAL_FAILURE                STOPPING_FAILURE
                                 HARNESS_ORCHESTRATION_FAILURE
```

The argument: a challenger that fails a bugfix because it could not find a class
looks like a weaker model. If the record says `SEARCH_FAILURE`, PWR can fix
it with symbol search, reference search, dependency traversal or semantic
retrieval — without changing the model. That is where the project's advantage
lies.

**Assessment: accepted as the most valuable section, with one load-bearing
change.** The kind must be decided **where the fault is found**, never recovered
afterwards by a classifier reading the log. This is already the rule in code:
`MalformedCall::kind` is set at the point of failure, because "a message is
written for a person to read, and classifying on it makes rewording one a silent
change of measurement". A post-hoc classifier inherits the classifier's errors
and reports them as measurements. So `SEARCH_FAILURE` is emitted by retrieval,
`CONTEXT_LOST` by the compactor, `PATCH_FAILURE` by the patcher.

## Metrics per run

`TaskSuccess`, `BuildSuccess`, `TestSuccess`, `Regression`, `FilesTouched`,
`UnnecessaryDiff`, `ToolCalls`, `InvalidToolCalls`, `CommandFailures`,
`RecoveryRate`, `SearchPrecision`, `ContextTokens`, `OutputTokens`, `Turns`,
`RepeatedReads`, `RepeatedCommands`, `Hallucinations`, `UserChangesDestroyed`,
`TimeToFirstCorrectHypothesis`, `TimeToSolution`, `TestBeforeFix`,
`VerificationQuality`, `StopQuality`.

**Assessment: adopted.** Roughly half exist already. Missing and cheap:
`SearchPrecision`, `RepeatedReads` (the read-history exists and is used for
annotation, not counted), `RepeatedCommands`, `UserChangesDestroyed`,
`TestBeforeFix`. Missing and needing definition: `VerificationQuality`,
`StopQuality`, `TimeToFirstCorrectHypothesis` — each needs a determinate rule
before it can be a number.

## Composite scores — not adopted

The guide proposes ten categories scored 0–5 for a total out of 50:
Correctness, Repository understanding, Autonomy, Tool efficiency, Context
efficiency, Diff quality, Verification, Instruction following, Recovery,
Hallucination.

**Assessment: declined, and the raw metrics kept instead.** PWR's load-bearing
claim is that nothing is asserted that is not verified: every number comes from
a deterministic check and every proportion carries a Wilson interval. A 0–5
Correctness score needs a judge. If the judge is a model, every figure is only
as good as the judge, the intervals become decoration, and the property that
makes this project worth more than a leaderboard is gone. The capability matrix
can be built from the verifiable metrics without a composite.

This is a disagreement with the guide, not a deferral. Reopening it means
answering: who judges, and what makes the judgement checkable.

## Harness torture tests

Tests of PWR rather than of the model. None needs a model to run.

| ID | What it does | Status |
|---|---|---|
| H01 | A test run printing 20,000 lines that fails on the last | **done — found a defect** |
| H02 | A word appearing 5,000 times: does search narrow, or dump? | **done — found a defect** |
| H03 | An 8,000-line file: are only the relevant regions returned? | **done — already correct** |
| H04 | After ~30 tool calls, is the original requirement still held? | **done — the harness half; the model half rides on the corpus** |
| H05 | Forced compaction: are goal, constraints, files discovered, root cause, changes made, tests run, remaining work and failed approaches preserved? | **done — six of eight** |

**H02, measured 2026-09-04.** The bound was never the problem: `search` stops at
the match cap and at the output limit, so nothing was ever dumped. What it could
not do was say so. It returned a bare `Vec`, while every other tool result in
the crate reports its own bounding — a file read carries `truncated` and
`total_lines`, a command carries `stdout_truncated`, a fetch carries
`truncated`. So a query matching five thousand lines came back as two hundred
and read exactly like a query that matched two hundred, and a run concluding
"the symbol is used in these places" was misled by the harness while looking
entirely sound. `SearchResult` now carries `truncated`, set wherever the walk
stops for a reason other than running out of files.

**H03, measured 2026-09-04: already correct.** An 8,000-line file comes back
bounded, reports `total_lines` so the deployment knows to ask for more, and
honours the window it is given. A torture test that passes is still a
measurement — this one says the retrieval defect is in search, not in reading.

**H04, measured 2026-09-04.** The guide asks this of the model. The harness half
comes first, because a deployment cannot hold a requirement the harness has
stopped sending, and the failure mode is not hypothetical: `compact_history`
carries a comment about a fixed index that "silently replaced the real task"
with the session ledger, and nothing held it to that afterwards. A run whose goal
has been quietly swapped for a summary of its own actions still looks like a
run; it answers a different question.

Thirty tool calls, a 2048-token context so compaction runs repeatedly, and the
original requirement asserted on **every** turn rather than the last — a goal
that survives to the end but went missing in the middle was missing for the
decisions taken there. It holds. The fixture was then checked by reintroducing
the historical off-by-one, and both assertions fail with it and pass without.

The model half needs no separate task: `deep-defect` states a constraint at the
start ("every other transform is correct and must not be altered") that only
bites twenty turns later, and its hidden verifier checks it.

**H05, measured 2026-09-04: six of eight.** The ledger preserves the goal, the
constraints, the files discovered with their hashes, the changes made with the
hash a later edit must quote, the tests run with their exit codes, the actions
already refused, and the outstanding plan steps. Two are absent: **the root
cause**, because nothing in a run records a diagnosis — there is no capability
that says "the defect is here" — and **an edit that turned out wrong**, which is
recorded as a change like any other. Neither is being built on this evidence
alone: `multiple_calls` was a lesson in deciding from too little.

**H01, measured 2026-09-04.** `read_bounded_pipe` kept the first `output_limit`
bytes and discarded the rest. Every test runner puts its verdict at the end, so
a suite that printed 20,000 passing lines and failed on the last reached the
deployment as 20,000 passing lines, ending at case 386 — and was then asked to
diagnose a failure it had never been shown. Both ends are now kept and the gap
says how many bytes it swallowed. See `crates/pwr-tools/tests/harness_torture.rs`.

## Fabricated Evidence Rate

Put a repository with no HTTP retry in front of the agent and ask where retry is
implemented, instructing it to say so if it is not there. A run that answers
"`RetryService` implements it" when no such symbol exists has fabricated
evidence. Rate = fabrications / opportunities.

**Assessment: adopted, built, and deterministic.** A task names the symbols the
repository does not contain in `forbidden_in_rationale`, and a rationale
containing one has fabricated evidence — the repository settles it, not a judge.
Reported as `fabricated_evidence_rate` over the tasks that could have
fabricated, so a corpus with no such task reports no rate rather than a clean
one. `no-retry-exists` is the first: an HTTP client with no retry, no backoff
and no circuit breaker, asked where retry is implemented and told that "there is
none" is an acceptable answer — because a task that does not say so measures a
trick rather than a tendency.

No bar. A bar derived from the corpus it judges is the failure the pilot
separation exists to prevent, and a bar of 0 could be falsified but never met,
so it would say less than the rate itself.

## Framework and language sweeps — blocked

The guide proposes the same semantic task across 19 backend frameworks, 12
frontend frameworks and ~30 languages, so that "Qwen is weak at Spring Boot" can
be told from "the Spring task was harder". The cross-language LRU cache in §28 is
the cheapest form of it.

**Assessment: correct in principle, blocked in practice, and the guide does not
price it.** Each stack needs a toolchain. The sandbox denies network; the only
way to install one is `--provision`, which grants network and arbitrary
executables together — the pair the roadmap already records as the unresolved
risk. Tens of gigabytes of toolchains, and a corpus whose tasks cannot be run by
anyone who has not installed them, is a reproducibility cost as well as a
storage one. Blocked until either a per-language container image is pinned, or
`--provision` is split.

## The cost the guide does not price

Every task in a PWR corpus needs a **reference solution**, run against both
verifiers before the task is admitted — see `crates/pwr-eval/tests/corpus_is_sound.rs`.
This is not optional bureaucracy: it caught `deep-defect` while its pipeline
made the defect unreachable by the visible test, and it caught `build-ledger`
having never been validated at all. Without a reference, a task can be
unsolvable and merely look hard.

Parameterised templates generate prompts. They do not generate references. A
345-task corpus is 345 reference solutions, each one proved.

## First AGENTIC campaign — 2026-09-04

Thirteen tasks, four carrying an adverse condition. 12 of 13 resolved, which is
the first corpus here that did not resolve everything.

The three conditions that held are worth naming because each is a failure mode
the guide predicts and this deployment did not have: it left someone else's
unfinished work alone rather than tidying it, it followed the executable code
over a README that contradicted it, and it answered "there is none" instead of
inventing a `RetryService`. `UserChangesDestroyed` measured zero; the Fabricated
Evidence Rate measured 0 of 1, interval `[0.000, 0.793]`, which says only that
fabrication is not obviously common.

The one failure was not one of the four evasions its task was built to catch. It
introduced an edge-case bug while fixing an edge case, which is `E04` in this
guide and is exactly what a hidden verifier exists for.

**What this says about the ladder.** The AGENTIC row discriminates where MEDIUM
and HARD did not, on one seed. But three conditions out of four were handled
cleanly, so the failure came from the ordinary difficulty of the repair rather
than from the adversity around it. Whether adverse conditions are the axis, or
merely the first corpus to contain a hard enough repair, needs more seeds than
one.

## Sequencing

1. **Harness torture tests** (H02–H05) and the Fabricated Evidence Rate.
   Deterministic, no references, no new toolchains, and nothing here is measured
   today. Chosen first on 2026-09-04.
2. **Failure taxonomy**, instrumented at each fault site.
3. **Difficulty ladder**, pushing the existing corpora to HARD and AGENTIC.
4. **Missing metrics** that already have a determinate rule.
5. **Sweeps**, once the toolchain question is answered.

## The failure taxonomy, built — 2026-09-05

The guide's most valuable section, implemented with the change recorded in its
assessment: every kind is decided where the fault is found, never by a
classifier reading the log afterwards.

**Three vocabularies already existed** and between them cover most of what §35
asks for, each written at the point of failure:

| Where | Kinds |
|---|---|
| How the run ended | `provider`, `timeout`, `budget`, `no_verifier`, `protocol`, `recovery`, `interrupted`, `declined`, `unclassified` |
| How a tool attempt ended | `allowed_success`, `allowed_failure`, `policy_denial`, `io_failure`, `protocol_failure`, `timeout` |
| Why a turn produced no call | `no_tool_call`, `unknown_capability`, `schema_mismatch`, `argument_shape`, `no_arguments`, `invalid_action`, `unparsed_output`, `multiple_calls` |

**What was missing is the roll-up**, and it is the thing that mattered: *whose
failure is this*. That question was asked by hand five times in two days and
answered wrongly on the first pass every time.

| Presented as | Was |
|---|---|
| A broken action channel | Refusals of attack tasks with no way to refuse |
| A deployment concluding too early | A search that could not report its own cap |
| A deployment that could not diagnose | An output bound that discarded the verdict |
| A deployment inventing capabilities | Tool calls echoed back without their names |
| A flaky backend | An unparsable generation handled on one path of two |

`attribute()` assigns a failed run from what the run recorded and nothing else —
no inference from messages, timing or the shape of the output. Owners are
`Harness`, `Backend`, `Corpus`, `Deployment`, and:

**`Unattributed`, which is deliberately not "the deployment".** Attributing an
unexplained failure to the model is the assumption that was wrong five times
running. It is a verdict that means *go and look*, and all five of those defects
would have landed there. Two metrics report it: `failures_owned_by_the_harness`
and `failures_unattributed`, both without a bar, because a bar on either would
create a reason to attribute a failure somewhere else.

The one judgement encoded rather than read is the budget: exhausting one is the
harness's wall when the repository checks were passing at the time, and the
deployment's otherwise. That distinction came from a measurement — nemotron
exhausted twelve actions having finished the work and been unable to say so.
