# Verification of the 2026-09-30 technical review

**Checked 2026-09-30 against `develop` at `0776ff4f`** — two commits after the
reviewed `efdd2798` (`1d657268` holds a conversation's `complete` once when
nothing was written, run or read; `0776ff4f` records the small-model
campaigns). Nothing was taken on the review's word: every claim that names
code was opened at the current revision, and every number that names an
experiment was found in the experiment's own report.

Verdicts: **confirmed** (the code or artifact says so), **confirmed, wider**
(true, and the defect reaches further than the review said), **corrected**
(partly wrong, with the correction), **not checkable** (a judgement or
forecast, recorded as such). Line numbers are current.

The review itself is [2026-09-30-technical-review.md](2026-09-30-technical-review.md).
What is done about each finding is in the
[implementation plan](../plan/implementation-plan.md), by the work-item ID in
the last column.

## Test suite, reproduced

`cargo test --workspace --no-fail-fast` on the maintainer's M2 Max, the same
day: **exit 101, one failure**, the same one the review saw —
`docker_reaches_its_daemon_only_when_the_engine_is_granted`
(`crates/pwr-tools/tests/sandbox_and_approvals.rs:1516`). After the grant,
`docker version` answered *"Cannot connect to the Docker daemon at
unix:///Users/vitosantanelli/.docker/run/docker.sock. Is the docker daemon
running?"*.

**Cause established:** Docker Desktop was not running, and its socket file was
still on disk from an earlier session. The test's guard,
`pwr_tools::container_socket()` (`crates/pwr-tools/src/lib.rs:842`), accepts a
candidate on `path.exists()`, so a stale socket file counts as "an engine is
running". The test then asserts the daemon answers. It is an environment
failure reported as a test failure, not a sandbox escape. → **W0.2**.

Desktop and sidecar suites were not re-run for this record; the review's
counts (94 desktop tests in 12 files, 37 sidecar tests) are the latest.

## Claims, section by section

