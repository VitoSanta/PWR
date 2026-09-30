# Implementation plan

**Adopted 2026-09-30.** Built from the [technical review of
2026-09-30](../reviews/2026-09-30-technical-review.md) after every one of its
claims was checked against the code ([verification](../reviews/2026-09-30-verification.md)).
It replaces the [v0.3.0-alpha plan](../archive/v0.3.0-alpha-plan.md) and the
[backlog](../archive/backlog.md) as the order of work: every item still open in
either is placed below (see [Carried over](#carried-over-from-earlier-plans)),
none is dropped silently.

The plan answers one question before any other, the one the review put:

> **At equal model, machine and budget, does PWR resolve more real tasks than
> a simple loop, or the same tasks with less time and fewer human
> interventions?**

Everything before the benchmark (W8) exists to make that answer trustworthy
and the product safe to use while it is being found. Everything after it
depends on the answer.

## How to read this plan

Each work item has an ID (`W<stream>.<n>`), a status, and these parts:

- **Problem** — what is wrong, in one or two sentences.
- **Evidence** — where the code shows it (current line numbers).
- **Change** — what to build. Where a design choice is still open it says so.
- **Done when** — acceptance criteria. An item is not done until all hold.
- **Tests** — the tests that must fail before the change and pass after.
- **Docs** — which document changes in the same commit.

Statuses: **DONE**, **NOW**, **NEXT**, **LATER**, **NOT NOW** (a deliberate
decision, see [decisions.md](../decisions.md)). Sizes are rough: S (a day or
less), M (a few days), L (a week or more).

Rules that hold for every item:

1. A change carries a test that fails without it (see [CONTRIBUTING](../../CONTRIBUTING.md)).
2. A change that alters behaviour a document describes updates that document
   in the same commit; the document says what the code does, not what it will do.
3. A change that alters what an evaluation measures (prompt, catalogue, loop,
   budgets) is recorded in the [experiment log](../experiment-log.md) with the
   commit, before a campaign relies on it.
4. No new crate, framework or runtime dependency without a stated reason in
   the item.

## Order and gates

```text
NOW    W0 truthful baseline ─┐
       W1 safe effects ──────┼─► G1 effects are safe and honestly reported
       W2.1-2.2, W3.1, W4.1 ─┘
NEXT   W2 one executor ──────┐
       W3 verification ──────┤
       W4 context contract ──┼─► G2 the app, the CLI and the evaluator run one path
       W5 inference ─────────┤
       W6 journal ───────────┘
       W8 decisive benchmark ──► G3 the decision: keep, cut or change the adaptive layer
       W7 desktop, W9 distribution run alongside, bounded by G1/G2
LATER  only what G3 justifies (see Later)
```

| Gate | Passes when | If it does not pass |
|---|---|---|
| **G1 — effects** | W0.2, W1.1–W1.6, W1.9, W2.2, W3.1 and W4.1 are done; every P1 of the review has a regression test; the Rust, desktop and sidecar suites are green on CI with skips reported, not hidden | No new capability work starts. A release may ship fixes only. |
| **G2 — one path** | W2.1, W2.3, W2.4 are done; `eval run` measures the executor the app uses; the parity test lists no behaviour present in one entry point and absent in another except the declared B0/B2 controls | The benchmark is not run: it would measure a path the product does not ship. |
| **G3 — the decision** | W8.4's confirmatory campaign has run under its preregistration | See [the decision rule](#w84-the-confirmatory-campaign). |

A gate is recorded as passed in [roadmap.md](../roadmap.md) and
[milestones.json](../milestones.json) with the commit and the evidence.

---

## Working-tree implementation ledger — 2026-09-30

This ledger describes local code, not completed gates. No item below is marked
DONE without its integration commit and complete acceptance evidence. G1, G2
and G3 remain unpassed; no confirmatory model campaign or release was run.

| Items | Implemented locally | Remaining evidence or implementation |
|---|---|---|
| W1.3 | Frozen files/folders and installed dependencies denied to commands; ancestor renames, aliases and parent deletion guarded; checks share protection | CI/macOS coverage; unsupported paths are surfaced as partially enforced, Full access as unconfined |
| W1.4 | Goal-wide actions/refusals/checks/reviews/deadline with batch guards | Integration commit and CI |
| W1.5, W3.3 | Failure identities for Rust, pytest, Go, .NET, Jest/Vitest; fingerprints drive repetition and reproduction | Integration commit; broader real-output fixtures remain useful |
| W1.6 | 8 MiB decoded stream and 64 MiB cumulative document limits | Integration commit and CI |
| W1.7 | Pre-read size bound, bounded read, HashSet retention in one transaction; 20,000-row fixture | Integration commit and CI |
| W1.8, W4.6 | Embedding startup/replies have deadlines; kill on timeout; lexical fallback is recorded and visible | W4.6 remains partial for other context refresh/retrieval paths |
| W1.9, W7.4 | Permission text states actual scope; turn outcome exposes confinement, including observed unconfined commands | Integration commit; native UI walk |
| W2.1, W7.1 | Typed domain outcome produced by conversation core, carried in ACP, consumed by app | PARTIAL: scripted runner/CLI JSON and shared executor not yet migrated |
| W2.2 | Marks follow typed verdicts; verification feedback is a typed user note, not an orphan tool result | Integration commit; remaining backend template fixtures |
| W2.5 | Window arithmetic moved to runtime; task profile construction stays in orchestrator | Integration commit and CI |
| W3.1 | Artifact paths/globs, inferred test/config/script evidence, hashes in persisted checkpoint; changed evidence stops goal; per-file human authorization | Integration commit; native permission-flow walk; declare artifacts explicitly when conventions cannot establish dependencies |
| W3.2 | Both check readers reject malformed declarations | PARTIAL: CI proposal provenance, skipped-step reasons and adoption UI remain |
| W3.5 | Zero-test signatures for cargo/pytest/Jest/Vitest/Go/.NET share one evidence helper | PARTIAL: extend fixtures to every supported runner |
| W4.1 | Full objectives/revisions survive repeated compaction; ContextFull if they cannot fit | Integration commit and CI |
| W4.3 | Compiler reports required-section overflow; callers reject oversized required input | PARTIAL: exact backend preflight is W4.2, still pending |
| W5.1 | MLX prefill observes cancel between chunks and clears failed resume cache | Integration commit; live model cancellation smoke test |
| W5.2 | Workspace summaries default off; cancellation handle passes to backend; Settings toggle; incoming-prompt preemption regression passes | Live-model smoke test, native Settings walk and integration commit |
| W6.1 | WAL/busy timeout, immediate atomic append, propagated journal errors and indexes | Integration commit and CI |
| W7.2 | Searchable repository outline is default; 3D is explicitly experimental and lazy | Native UI walk; integration commit |
| W8.1 | Strict comparison default, explicit --legacy-pairing marked noncausal | Integration commit and CI; no campaign inference from this change |
| W9.1 | Tagged release reuses all CI jobs, requires them before building, derives version from tag | Actual workflow run; no release created |

Local consolidation evidence (2026-09-30): CLI 216 unit tests pass, desktop
106 tests and production build pass, MLX sidecar 38 tests pass using the installed
engine Python, workspace Clippy passes with warnings denied, formatting and diff
checks pass. A regression executes a verifier that rewrites its own acceptance
artifact under Full access and confirms that it cannot certify the goal.
The full workspace suite completed: 1,327 passed, zero failures, five explicitly
ignored live/environment-dependent tests. A subsequently added lexical-fallback
regression passed separately, and its target passes Clippy. This is test evidence,
not evidence of model uplift. No live-model campaign was run. Remote CI cannot currently be dispatched from this environment:
`gh auth status` reports no authenticated GitHub host. This does not satisfy G1.
The gate-dependent executor migration and product-path benchmark remain pending.

Additional W1.3 regression found during implementation: protecting a leaf alone
was insufficient when an agent deleted/renamed its parent or addressed it through
an alias. File guards now compare the physical target and ancestors; Seatbelt
blocks unlink/rename of protected ancestors while allowing sibling writes.

---

## W0 — A truthful baseline

### W0.1 Rewrite the documentation to match the code

**Status:** DONE (2026-09-30)

- **Problem.** Current documents contradicted the code and each other
  (verification N7, 9.4, 16.2): the README named a superseded release, two
  documents described two permission modes where there are three, several
  said no-verifier completion is refused, CONTRIBUTING described CI wrongly.
- **Change.** The documentation set was rewritten from the code at `0776ff4f`;
  every earlier document was moved, whole, to [archive/](../archive/README.md).
- **Done when.** Every current document names the revision it was checked
  against; `docs/README.md` indexes them; no current document links into the
  archive as if it were current.

### W0.2 Say what a test run exercised

**Status:** DONE (2026-09-30)

- **Problem.** Host-dependent tests `return` early and count as passes; the
  one failing test fails for an environment reason (verification 16.1, N6).
- **Evidence.** `crates/pwr-tools/tests/sandbox_and_approvals.rs:1265, 1321,
  1357, 1518, 1570`; `local_service.rs:114, 131`; `provisioning.rs:157, 166`;
  `pwr-models/tests/live_model.rs:31`; `container_socket()`,
  `crates/pwr-tools/src/lib.rs:842` (accepts a stale socket file).
- **Change.**
  1. A small test helper, `skip!(reason)`, that prints one line in a fixed
     format (`PWR-SKIP <test> <reason>`) and returns. Every early return uses it.
  2. The Docker test probes the daemon (a connect on the socket, outside the
     sandbox) before asserting, and skips with *"socket present, daemon not
     answering"* when it does not answer. `container_socket()` itself is left
     as it is — it answers "where would the engine be", which is still right
     for the sandbox grant — and gains a doc comment saying it does not prove
     the engine runs.
  3. CI's `check` job greps the skip lines into an annotation, like the failing
     tests, so a run states what it did not exercise.
  4. Tests that need an environment variable (`PWR_LIVE_MODEL`) stay
     `#[ignore]`d or env-gated, and are listed in [testing.md](../testing.md)
     with how to run them.
- **Done when.** With Docker Desktop off, `cargo test --workspace` exits 0 and
  prints a `PWR-SKIP` line for the Docker test; CI shows the skip list.
- **Tests.** The helper's own unit test; the Docker test run with the daemon
  off (by hand on a Mac, recorded in testing.md).
- **Docs.** [testing.md](../testing.md).

---

## W1 — Safe, predictable effects

The review's first "build next". Each item closes a way the agent can damage
the person's work or report a boundary it does not have.

### W1.1 Bind an overwrite to the version the model read

**Status:** DONE (2026-09-30) except the measurement below

- **Problem.** `write_file` onto an existing file becomes `ApplyReplace` with
  the hash the file has *at execution time*, so a rewrite made from stale
  content passes the hash check and overwrites a newer edit. It applies to
  every existing file, including ones the conversation never read
  (verification 8.2, N2).
- **Evidence.** `own_overwrite`, `crates/pwr-orchestrator/src/converse.rs:2498`;
  called at `converse.rs:1897`; the `written` map (`converse.rs:494`) is not
  consulted.
- **Change.**
  1. Extend the conversation's `written` map into a *known versions* map:
     path → the hash of the version this conversation last saw, updated by
     every read result (`read_file`, window reads included: the tool already
     returns the file's hash), every edit result and every create.
  2. `own_overwrite` uses the known hash, never the current one. No known
     hash → refused: *"`<path>` exists and has not been read in this
     conversation; read it first"*. Known hash differs from the file → the
     normal conflict refusal, worded for the case: *"`<path>` changed since you
     read it (outside this conversation, or by a command); read it again"*.
  3. The same rule for `restore_file` and the app's Revert, which already
     refuse a file that no longer holds what the model wrote (keep that test).
- **Done when.** A file edited by a person after the model read it cannot be
  overwritten by `write_file` or `apply_replace` without a new read; a file
  the conversation created or read can still be rewritten whole in one call.
- **Tests.** In `converse.rs` / `two_loops.rs`: (a) read, external edit,
  `write_file` → refused, file unchanged; (b) create then rewrite → allowed;
  (c) existing never-read file → refused with the read instruction; (d) a
  command changes the file after the read → refused.
- **Measure.** The rule was relaxed on 2026-09-25 because Ornith-1.5-35B
  regenerated whole files six times after refusals. Re-run
  `corpus/small-apps-v1.json` on the four small models of 2026-09-29/30 with
  the change and record refusals per task in the experiment log. If refusal
  churn returns, the answer is a better refusal (show the current content's
  window), not a hash the core invents.
- **Implemented** as `Continuity::known` (across turns, seeded after a restart
  from the checkpoint's changed files) beside `written`, which stays what
  rewind compares against because a read must not update it. Tests: four unit
  tests on `own_overwrite` and five fixtures in `two_loops.rs` (never-read file
  refused; refused rewrite leaves the file; an edit made between read and
  rewrite survives; a read in one turn is known in the next and a change
  between turns is caught; a file the model created is rewritten without a
  read). With the old behaviour restored, the four refusal tests fail.
  `apply_replace` called directly is unchanged: it refuses a stale hash and
  names the current one. **Still to do:** the measurement above, which needs
  model runs and waits for the maintainer's go-ahead.
- **Docs.** [tools-and-sandbox.md](../tools-and-sandbox.md), [agent-loop.md](../agent-loop.md).

### W1.2 Atomic, checked writes

**Status:** DONE (2026-09-30)

- **Problem.** Edits are read → check hash → `std::fs::write`; a crash or error
  leaves a partial file, and a concurrent change between check and write is
  lost (verification 8.3).
- **Evidence.** `crates/pwr-tools/src/lib.rs:3493` (apply_patch), `3927`,
  `4067`, `5036`.
- **Change.** One helper in `pwr-tools`, used by every file-writing tool
  (write, apply_patch, replace_text, apply_replace, restore_file, the core's
  revert):
  1. resolve the real target (a write through a symlink writes the target, as
     today, and the target must still pass the path policy);
  2. write the new bytes to a temporary file in the same directory
     (`.<name>.pwr-<random>`), `fsync` it, copy the original's permissions;
  3. re-read the target's hash immediately before the rename and refuse if it
     is not the expected one;
  4. `rename` over the target, then `fsync` the directory;
  5. on any error, remove the temporary file.
  Temporary names are excluded from the index, the diff and the tools' listings.
  The residual window between step 3 and step 4 is stated in SECURITY.md; no
  general rollback is promised.
- **Done when.** No file-writing tool calls `std::fs::write` on a workspace
  file; an injected failure between write and rename leaves the original
  intact; permissions survive an edit.
- **Tests.** Unit tests with a failure hook; permissions (`0o755` script stays
  executable); symlink-to-inside and symlink-to-outside behave as before; a
  change between hash check and rename is refused.
- **Implemented** in `crates/pwr-tools/src/atomic.rs` (`write_atomic`, with
  `Expect::{Anything, Absent, Hash}` and a stage hook for tests). Differences
  from the sketch above: the temporary name is `.pwr-tmp-<pid>-<n>-<name>`
  (`pwr_repo::TEMPORARY_WRITE_PREFIX`); a new file is created with `hard_link`,
  which fails if the name is taken, instead of a check-then-rename; a read-only
  target is still refused; `resolve` already returns the real path of a
  symlink, so no separate resolution step was needed. Wired into `write_file`,
  `apply_patch`, `replace_text`, `apply_replace`, `restore_file`,
  `extract_document`, the core's Revert and rewind. Tests: eight in `atomic.rs`
  (failure at each stage leaves the original, a change after the check is
  refused and kept, a deleted file is not recreated, permissions, symlink,
  read-only, create and a lost creation race) and three tool-level tests
  (no stray file after each tool, an edited script stays executable, a write in
  flight is not listed or searched).
- **Docs.** [tools-and-sandbox.md](../tools-and-sandbox.md), [SECURITY.md](../../SECURITY.md).

### W1.3 The same protections for commands as for file tools

**Status:** NOW · M

- **Problem.** Protected paths (`.pwr/protected.json`) and installed dependency
  trees are refused to the file tools but writable by any command the agent
  runs, and by the checks (verification 8.1). In Full access mode nothing at
  all is protected (N3).
- **Evidence.** `refuse_if_protected`, `crates/pwr-tools/src/lib.rs:1984`;
  `sandbox_profile`, `lib.rs:2092-2375` (denies `.pwr` and `.git/hooks` only);
  `sandbox_for`, `crates/pwr-cli/src/main.rs:1119`.
- **Change.**
  1. The Seatbelt profile denies `file-write*` on every protected path
     (`literal` for files, `subpath` for folders) and, unless
     `DependencyChange` is granted, on installed dependency trees
     (`node_modules`, `site-packages`, `vendor` at any depth, as a `regex`).
  2. Reconcile with commands that install dependencies (`npm install`, `pip
     install`, …): today some command forms are recognised as dependency
     changes and asked about. Under the new profile, an install without the
     grant fails in the sandbox; its result must say which grant it needs,
     the way a network denial already does.
  3. A protection the profile cannot express (a path with a quote, a platform
     without an adapter) is reported as *not enforced for commands* in the
     turn's outcome (W2.1), never presented as held.
  4. Full access: the outcome says `confinement: unconfined` for every command
     and check; the UI shows it on the turn (W7.4). See W3.1 for what a goal
     can claim there.
- **Done when.** A sandboxed `sh -c 'echo x > spec.md'` with `spec.md`
  protected is denied; a write into `node_modules/` is denied without the
  grant and allowed with it; the checks run under the same profile.
- **Tests.** `crates/pwr-tools/tests/sandbox_and_approvals.rs`: protected file,
  protected folder, dependency tree with and without the grant, and an install
  command's refusal text.
- **Docs.** [tools-and-sandbox.md](../tools-and-sandbox.md), [SECURITY.md](../../SECURITY.md).

### W1.4 A goal budget that bounds every path

**Status:** NOW · implemented in working tree; integration commit pending

- **Problem.** `GOAL_MAX_ACTIONS` is checked only when the model did *not*
  complete, so refused completions with alternating failures loop without
  limit; there is no time limit (verification 4.2).
- **Evidence.** `crates/pwr-cli/src/serve.rs:450`, `2028`, `2182`.
- **Change.** A `GoalBudget` checked at the top of every iteration of the goal
  loop, before generation, verification or review: actions, refused
  completions, verification runs, review rounds, and wall-clock time. Defaults
  start from today's (208 actions) plus a wall-clock limit of 60 minutes and a
  cap of 6 refused completions; they are settable per workspace and recorded
  in the outcome. When a limit is reached the goal stops with terminal
  `budget` and says which limit.
- **Done when.** No sequence of turn results can keep a goal running past any
  limit.
- **Tests.** Fake `TurnRunner` (as in `serve.rs` tests): completes every turn
  and fails verification with alternating check sets → stops at the refused
  completion cap; Tokio’s paused clock → stops during baseline, generation,
  completion verification and review at the time limit; the existing
  `blocked` and `stalled` tests still pass.
- **Docs.** [agent-loop.md](../agent-loop.md).

- **Implemented.** Defaults: 208 actions, 6 refusals, 9 verification runs,
  1 review round, 3,600 seconds; workspace `goal_budget` overrides. Counters and
  limits are recorded on all goal outcomes. Tests also cover batched calls and
  a zero remaining action allowance and ordinary-chat isolation.
- **Validation (2026-09-30).** Workspace Rust suite passed (host-dependent
  tests retain their skip semantics; live-model tests remain ignored); CLI
  suite passed; 9 W1.4 regression tests passed; workspace Clippy and final CLI
  Clippy passed with warnings denied; desktop 95 tests and production build
  passed; formatting, diff checks and roadmap generation passed. No model
  campaign was run. W1.3 is still open; G1 is not passed.

### W1.5 Tell progress from repetition in goal verification

**Status:** NOW · S

- **Problem.** Refused completions are compared by the *names* of the failing
  checks: a goal fixing tests one by one inside one suite looks stuck, and two
  alternating suites look like progress (verification 10.3).
- **Evidence.** `verify_goal`, `crates/pwr-cli/src/main.rs:3756-3773`;
  `same_failure`, `serve.rs:2139`.
- **Change.** A failure fingerprint per check: the failing test identifiers
  where the output format is known (cargo, jest/vitest, pytest, go test,
  dotnet), otherwise a digest of the normalised tail of the output (paths,
  timings and addresses stripped). *Blocked* = the same fingerprint three
  times; *progress* = the set of failing identifiers shrank. The budget of
  W1.4 bounds everything else.
- **Tests.** Fixtures of each output format; a suite whose failures shrink
  each round is not stopped; the same failure three times is.
- **Docs.** [verification.md](../verification.md).

### W1.6 Bound PDF decompression

**Status:** NOW · S

- **Problem.** PDF streams are inflated with an unbounded `read_to_end`
  (verification 8.6).
- **Evidence.** `inflate`, `crates/pwr-tools/src/document.rs:232-250`.
- **Change.** `take()` each decoder at a per-stream limit and keep a
  per-document total; past either, the document is reported as too large to
  extract, with the limit.
- **Tests.** A small PDF with a stream that inflates past the limit.

### W1.7 Bound and speed up the repository walk

**Status:** NOW · S

- **Problem.** The indexer reads a whole file before its size bound; `retain`
  is O(n·m) (verification 7.5, 12.1).
- **Evidence.** `crates/pwr-repo/src/lib.rs:558-560`, `477-496`.
- **Change.** Check `metadata.len()` against `MAX_INDEXED_BYTES` before
  reading; `retain` takes a `HashSet`; delete in one transaction.
- **Tests.** A file over the bound is never read (a counting reader or a size
  check in the work report); a 20,000-file fixture index stays linear.

### W1.8 A timeout on the embedding sidecar

**Status:** NOW · S

- **Problem.** `Embedder::read_line` blocks forever on a sidecar that does not
  answer; "fall back to lexical" does not cover a hang (verification 12.4).
- **Evidence.** `crates/pwr-mlx/src/embed.rs:138-155`.
- **Change.** Read with a deadline (a reader thread and a channel, or an async
  child); on timeout kill the process and fall back to lexical ranking, and
  record the fallback where the turn's context composition is reported (W4.6).
- **Tests.** A fake sidecar that sleeps: the ranker returns within the
  deadline and the fallback is visible.

### W1.9 State the boundaries the product cannot enforce

**Status:** NOW · S

- **Problem.** Some limits cannot be closed on macOS today — the container
  engine acts outside the sandbox, `LocalService` covers every local address,
  a network grant is not per destination, argv does not stop an interpreter
  running arbitrary code, Full access has no sandbox. They must be said where
  the person decides, not only in SECURITY.md.
- **Change.** The permission question for `container_engine`,
  `network_access`, `local_service` and `outside_sandbox`, and the Full access
  mode selector, carry one sentence each on what the grant actually opens
  (text in [tools-and-sandbox.md](../tools-and-sandbox.md)). No new mechanism.
- **Tests.** Desktop unit tests on the permission component's text.

---

## W2 — One execution semantics

The review's second "build next": the app, the CLI and the evaluator must mean
the same thing by "complete".

### W2.1 One structured outcome

**Status:** NOW · M

- **Problem.** "Complete", "verified" and "done" mean different things in the
  chat turn, the goal loop, the scripted run and the evaluator; unverifiable
  completion is allowed in one and described as refused in documents
  (verification 9.4).
- **Change.** A typed `TurnOutcome` in `pwr-domain`, produced by every entry
  point and carried to the app, the CLI's JSON and the evaluator:

  | Field | Values |
  |---|---|
  | `terminal` | `completed`, `blocked`, `interrupted`, `budget_exhausted`, `declined`, `failed{class}` |
  | `delivered` | whether the agent handed over an answer or changes |
  | `checks` | `not_run`, `unavailable{why}`, `passed`, `ran_zero_tests`, `failed{fingerprints}`, `could_not_run{why}` |
  | `baseline` | `preserved`, `regressed{checks}`, `no_baseline` |
  | `acceptance` | `accepted`, `not_declared`, `contract_changed{what}`, `failed` |
  | `confinement` | `sandboxed`, `partially_enforced{what}`, `unconfined` |
  | `budget` | what was spent against which limit (W1.4, W2.6) |

  "Verified" is reserved for `acceptance: accepted` with `confinement` not
  `unconfined` or with the verifier artifacts intact (W3.1).
- **Done when.** `TurnReport`, `GoalVerification` and `TaskRunResult` are
  derived from, or replaced by, `TurnOutcome`; the ACP `_meta.pwr` carries it;
  the CLI's `--json` output carries it.
- **Tests.** Serde round-trip; one test per entry point asserting the outcome
  for: no checks, checks pass, checks fail, zero tests, baseline red,
  acceptance contract changed, Full access.
- **Docs.** [verification.md](../verification.md), [pwr-serve.md](../pwr-serve.md), [glossary.md](../glossary.md).

### W2.2 Report what the checks said, not that they ran

**Status:** NOW · S

- **Problem.** The post-turn note is `✓ …` whatever the verdict; the verdict is
  appended as a `tool` message with no call id (verification 13.1, N5).
- **Evidence.** `crates/pwr-cli/src/main.rs:4922-4927`.
- **Change.** The note's mark comes from the outcome (`✓` passed, `✗` failed or
  regressed, `–` unavailable or not run, `!` could not run). The verdict
  reaches the model as a harness note with a typed purpose, rendered in the
  role every template accepts (system or user, per adapter), not as an orphan
  `tool` message.
- **Tests.** One per verdict for the mark; an adapter test that the rendered
  prompt of each family contains no tool message without a call.

### W2.3 One session executor

**Status:** NEXT · L

- **Problem.** The conversation turn, its post-turn verification, and Goal
  mode live in `pwr-cli` (`main.rs`, `serve.rs`); a new interface or benchmark
  re-assembles the product by hand (verification 2.1, 2.2, 3.1).
- **Change.** A session executor in `pwr-orchestrator` (a module, not a new
  crate): `execute(request, policy, verifier, budget) -> TurnOutcome`, where
  `policy` is *conversation* or *goal*. It owns: the turn (`take_turn`),
  post-turn verification, the goal loop, its review round, budgets and the
  outcome. `pwr-cli`'s chat console and `pwr serve` call it and only decide
  presentation and interaction. Extract decisions with a contract — budget,
  acceptance, action execution, persisted state — not code by line count.
- **Done when.** `serve.rs` has no goal loop and `main.rs` no post-turn
  verification; the protocol tests in `serve.rs` pass unchanged.
- **Tests.** Existing `two_loops.rs` and `serve.rs` protocol tests; new
  executor tests with a fake provider for each policy.

### W2.4 Converge the scripted runner onto the executor

**Status:** NEXT · L

- **Problem.** `pwr run` and `eval run --arm b1` measure a loop the app does
  not ship; fixes land in one loop only (verification 3.2;
  `docs/release/v0.2.x-mac-verification.md`, "Still open").
- **Change.**
  1. Inventory, in a parity test, every behaviour of the scripted loop and of
     the conversation turn: planning (`--plan`), `record_progress`,
     `propose_verifier`, context-tier retry, ledger compaction and the
     evidence-state policy, recovery classification, provisioning, the
     completion holds (unseen results, nothing done, nothing run), repeated
     failure detection, reasoning budgets.
  2. Each behaviour is either moved into the executor (with evidence that it
     helps), kept only as a declared option of the B0/B2 controls, or removed.
     The decision per behaviour is recorded in [decisions.md](../decisions.md).
  3. `pwr run` and `eval run --arm b1` call the executor with the goal policy.
     B0 and B2 remain in `baseline.rs` as experimental controls sharing the
     executor's tools, policy and outcome.
- **Done when.** The parity test lists no undeclared difference; `eval run
  --mode product-path --arm b1` and the app run the same code.
- **Docs.** [agent-loop.md](../agent-loop.md), [evaluation.md](../evaluation.md), [architecture.md](../architecture.md).

### W2.5 Move the window arithmetic out of the orchestrator

**Status:** NEXT · S

- **Problem.** `pwr-models` (catalogue, download, fit) depends on
  `pwr-orchestrator` for `window::decide` (verification 3.3).
- **Change.** Move `window.rs`'s arithmetic (`decide`, `HostBudget`,
  `ModelShape`, the reserve) into `pwr-runtime`, which already owns host
  observation; `pwr-models` drops the orchestrator dependency. Nothing else moves.
- **Done when.** `crates/pwr-models/Cargo.toml` has no `pwr-orchestrator`; the
  window tests move with the code and pass.

### W2.6 One recovery budget

**Status:** NEXT · M

- **Problem.** A dozen detectors each have their own limit; their combination
  is hard to predict (verification 4.4).
- **Change.** Keep every classification. Add a per-turn `RecoveryBudget`: each
  detector spends from it with its cause recorded; the turn stops when the
  shared budget is spent, with per-cause counts in the outcome. Individual
  limits stay only where a measurement justified them (each keeps its comment).
- **Tests.** A fake provider mixing causes (one silent reply, one malformed
  call, one backend fault, …) stops at the shared budget; per-cause counts are
  right.

### W2.7 Type the boundaries that decide safety and completion

**Status:** NEXT · M

- **Problem.** `ChatMessage.role` is a string and sampling a
  `BTreeMap<String, Value>`, so invalid combinations are representable and
  unknown options pass silently (verification 11.5, 15.1).
- **Change.** `Role` enum (`system`, `user`, `assistant`, `tool`) with serde
  names unchanged; `Sampling` struct with the known fields and an explicit
  `extra` map that each backend must either consume or reject by name.
  Nothing else is retyped.
- **Tests.** An unknown sampling key reaching a backend that ignores it is an
  error, not a no-op.

---

## W3 — A verification contract that means what it says

### W3.1 Freeze what decides acceptance

**Status:** NOW · M

- **Problem.** Goal acceptance hashes `.pwr/checks.json` only; the tests and
  scripts its commands run can be weakened and the goal still certified
  (verification 9.2).
- **Evidence.** `acceptance_contract_hash`, `crates/pwr-cli/src/serve.rs:723`;
  `verify_goal`, `crates/pwr-cli/src/main.rs:3706-3790`.
- **Change.**
  1. `.pwr/checks.json` gains `acceptance.artifacts`: paths or globs of the
     files that decide acceptance (tests, fixtures, scripts, configuration).
     When the owner declares none, the default is the test files each
     acceptance command is known to run (by toolchain convention), stated as
     inferred.
  2. At session start PWR records their hashes with the contract hash. At goal
     verification, a changed artifact makes the acceptance
     `contract_changed{files}` unless the person authorised that change during
     the session (a permission question naming the file).
  3. Acceptance artifacts are protected paths for the file tools and, through
     W1.3, for commands.
  4. In Full access the same comparison holds; it is the only thing that can
     let a goal be `accepted` there.
- **Done when.** A goal that edits an acceptance test and then passes it is
  reported `contract_changed`, not verified.
- **Tests.** `serve.rs` goal tests: edit a declared artifact → not verified;
  authorised edit → verified; inferred artifacts for cargo and npm projects.
- **Docs.** [verification.md](../verification.md).

### W3.2 CI configuration proposes checks; it does not define acceptance

**Status:** NEXT · S

- **Problem.** CI discovery reconstructs commands textually, without
  `working-directory`, `env` or multi-line scripts (verification 9.1).
- **Change.** Checks found in CI configuration are marked `source: ci_inferred`
  in the outcome and the UI, are never acceptance checks, and the first turn
  that relies on them offers to write them into `.pwr/checks.json` for the
  person to confirm. Steps with `working-directory` or `env` are skipped with
  a reason rather than run in the wrong place. A `.pwr/checks.json` that does
  not parse is reported as an error, never skipped silently in favour of CI or
  marker discovery (verification N11). For a workspace with no checks at all,
  the offer proposes the project type's usual ones (`dotnet build`, `ng build`,
  `cargo test`, a served page) for the person to confirm.
- **Tests.** Fixtures with `working-directory`, `env`, `run: |`; a malformed
  `checks.json` is an error in both readers.

### W3.3 Tell flaky from failing by the failure, not the exit code

**Status:** NEXT · S

- **Problem.** Reproduction compares exit codes only (verification 10.1).
- **Evidence.** `classify_with_reproduction`, `crates/pwr-verify/src/lib.rs:1002`.
- **Change.** Compare the W1.5 fingerprint of the two runs: same exit code
  with a different failing set is non-determinism; the same fingerprint is a
  reproduced failure.
- **Tests.** Two runs, exit 1 both, different failing tests → `NonDeterminism`.

### W3.4 Unverifiable completion, said once and the same way

**Status:** NOW · S

- **Problem.** The scripted loop ends `verified: false, verifiable: false`
  when no verifier exists, which is right; documents said it was refused
  (verification 9.4).
- **Change.** Keep the behaviour; express it through `TurnOutcome`
  (`checks: unavailable`, `acceptance: not_declared`). The documents now say
  what the code does (done in W0.1). The `complete` tool's description, which
  tells the model *"Accepted only if deterministic verification then passes"*,
  is rewritten per policy to say what will actually happen (verification N10).
- **Tests.** The catalogue a conversation, a goal and a scripted run offer
  each describes `complete` as that path treats it.

### W3.5 Checks that ran no tests, for every toolchain

**Status:** NEXT · S

- **Problem.** "Ran zero tests" is detected only for `cargo test`
  (`cargo_checks_ran_zero_tests`, `crates/pwr-cli/src/main.rs:4163`, and `verify_goal`, `main.rs:3729`); `jest`, `pytest` (exit 5), `go test` ("no test
  files") and `dotnet test` pass the same way.
- **Change.** Per-toolchain detectors feeding `checks: ran_zero_tests`.
- **Tests.** One fixture per toolchain.

---

## W4 — Context as a contract

The review's third "build next".

### W4.1 Keep the objective outside what compaction may shorten

**Status:** NOW · M

- **Problem.** Compaction keeps the first request to 800 characters and later
  requests to 200-character lines; a long specification loses the
  constraints that decide success (verification 4.3).
- **Evidence.** `crates/pwr-orchestrator/src/compaction.rs:48-54`;
  `conversation.rs:52` keeps the revision number only.
- **Change.** The conversation checkpoint stores the objective verbatim (the
  first request of a task) and each steering revision verbatim. Compaction
  renders them from the checkpoint, whole, in a section it never shortens. If
  the objective and its revisions no longer fit the room compaction can make,
  the turn stops with `ContextFull` and says so ("the objective takes N tokens
  of M"), rather than abbreviating it.
- **Done when.** A 10,000-character specification with its decisive
  constraint in the last paragraph reaches the model verbatim after three
  compactions.
- **Tests.** `crates/pwr-orchestrator/tests/context_compaction.rs`: the above;
  a steering revision survives; an objective larger than the room stops the turn.

### W4.2 Count the real prompt before generating

**Status:** NEXT · M

- **Problem.** Budgets use `len()/4` and `len()/3` estimates; tool-call
  arguments appended since the last measurement are not counted at all
  (verification 7.1, 7.3, N1).
- **Change.**
  1. Immediately (NOW, S): count tool-call arguments in `prompt_tokens_now`
     and `conservative_prompt_tokens`, as `compaction::message_tokens` does.
  2. A `count_prompt` request to the MLX sidecar: render the template with the
     messages and tools, tokenize, return the count. The turn asks it before a
     generation whose estimate is within 20 % of the room (or always, if
     measurement shows the cost is negligible). llama.cpp: its
     `/apply-template` and `/tokenize`, where the server version has them;
     otherwise the estimate, labelled.
  3. The context panel shows the counted figure when there is one.
- **Tests.** A history whose last assistant message carries a 40 KB
  `write_file` triggers compaction before the request; sidecar unit test of
  `count_prompt` against the tokenizer.

### W4.3 Refuse a prompt whose required part does not fit

**Status:** NEXT · S

- **Problem.** `context::compile` keeps required sections over budget with no
  signal (verification 7.2).
- **Change.** `CompiledPrompt` gains `over_budget_by`; the callers stop with
  `ContextFull` and the reason instead of sending it.
- **Tests.** Required sections larger than the budget → the flag is set and
  the turn stops.

### W4.4 Keep large tool output retrievable

**Status:** NEXT · M

- **Problem.** Tool output is bounded and hashed, but the bytes past the bound
  are not kept, so a model cannot read the part of a long log it needs; a hash
  without the bytes is not retrievable evidence.
- **Change.** Output past the bound is stored under `.pwr/outputs/<hash>` (a
  projection with a size cap per workspace, oldest removed first). The model
  gets head, tail and a handle; `read_file` on the handle reads a window of it.
- **Tests.** A command printing 2 MB: head and tail in the result, a window
  read of the middle works, the cap removes the oldest.

### W4.5 Framework guidance from the actual catalogue

**Status:** NOW · S

- **Problem.** The Angular guidance says no tool renders a page while `look_at`
  exists for vision models (verification 7.7).
- **Evidence.** `crates/pwr-orchestrator/src/context.rs:384`.
- **Change.** The sentence is built from the catalogue the turn offers: with
  `look_at`, it says how to use it; without, it says the page cannot be seen.
- **Tests.** Guidance text with and without `look_at` in the catalogue.

### W4.6 Make retrieval failures visible

**Status:** NEXT · S

- **Problem.** A retrieval error becomes an empty section silently (verification 7.6).
- **Change.** A retrieval error is recorded as an event and shown in the
  context composition ("retrieval failed: …"); the turn goes on without it.

### W4.7 Compare the context policies on the product path

**Status:** LATER

`current`, `recency-fill` and `evidence-state` exist as scripted-loop options
(`eval run --context-policy`). After G2 they are compared on the executor with
W8's protocol. The winner, or the simplest if none wins, becomes the default;
the others are removed.

---

## W5 — Inference robustness

### W5.1 Cancel during prefill

**Status:** NEXT · S

- **Problem.** Stop is not seen during a long prefill, in the sidecar's own
  prefill loop or in `stream_generate`'s (verification 11.1).
- **Evidence.** `crates/pwr-mlx/sidecar/pwr_mlx.py:734-743`, `925`.
- **Change.** `prefill()` checks the inbox's cancel flag between chunks; the
  `prompt_progress_callback` raises a cancellation the generation path turns
  into `finish_reason: cancelled`. The cache is left in a state the next
  request can trust (cut back, or dropped).
- **Tests.** Sidecar unit test with a fake model whose prefill takes several
  chunks: a cancel mid-prefill ends the request before generation.

### W5.2 Background summaries off by default and pre-emptible

**Status:** NOW · S

- **Problem.** Module summaries run after every turn, occupy the only engine
  and produce unverified text (verification 11.2; review, feature audit REMOVE).
- **Change.** Off by default (a Settings switch, per workspace). When on, a
  prompt that arrives cancels the summary in flight (with W5.1's cancellation).
- **Tests.** `serve.rs`: no summary starts after a turn by default; with the
  switch on, a prompt cancels it.

### W5.3 Check that the request fits before loading

**Status:** NEXT · S

- **Problem.** `prepare_context` returns the window asked for without checking
  the tokenized request fits (verification 11.3).
- **Change.** With W4.2's count, refuse a request whose prompt plus
  `max_tokens` exceeds the window, with the numbers.

### W5.4 Separate protocol support from turn behaviour on llama.cpp

**Status:** LATER · S

- **Problem.** llama.cpp requests `tool_choice: required` whenever tools are
  sent; a conversation there cannot answer in prose (verification 11.4).
- **Change.** `auto` in conversations, `required` only where the executor's
  policy asks for it. Part of the llama.cpp end-to-end path (Later).

### W5.5 Say when the engine is busy

**Status:** NEXT · S

- **Problem.** One generation runs at a time; a person waiting behind a
  summary or a review sees no reason.
- **Change.** The core reports engine occupancy (`_pwr/turn_event` or a new
  notification); the app shows "engine busy: <what>".

---

## W6 — The journal and the state

### W6.1 A journal that holds under concurrent writers

**Status:** NEXT · S

- **Problem.** `Store::append` reads the last hash and inserts without a
  transaction; read errors become "no previous hash"; a `NULL` per-run link
  verifies as unlinked; no WAL, busy timeout or indexes (verification 15.2,
  15.3, N4).
- **Evidence.** `crates/pwr-store/src/lib.rs:100-155`, `188-230`.
- **Change.** `BEGIN IMMEDIATE` around read-and-insert; errors propagate;
  `journal_mode=WAL`, `busy_timeout`; indexes on `(run_id, rowid)` and
  `(run_id, event_type)` through a numbered migration; a per-run link that is
  `NULL` after a linked event verifies as broken. Call it a *verifiable log*
  everywhere; no external anchoring is planned.
- **Tests.** Two connections appending concurrently keep both chains valid;
  a stripped link is reported broken; migration from the current schema.

### W6.2 Declare what each piece of state is

**Status:** NEXT · S

- **Problem.** Overlapping stores without an ownership hierarchy (verification 3.4).
- **Change.** The map in [state-and-persistence.md](../state-and-persistence.md)
  becomes enforced: projections (index, wiki, graph, summaries, embeddings,
  outputs) can be deleted and are rebuilt on demand, which a test proves by
  deleting them; nothing that decides behaviour lives in a projection.
- **Tests.** Delete every projection, run a turn: same behaviour, projections rebuilt.

### W6.3 Bound conversation snapshots

**Status:** NEXT · M

- **Problem.** Every turn appends the whole message list to the event log
  (verification 12.2).
- **Change.** Measure first: log size per turn on the small-apps corpus and a
  long session. If it matters, store the snapshot once per compaction and
  deltas between.

### W6.4 Resume with reconciled effects

**Status:** NEXT · M

- **Problem.** `--continue` and the app's resume restore messages and tell the
  model what changed on disk; effects of an interrupted turn (a command that
  may or may not have run) are ambiguous.
- **Change.** On resume, every action recorded as started without a recorded
  result is listed as *unknown effect* in the outcome and to the model; file
  effects are reconciled by hash against the known-versions map (W1.1).

### W6.5 Evict the embedding cache

**Status:** LATER · S

With semantic retrieval (experimental): drop entries for sections that no
longer exist, cap the file.

---

## W7 — The desktop: say what matters first

### W7.1 The turn's outcome is the primary status

**Status:** NEXT · M

- **Change.** The end of every turn shows, in this order: what it did, which
  files changed, what is left, what the checks said (W2.2's mark), why it
  stopped, which decision is needed. From `TurnOutcome`; nothing new in the core.

### W7.2 3D graph out of the default product

**Status:** NOW · S

- **Change.** The Knowledge card opens on a searchable outline (files,
  symbols, imports, recent changes) built from the same graph; the 3D view
  moves behind an *Experimental* switch and its libraries (`3d-force-graph`,
  `three-spritetext`) are loaded lazily, only when switched on.
- **Done when.** A default build's main bundle does not contain the 3D
  libraries.

### W7.3 Advanced information behind Advanced

**Status:** NEXT · S

- Token accounting, context composition, compaction parameters, calibration
  diagnostics, backend diagnostics and reasoning streams stay available, one
  click away, not in the primary view. The reasoning stream is not the main
  "working" indicator: the current action is.

### W7.4 Confinement on every turn

**Status:** NOW · S

- **Change.** A turn that ran any command unconfined (Full access, or an
  `outside_sandbox` grant) says so in its header, from `TurnOutcome.confinement`.

### W7.5 Graphical defects from the manual walk

**Status:** NOW · ongoing

Carried from the v0.3.0 plan: collect them from the manual walk of the
current build (Focus, resizable cards, the three trace views, light and dark
themes, narrow windows, full screen) and fix each with a screenshot before and
after. Record in `docs/release/` as `v0.2.x-mac-verification.md` does.

### W7.6 End-to-end desktop test

**Status:** LATER · L

First launch, engine install, workspace change and process shutdown are not
covered by CI (review §16). A scripted native walk (Tauri driver or the
built-in browser against `tauri dev`) is planned after G2.

### W7.7 Gated models and disk pressure in the Model Manager

**Status:** NEXT · S

Old backlog B.8. A gated Hugging Face repository says it needs an access token
(`HF_TOKEN`) and where to get one, instead of failing as a generic download
error; a download refused for disk space names the space needed and the
margin (the core already refuses it, `crates/pwr-models/src/download.rs:214`).

---

## W8 — The decisive benchmark

The review's fourth "build next", and the reason for the order above.

### W8.1 Strict pairing by default

**Status:** NOW · S

- **Change.** `eval compare` pairs strictly by default; the old behaviour
  moves behind `--legacy-pairing`, and its output is headed "not a causal
  comparison".
- **Tests.** Two campaigns differing in `corpus_rev` are refused without
  `--declare corpus_rev`.

### W8.2 Separate unattended success from success with nudges

**Status:** NOW · S

- **Change.** The stack-matrix runner's reports and any product-path runner
  report: success on the first cycle (no intervention); success after generic
  interventions, with their number; the cost (tokens, minutes) of each. The
  nudge is an intervention and is never counted as unattended.
- **Tests.** The report generator on recorded `result.json` fixtures.

### W8.3 Define the simple loop

**Status:** NEXT · M (after W2.4)

- **Change.** The baseline arm: the objective fixed in the prompt, the recent
  history (recency fill to the same window), files read on request, the same
  tools, policy, sandbox, model, window and reasoning budget as PWR; no
  retrieval, no ledger, no framework guidance, no completion holds beyond
  "complete is a tool". It runs on the executor as a declared policy so that
  the difference between arms is the harness, not the plumbing. B0 in
  `baseline.rs` is the starting point.

### W8.4 The confirmatory campaign

**Status:** NEXT · L (after G2)

- **Question.** On the runtime the app ships, does a 9B and a 14B model with
  PWR beat the simple loop on new tasks at equal budget, without more false
  acceptance or human interventions?
- **Deployments.** One ~9B and one ~14B MLX deployment, chosen and pinned
  (artifact digest, quantization, engine version) in the preregistration.
  Candidates from the 2026-09-29/30 campaigns: Ornith-1.5-9B, Qwen3-14B.
  Same Mac, same window, same reasoning effort for both arms.
- **Tasks.** New tasks never used in development: bounded repository changes
  (diagnosis and repair, small features, limited refactors) in existing
  repositories, each with a hidden verifier and a reference solution, split
  from the stack matrix's held-out set plus new ones. At least 30 tasks, at
  least 3 seeds each.
- **Measures.** Hidden-verifier success on the first cycle (primary); success
  with interventions and their count; false acceptance (the agent or PWR says
  done/verified, the hidden verifier fails); wall-clock and generated tokens
  per task; accepted tasks per hour.
- **Analysis.** Paired by task and seed; absolute uplift in percentage points
  with a confidence interval over tasks; per-deployment, never pooled.
- **Preregistration.** Deployments, tasks, seeds, budgets, thresholds and
  this decision rule are committed before the first trial, in
  `docs/thresholds.json` (a new dated amendment) and the experiment log.
- **Decision rule.**
  - PWR's primary success beats the simple loop by the preregistered margin on
    at least one of the two deployments and does not lose on the other, with
    no more false acceptance → the harness's adaptive mechanisms stay, and
    each is then ablated (W8.6).
  - Otherwise → the adaptive layer is cut back to what the executor needs to be
    safe and honest (effects, verification, outcome, budgets); retrieval
    heuristics, framework guidance, evidence-state compaction, calibration-driven
    strategy and other adaptive parts are removed or moved behind experiment
    switches, and the product is positioned as a dependable local agent without
    the claim of compensation.

### W8.5 Publish the evidence

**Status:** NEXT · M

- **Change.** The campaign's protocol, manifests, per-trial outcomes and
  analysis are published in the repository (or a companion repository),
  anonymised where needed. This settles the open policy on git-ignored
  evidence (old backlog R.9) for this campaign; older campaigns stay private
  and are cited as such.

### W8.6 Ablations

**Status:** LATER

Only if W8.4 keeps the adaptive layer: remove one mechanism at a time
(retrieval, framework guidance, completion holds, reasoning budgets, context
policy) and keep only what earns its cost.

### W8.7 Stress and agent-mode matrix

**Status:** NEXT · M

Carried from the v0.3.0 plan and kept, because they measure robustness, not
uplift: long sessions through many compactions; large repositories; cancel,
rewind and resume; the app or the engine killed mid-turn; low memory and
models near the machine's limit; network off; long-running commands and
services. And the agent-mode model matrix: every model the Model Manager
offers, on the same small fixed tasks, one recorded result each, shown in the
Model Manager. Thresholds written before the run.

---

## W9 — Distribution and reproducibility

### W9.1 A release is built from a commit that passed the gates

**Status:** NOW · S

- **Change.** The release workflow runs the same jobs as CI (fmt, clippy,
  Rust tests, desktop tests and build, sidecar tests) on the tagged commit,
  and builds the DMG only after them; the tag and notes file come from the
  pushed tag, not a hard-coded `v0.2.0-alpha` (verification 24.3, N8).

### W9.2 Lock the engine's environment

**Status:** NEXT · S

- **Change.** A lock file for the engine's Python packages with hashes,
  transitives included (`uv pip compile --generate-hashes`), installed with
  `--require-hashes`; the Python version pinned to a patch release. Used by
  the app's installer, `scripts/setup-mlx.sh` and the sidecar CI job.

### W9.3 Licence and supply-chain inventory

**Status:** NEXT · S

Old backlog R.6: a generated licence inventory for Cargo, npm and the engine
lock, shipped with the release.

### W9.4 Notarization

**Status:** LATER

Old backlog E.3; ad-hoc signing continues until then, as the README says.

---

## W10 — Less surface

### W10.1 Audit selection and certification

**Status:** NEXT · S

- **Problem.** `pwr models select`, `certification`, `certify`, the Verified
  registry and the certification levels are more surface than there is
  evidence for (review §14, §15).
- **Change.** Trace what reads each (the app, the executor, the evaluator).
  What nothing reads is removed; what the Model Manager uses is kept and
  documented; the Verified registry stays empty until W8's campaign produces
  evidence of the kind it requires.

### W10.2 Simplify the wiki

**Status:** NEXT · S

Keep the inventory, the work log and the person's confirmed memories;
summaries are off by default (W5.2); the graph serves the outline (W7.2) and
`wiki_query`. No new memory form before W6.2's ownership map holds.

### W10.3 Retire the permissive comparator

**Status:** LATER

After W8.1 has been the default through one campaign, remove
`--legacy-pairing` if nothing needs it.

### W10.4 Break up the god functions along their decisions

**Status:** NEXT · with W2.3

`take_turn_inner`, the scripted runner and `main.rs` are split where W2's
contracts cut them (budget, acceptance, action execution, persisted state),
not by size.

---

## Later

Only after G3, and only what its answer supports:

- Semantic retrieval and evidence-state compaction, if they beat the simple
  alternatives under W8's protocol (W4.7).
- A complete llama.cpp path: a server kept alive across turns (old R.4),
  W5.4, then a second operating system with its own command isolation (old
  E.1, E.2, E.4). Windows was the v0.3.0 plan's main item; it moves here
  (decision D-2026-09-30-4).
- Vision for selected UI tasks, with an independent browser acceptance check,
  not screenshots that merely succeed (`look_at` stays experimental).
- Versioned documentation retrieval and KV-cache quantization (old C.22c),
  as experiments against fixed baselines.
- A desktop end-to-end test (W7.6), notarization (W9.4).

## Not now

Decided in [decisions.md](../decisions.md) (D-2026-09-30-3): multi-agent
execution; automatic model routing (old C.23); a richer 3D knowledge graph;
generalised semantic memory; critic/consensus with extra calls; a general
browser or computer agent; a plugin/MCP marketplace; Windows/Linux parity now;
a new PDF/OCR stack; optimising for the maximum context; a full certification
system; enterprise audit features. Each can be reopened by a dated decision
citing new evidence.

---

## Carried over from earlier plans

### v0.2.x fixes and the v0.3.0-alpha plan (2026-09-28)

| Item | Where it is now |
|---|---|
| Models that fail in agent mode: record, reproduce, fix in the harness or mark unsuited | W8.7 (agent-mode matrix); per-model fixes continue as found, with the rule that a fix must help every model |
| Graphical bugs from the manual walk | W7.5 |
| .NET installed per user; `look_at` on GitHub's runners | Done 2026-09-29 (`docs/release/v0.2.x-mac-verification.md`) |
| llama.cpp kept alive across turns (R.4) | Later |
| Command isolation on Windows; Windows installer and parity | Later (decision D-2026-09-30-4) |
| Agent-mode model matrix | W8.7 |
| Stress tests | W8.7 |
| Repeated campaigns (pass@k) | W8.4 (seeds), W8.7 |
| Architecture review from stress results; scripted vs conversation divergence | W2.3, W2.4 |
| Release gate: CI green without CI-only skips; stress suite; manual walk; model matrix; known limits | G1, W0.2, W7.5, W8.7, W9.1 |
| After 0.3: C.22b, C.22c, MoE expert-routing experiment, speculative decoding, public evidence site | Later (C.22b/C.22c); MoE routing and speculative decoding: not now, no evidence yet that inference speed is the binding constraint for the benchmark; public evidence: W8.5 |

### Still open from the small-model campaigns (2026-09-29/30)

| Item | Where it is now |
|---|---|
| Run loop and conversation loop keep separate guards; campaigns do not measure the desktop's path | W2.4, G2 |
| The project's own verification never ran in real use: no checks declared | W3.2 (offer to declare the project type's usual checks) |
| Qwen3-14B answered the same prompt four ways across four runs | W8.4 (seeds, per-deployment variance reported) |
| Thirteen more models for a tiered campaign; Nemotron-3-Nano-4B and Kimi-Linear-48B refused (need repository code) | W8.7; the refusal stands (PWR never runs repository code) |

### Old backlog, every open item

The backlog's last reconciliation (2026-09-27, [archive/backlog.md](../archive/backlog.md))
left these items *parziale* or *proposto*. Each is placed here.

| Item | Placement |
|---|---|
| A.3 tool-call replay, live multi-family coverage | W8.7 (matrix) |
| A.5 editing fixtures, discriminating small-model comparison | W8.4 |
| A.6 verification fixtures, product-path adoption | W2.4, W3 |
| A.7 compaction fixtures, frozen area and gate | W4.1, W4.7 |
| A.8 stack matrix scored beyond nine tasks | W8.4 (tasks), W8.2 |
| A.9, A.10 six-area suites and their gates | Not now: the area suites stay as regression checks (`eval suite`); their promotion gates wait for W8's result |
| A.11 durable per-task campaign reporting | W8.2, W8.5 |
| A.12 continuous memory accounting | W8.7 (low-memory stress) |
| A.13 artifact schema upgrades | W6.1 (migrations) |
| A.14 human legibility rubric | Not now |
| A.15 computed configuration vs product-level comparison | W8.4 |
| A.16 build fingerprint, report attribution | W8.4 preregistration pins it |
| A.17 long-prefill and per-deployment performance | W5.1, W8.7 |
| R.4 persistent llama.cpp server | Later |
| R.6 licence inventory | W9.3 |
| R.9 publishing anonymised evidence | W8.5 |
| R.10 paired small-model comparison | W8.3, W8.4 |
| R.11 one contract for both loops | W2.3, W2.4 |
| B.3, B.3a measured catalogue, evidence-backed shortlist | W8.7 |
| B.3b weight-quantization comparison at equal memory | Later |
| B.4 16 GB cohort | W8.7 (models near the limit); a 16 GB machine is needed to run it |
| B.7 GGUF path completeness | Later |
| B.8 gated downloads, disk pressure | W7.7 |
| B.11 unknown-model states, backend coverage | W10.1 |
| C.1 one compaction for both loops | W2.4 |
| C.2 shared budget defaults | W1.4, W2.6 |
| C.3 evidence-based context allocation | W4.7 |
| C.4 learned small-decision classifier | Not now |
| C.5 diagnostic/check-selection scope | W3.2, W3.5 |
| C.6 semantic/procedural memory | Not now (decision D-2026-09-30-3) |
| C.11 live operating-regime diagnosis | W5.5, W7.3 |
| C.13 complete call graph | Not now; the graph serves the outline (W7.2) |
| C.14 staged clean-context execution | Not now |
| C.15 recovery by returning a runaway reasoning tail | Not now; reasoning budgets stay |
| C.21 skill-pack library | Not now; framework guidance derived from the catalogue (W4.5) |
| C.22 semantic document retrieval | Later (W4.7 protocol) |
| C.22b evidence after compaction | Later (W4.7) |
| C.22c documentation RAG, KV-cache quantization | Later |
| C.23 FAST/STANDARD/DEEP router | Not now |
| C.24 per-model configuration and loop parity | W2.4; per-model configuration only through profiles |
| C.25 vision retention, GGUF images | Later |
| C.12 web search, versioned documentation | Later (with C.22c) |
| C.7 adaptive-policy advantage | G3 |
| C.8 product gate on held-out tasks | W8.4 |
| C.9 browser/MCP/apps breadth | Not now |
| C.10 R3 confirmatory resumption | Superseded by W8.4 on the product path |
| C.19 same-run stale-hash recovery | W1.1 |
| D.4 permission usability with unfamiliar users | W1.9 (texts); a usability session is Later |
| D.6 Model Manager backend switching, Windows | Later |
| D.9 per-kind permission configuration | Not now; three modes plus per-question answers cover it |
| D.10 guided first task | Later |
| D.11 verification and stop-class usability | W7.1 |
| D.12 offline and recovery UX | W8.7 (network off), W6.4 |
| D.14 persistent reject-always | Not now |
| D.15 read-only mode in a coding workspace | Not now |
| D.16 whole-app localization | Not now |
| D.17 notarization, product acceptance | W9.4, W7.5 |
| D.E2E-5 model-specific corrupted code | W8.7 (reproduce on the matrix, or close) |
| D.E2E-12 model-facing page inspection | Merged on `develop` as `look_at`; stays experimental (Later) |
| D.E2E-15 document outline cost/quality | Not now |
| E.1, E.2, E.4 Windows isolation, core path, cross-platform guarantees | Later |
| E.3 signing and notarization | W9.4 |
| F.1 structural debt in the orchestrator | W10.4 |
| F.2 artifact table and migrations | W6.1 |
| F.3 obsolete calibration gate code | W10.1 |

## Keeping this plan true

- When an item changes status, change it here, in [roadmap.md](../roadmap.md)
  and, for a gate, in [milestones.json](../milestones.json)
  (`python3 scripts/milestones.py` regenerates the table CI checks).
- An item is not marked DONE until its "Done when" holds on `develop` and the
  commit is named beside it.
- A new problem found while working goes into this plan with an ID, not into a
  new backlog.