| # | Review claim | Verdict | Evidence at `0776ff4f` | Plan |
|---|---|---|---|---|
| 2.1 | Post-turn verification runs in the caller, after `take_turn` returns | confirmed | `crates/pwr-cli/src/main.rs:4851-4926` (`run_chat_turn`): `take_turn` at 4833, checks at 4855+ | W2.3 |
| 2.2 | Goal mode is an outer loop in `serve.rs` | confirmed | `crates/pwr-cli/src/serve.rs:1966-2236` | W2.3 |
| 2.3 | The scripted runner has its own verification and recovery | confirmed | `run_action_loop_with_prompt_budget_and_context_tiers`, `crates/pwr-orchestrator/src/lib.rs:2936` | W2.4 |
| 3.1 | CLI layer owns product behaviour; file sizes | confirmed | `main.rs` 13,196 lines, `serve.rs` 6,131, orchestrator `lib.rs` 8,731, `converse.rs` 3,475 (`take_turn_inner` from 893). Counts include tests and comments | W2.3, W10.4 |
| 3.2 | Two loops with different planning, completion, compaction, recovery, catalogue | confirmed | README and `docs/release/v0.2.x-mac-verification.md` ("some fixes exist in one only"); conversation catalogue drops `record_progress`, `propose_verifier` and adds `remember`, `recall_project`, `wiki_query`, `look_at` (`converse.rs:619-645`) | W2.4 |
| 3.3 | `pwr-models` depends on the orchestrator for window arithmetic | confirmed | `crates/pwr-models/Cargo.toml:14`; `crates/pwr-models/src/fit.rs:30` uses `pwr_orchestrator::window` | W2.5 |
| 3.4 | Overlapping state without a clear ownership hierarchy | confirmed | Event log, checkpoints, snapshots, ledgers, wiki, graph, summaries, project registry, embeddings, index — mapped in [state-and-persistence.md](../state-and-persistence.md) | W6.2 |
| 4.1 | `complete` behind other calls of the same reply is refused | confirmed | `COMPLETION_OVER_UNSEEN_RESULTS`, `converse.rs:2482`; since `1d657268` also `COMPLETION_WITH_NOTHING_DONE`, `converse.rs:2476` | — |
| 4.2 | P1: `GOAL_MAX_ACTIONS = 208` is checked only in the branch after `if report.completed` | confirmed | const `serve.rs:450`; check `serve.rs:2182` is an `else if` of `if report.completed` (2028). A goal whose completions are refused with *alternating* failure sets never reaches it; there is no wall-clock limit | W1.4 |
| 4.3 | P1: compaction keeps the first request to 800 chars, later requests to 200-char lines, at most 12 | confirmed | `FIRST_REQUEST_CHARS`, `MAX_REQUESTS`, `LINE_CHARS`, `crates/pwr-orchestrator/src/compaction.rs:48-54`. The checkpoint keeps a revision *number* for steering, not the objective's text (`conversation.rs:52`) | W4.1 |
| 4.4 | P2: many detectors, each with its own limit, no shared recovery budget | confirmed | `converse.rs:60-131` (empty turns 3, unparseable 3, compactions 2, check-in 26, backend faults 3, finalization retries 1), `repetition.rs:21-193` (refusals 3, echoes 3/10, repeated failure 2, failed runs 5), `stall.rs` `NO_PROGRESS_LIMIT` | W2.6 |
| 5 | Small-model reality check | not checkable | A design judgement; the plan adopts its target (7B–14B) and its rule (a ~35B result is not evidence for a 9B) | W8.4 |
| 6.1 | Quick Calibration: at most nine requests; its own code says it measures neither coding nor long context | confirmed | `crates/pwr-models/src/calibration.rs:1-27` (`MAX_REQUESTS = 9`). Suite is now `quick-calibration-5` (`crates/pwr-models/src/profile.rs:33`) | W10.1 |
| 6.2 | `verified-models.json` is empty | confirmed | `crates/pwr-models/verified-models.json`: `"entries": []` | W10.1 |
| 7.1 | Tokens estimated as `len()/4` bytes | confirmed | `CHARS_PER_TOKEN = 4`, `crates/pwr-orchestrator/src/context.rs:129`, `lib.rs:2326`. The conversation does use the engine's own count of the last request as a base and a learned constant overhead (`context.rs:131-150`); the estimate covers what was appended since | W4.2 |
| 7.2 | `context::compile` keeps required sections over budget with no refusal | confirmed | `context.rs:521-651`: required sections are never cut, and `CompiledPrompt` carries no over-budget flag | W4.3 |
| 7.3 | Two estimates (`/4` and a conservative `/3`), no single exact preflight | confirmed | `conservative_prompt_tokens`, `converse.rs:2448`; `prompt_tokens_now`, `converse.rs:2537` | W4.2 |
| 7.4 | Index cache keyed on mtime+size | confirmed | `walk_with_cache`, `crates/pwr-repo/src/lib.rs:509-556` | W6.2 |
| 7.5 | The indexer reads a whole file before applying `MAX_INDEXED_BYTES` | confirmed | `fs::read(path)` at `pwr-repo/src/lib.rs:558`, bound checked at 560 | W1.7 |
| 7.6 | Retrieval errors degrade silently to empty | confirmed | `retrieved_context` ends in `.unwrap_or_default()`, `context.rs:495` | W4.6 |
| 7.7 | Angular guidance says "no tool renders or screenshots it" while `look_at` exists | confirmed | `context.rs:384`; `look_at_tool`, `converse.rs:649`, offered to vision models (`main.rs:4813`) | W4.5 |
| 7.8 | Graph relies on path resolution and naming conventions | confirmed | `Certainty::{Fact, Resolved, Named, Guess}`, `crates/pwr-orchestrator/src/graph.rs:58-343` | W7.2 |
| 8.1 | P1: protected paths are enforced for file tools, not in the Seatbelt profile | confirmed, wider | `refuse_if_protected`, `crates/pwr-tools/src/lib.rs:1984`, checks `.pwr/protected.json` paths and installed dependency trees; `sandbox_profile`, `lib.rs:2092-2375`, denies writes only to `.pwr` and `.git/hooks`. Wider: in **Full access** mode there is no profile at all (see N3) | W1.3 |
| 8.2 | P1: `own_overwrite` computes the current hash at execution time | confirmed, wider | `own_overwrite`, `converse.rs:2498-2510`: **any** existing file, not only one the conversation wrote — the `written` map (`converse.rs:494`) is not consulted, and the file need never have been read. Mitigation that exists: an in-memory, bounded copy for rewind (`FileEdit`, `converse.rs:503-560`), lost when the session ends and `NotKept` past its bounds | W1.1 |
| 8.3 | P1: writes are not atomic | confirmed | `std::fs::write` after read/check: `pwr-tools/src/lib.rs:3493` (apply_patch), `3927`, `4067`, `5036`; `rename` is used only for moves (3713) and downloads (4559) | W1.2 |
| 8.4 | Docker socket grants daemon authority beyond the sandbox | confirmed | Stated in `SECURITY.md`; `container_socket`, `lib.rs:842` | W1.9 (disclosure) |
| 8.5 | LocalService is not strict loopback | confirmed | Comment and profile at `lib.rs:2277`: Seatbelt accepts only `*` or `localhost` | W1.9 (disclosure) |
| 8.6 | PDF streams are inflated with an unbounded `read_to_end` | confirmed | `inflate`, `crates/pwr-tools/src/document.rs:232-250` | W1.6 |
| 9.1 | CI discovery is textual and heuristic | confirmed | `ci_declared_checks`, `crates/pwr-verify/src/lib.rs:438-529`: `split_whitespace`; steps with `&&`, `|`, `>`, `$`, `{}` skipped; no `working-directory`, `env` or multi-line `run: |` semantics | W3.2 |
| 9.2 | Goal acceptance hashes `.pwr/checks.json` only, not what the checks execute | confirmed | `acceptance_contract_hash`, `serve.rs:723-731`; compared in `verify_goal`, `main.rs:3743-3744`. Tests and scripts a check runs are protected only if listed in `.pwr/protected.json` | W3.1 |
| 9.3 | The stack-matrix runner restores the owner's tests in its verification copy | confirmed | `evidence/stack-matrix/runner/run.py:153-159` | W3.1 |
| 9.4 | Scripted runs may end `TaskComplete { verified: false }`, contradicting documents that say no-verifier completion is refused | confirmed | `crates/pwr-orchestrator/src/lib.rs:3783-3815`. Contradicting text: archived `PWR_PRODUCT_SOURCE_OF_TRUTH.md:1104`, `verification-recovery.md:45`, `experiment-log.md:720`, `milestones.json` M4 | W2.1, W3.4 |
| 10.1 | Reproduction tells non-determinism by exit code only | confirmed | `classify_with_reproduction`, `pwr-verify/src/lib.rs:1002-1015` | W3.3 |
| 10.2 | Failure classification is pattern-based | confirmed | `DENIED`, `MISSING` and `outside_warnings`, `pwr-verify/src/lib.rs:1023-1060` | W3.3 |
| 10.3 | Goal compares failures by check name, which can stop a goal that is progressing | confirmed | `verification.failing` is a list of command strings (`main.rs:3756-3773`); `same_failure` compares it (`serve.rs:2139`) | W1.5 |
| 11.1 | Cancellation is not checked during the sidecar's manual prefill | confirmed, wider | `Engine.prefill`, `crates/pwr-mlx/sidecar/pwr_mlx.py:734-743`, has no cancel check; the generation path checks `cancelled()` only per generated token (`pwr_mlx.py:925`), so `stream_generate`'s own prefill is not interruptible either | W5.1 |
| 11.2 | One generation at a time; idle summaries can occupy the engine | confirmed | `summarise_while_idle`, `serve.rs:2781`, runs after every non-chat turn (`serve.rs:2771`); it checks for a busy session between modules, but a summary in flight is not cancelled | W5.2 |
| 11.3 | `prepare_context` records the window and returns it without checking the request fits | confirmed | `crates/pwr-mlx/src/lib.rs:1474-1485` | W5.3 |
| 11.4 | llama.cpp forces `tool_choice = required` whenever tools are sent | confirmed | `chat_completions_body`, `crates/pwr-llama/src/lib.rs:150-155` | W5.4 |
| 11.5 | Sampling is `BTreeMap<String, Value>` | confirmed | `crates/pwr-domain/src/lib.rs:1442`, `596` | W2.7 |
| 12.1 | `IndexCache::retain` is linear in the present list | confirmed | `present.contains(&path)` on a `&[String]`, `pwr-repo/src/lib.rs:477-496` — O(n·m) | W1.7 |
| 12.2 | Wiki/graph rebuilt after turns; full snapshots per turn | confirmed | `record_snapshot` appends every message, `crates/pwr-orchestrator/src/conversation.rs:107-120` | W6.3 |
| 12.3 | Embedding cache without eviction | confirmed | `EmbeddingRanker`, `crates/pwr-cli/src/semantic.rs:20-70`: one JSON map per model, nothing removed. Semantic retrieval is opt-in (`PWR_SEMANTIC_RETRIEVAL=1`, `main.rs:9452`) | W6.5 |
| 12.4 | `Embedder::read_line` blocks with no timeout | confirmed | `crates/pwr-mlx/src/embed.rs:138-155` | W1.8 |
| 12.5 | Window uses total memory minus a heuristic reserve | confirmed | `default_reserve_bytes`: a quarter of memory, at least 8 GiB, `crates/pwr-orchestrator/src/window.rs:53` | W4.2 (docs), W8.7 |
| 13.1 | Post-turn note is `✓` even when the verdict is a failure | confirmed | `steps(TurnStep::Note(format!("✓ {verdict}")))`, `main.rs:4922` — also for "unavailable" and "could not be run" | W2.2 |
| 13.2 | 3D graph adds dependencies without evidence of value | confirmed (dependency); value not checkable | `3d-force-graph`, `three-spritetext` in `apps/desktop/package.json`; `pa-graph3d` in `apps/desktop/src/app/ui/workbench/knowledge.ts:86-91` | W7.2 |
| 15.1 | `ChatMessage.role` is a string | confirmed | `crates/pwr-domain/src/lib.rs:1559` | W2.7 |
| 15.2 | `Store::append` reads the last hash then inserts, with no transaction; no WAL, busy timeout or indexes | confirmed | `crates/pwr-store/src/lib.rs:188-230`; no `PRAGMA` or `CREATE INDEX` in the crate | W6.1 |
| 15.3 | Hash chain detects alteration against itself, not suffix truncation or a rewritten chain | confirmed | `verify_run_chain`, `pwr-store/src/lib.rs:100-155` | W6.1 (naming) |
| 15.4 | Selection/certification needs a reachability audit | not checkable yet | `pwr models select|certification|certify` are reachable from the CLI; whether the app or any decision reads them is the audit's question | W10.1 |
| 16.1 | Host-dependent tests `return` early and count as passes | confirmed | `sandbox_and_approvals.rs:1265, 1321, 1357, 1518, 1570`; `local_service.rs:114, 131`; `provisioning.rs:157, 166`; `live_model.rs:31` | W0.2 |
| 16.2 | `CONTRIBUTING.md` says the desktop is not in CI and the rest is hermetic | confirmed | Old text (now replaced) vs `.github/workflows/ci.yml` jobs `desktop`, `mlx-sidecar`, `check`; the Docker, .NET and browser tests depend on the host | W0.1 (done) |
| 17.1 | `compare_strict` exists; the permissive comparator is the default | confirmed | `crates/pwr-eval/src/lib.rs:2218` (`compare`), `2883` (`compare_strict`); `main.rs:9962-9994` uses strict only with `--strict`/`--declare` | W8.1 |
| 17.2 | R2 rerun: `4420340fd319` 11/30 vs 11/30; `5df73e30dde5` 18 vs 13, p = 0.227; 152,226 vs 106,034 generated tokens | confirmed | `experiments/r2-rerun-20260915-8648aed/analysis.md` (local, git-ignored) | W8.5 |
| 17.3 | Post-freeze revision: hidden check 14 with the repairs, 17 without | confirmed | `experiments/r2-harness-revision-20260916/analysis.md:8` (local) | — |
| 17.4 | R3 development run: 2/35 resolved | confirmed | `experiments/r3-h2-dev-20260917/analysis-35-trials.md:8` (local) | — |
| 17.5 | Stack-matrix runner nudges after an independent failure | confirmed | `NUDGE` sent on turns after the first, `run.py:431`; each turn is recorded, so first-cycle success is recoverable from existing results | W8.2 |
| 17.6 | Much evidence is git-ignored | confirmed | `.gitignore` (`/experiments/`, `/.pwr/*`, `/pwr-tests/`) | W8.5 |
| 24.1 | Engine installs direct pins, no transitive lock; Python `3.11` unpinned | confirmed | `apps/desktop/src-tauri/src/engine.rs:19-20`; `scripts/setup-mlx.sh:21` | W9.2 |
| 24.2 | `uv` is taken from `PATH` | **corrected** | A release build uses the `uv` bundled in the app's resources; `PATH` is only the development fallback (`engine.rs:153-171`). Building the release does require `uv` on the builder's `PATH` (`scripts/release-macos.sh`) | W9.2 |
| 24.3 | The release workflow does not run the CI gates | confirmed | `.github/workflows/release-macos.yml` and `scripts/release-macos.sh` build and verify the DMG and run no tests; the workflow also hard-codes `v0.2.0-alpha` (N8) | W9.1 |
| 18–23, 25–26 | Product positioning, hypotheses A–G, core, do-not-build, vision, risks, final verdict | not checkable | Judgements. Adopted as the project's direction in [MASTER_SPEC.md](../../MASTER_SPEC.md), [decisions.md](../decisions.md) and [risks.md](../risks.md) | — |

**Line-number drift.** The review cited `context.rs:526` (now 521), `context.rs:339`
(now 384), `compaction.rs:43` (now 48–54), `converse.rs:2470` (now 2498),
`converse.rs:1730` (the message is now at 2482). The code at those places is what the
review described.

## What the review missed

| # | Finding | Evidence | Plan |
|---|---|---|---|
| N1 | Prompt estimates ignore **tool-call arguments** appended since the engine's last count. `prompt_tokens_now` and `conservative_prompt_tokens` add `content` and `reasoning` only, so an assistant message carrying a large `write_file` is counted as nearly free until the next measured request. | `converse.rs:2448-2476`, `2537-2559`; contrast `compaction::message_tokens`, which does count calls (`compaction.rs:129`) | W4.2 |
| N2 | `own_overwrite` covers **every** existing file (8.2 above). The comment says "a file it had itself created"; the code does not check. | `converse.rs:2485-2510` | W1.1 |
| N3 | **Full access** mode (added 2026-09-29) runs commands with `SandboxPolicy::FullAccess`: no Seatbelt profile, so `.pwr/`, `.git/hooks`, protected paths and installed dependencies are all writable by any command, and the checks run unconfined. The goal's contract hash still catches a change to `.pwr/checks.json`; nothing else does. The review predates the mode's weight in the UI. | `sandbox_for`, `main.rs:1119-1124`; `PermissionMode::Full`, `main.rs:1106-1108` | W1.3, W1.9, W2.1 |
| N4 | `Store::append` turns a failure to read the previous hash into "no previous hash" (`.ok()`), silently starting a new chain; and a per-run link stored as `NULL` verifies as "unlinked", not broken, so removing a link downgrades the verdict instead of failing it. | `pwr-store/src/lib.rs:193-212`, `142-150` | W6.1 |
| N5 | After an edit, the chat turn appends the check verdict as a `ChatMessage::text("tool", …)` with no tool-call id. Whether every chat template accepts a tool message with no matching call is not tested. | `main.rs:4924-4927` | W2.2 |
| N6 | `container_socket()` accepts a stale socket file (`path.exists()`), which is the root cause of the only failing test. | `pwr-tools/src/lib.rs:842-865` | W0.2 |
| N7 | Current documents contradicted the code in places the review did not list: README still named 0.1.2 as the latest alpha although `v0.2.0-alpha` is tagged; `SECURITY.md` and `apps/desktop/README.md` described two permission modes where there are three; `model-compatibility.md` said reasoning is never written back into the conversation, while each step keeps up to 16,000 characters of it (`kept_reasoning`, `converse.rs:2519`). | `git tag`; `main.rs:1096-1108`; archived docs | W0.1 (done) |
| N8 | The release workflow hard-codes the tag `v0.2.0-alpha` and its notes file. | `.github/workflows/release-macos.yml` | W9.1 |
| N9 | Files over 1 MB and binary files are left out of the repository index entirely, while the constant's comment says they are "inventory entries only". | `MAX_INDEXED_BYTES` and its comment, `crates/pwr-repo/src/lib.rs:397-398`; `continue` at `560-562` | W1.7 |
| N10 | `complete`'s description tells the model *"Accepted only if deterministic verification then passes"* — true of Goal mode with an acceptance check, not of a conversation turn or a scripted run without a verifier. | `action_tool_catalog`, `crates/pwr-orchestrator/src/lib.rs:4560+` | W3.4 |
| N11 | A `.pwr/checks.json` that does not parse is silently ignored by check discovery (`declared_checks` returns `None` and discovery falls through to CI and markers), while the acceptance reader reports the same file as an error. | `crates/pwr-verify/src/lib.rs:302-322`, `333-356` | W3.2 |

## What this verification did not do

No model was run, no DMG was built, the desktop was not walked by hand, and
no penetration test was attempted. The "inference" claims (a race between hash
check and write, concurrent appenders breaking the chain, alternating failures
running past the goal budget) were confirmed as reachable in the code, not
reproduced. Each plan item that fixes one starts with a test that reproduces it.
