# Verification

**Checked against `develop` at `bff93062`, 2026-10-01.** How PWR finds a
repository's checks, when it runs them, what a result is allowed to claim, and
where the contract is incomplete. Crate: `crates/pwr-verify`.

## What changed since the baseline (`0776ff4f`)

A goal freezes `.pwr/checks.json` and the selected verifier artifacts before
baseline verification. Declare them explicitly when possible:

```json
{"checks":[{"executable":"npm","args":["run","e2e"],"kind":"acceptance"}],
 "acceptance":{"artifacts":["tests/**","scripts/check.mjs","playwright.config.ts"]}}
```

Artifacts accept workspace-relative paths, directories, and `*`, `**`, `?`
globs (no bracket expressions). With no declaration, PWR infers conventional
test/fixture/script files, manifests and direct executable-script arguments;
Cargo acceptance tests also include Rust source containing inline test modules.
Inference is recorded as inference and cannot discover arbitrary dependency
chains: owners should declare fixtures and configuration explicitly.

Hashes are persisted separately from compressible messages. A changed, added
or removed selected artifact produces `contract_changed`, even in Full access.
The app offers human review and a permission question naming one file; a grant
covers that file for this session and is journaled before it takes effect.
Authorization is absent from the model tool catalogue. PWR's own `.pwr` state
remains immutable to model tools; changes to the check declaration are made by
the person and can subsequently be reviewed/authorized. Legacy resumed sessions
without an original snapshot cannot acquire fresh evidence for prior edits.

`TurnOutcome` distinguishes delivery, checks, baseline, acceptance, confinement
and budgets. Conversation/ACP/app use it; the scripted runner migration remains
open (W2.1/G2). Green compilation alone never grants `acceptance: accepted`.
Post-edit failure marks follow typed results, and model feedback uses a user
message with `VerificationFeedback`, never a tool result without a call id.
Failure fingerprints also compare reproductions and goal repetition. Known
zero-test signatures are handled for Rust, Python, JS, Go and .NET; this is
signature detection, not proof of coverage.

## What each mechanism can prove

| Mechanism | What it shows | What it does not |
|---|---|---|
| A build, compiler or type checker | the properties that tool checks | that the request was met |
| The repository's tests | the behaviour they cover | behaviour they do not cover, or tests that were weakened |
| Baseline before/after | that checks passing before still pass | anything about checks that did not exist or already failed |
| Web asset check | that files a page references exist | that the page works |
| Diff and hashes | what changed on disk | whether the change is right |
| Review by the same model | a second opinion | independent verification |

## Discovering checks

`discover_checks` (`crates/pwr-verify/src/lib.rs`), first source that
yields anything wins:

1. **`.pwr/checks.json`** — the owner's declaration:
   ```json
   {"checks": [
     {"executable": "npm", "args": ["test"]},
     {"executable": "npm", "args": ["run", "e2e"], "kind": "acceptance"}
   ]}
   ```
   A check with `"kind": "acceptance"` is an **acceptance check**: the owner's
   executable evidence that the requested behaviour works. Malformed or unreadable declarations now fail closed in both discovery
   and acceptance reading; they cannot silently fall through to another source.
2. **CI configuration** — commands read as text from the CI files: `run:`
   lines and `script:`/`commands:` lists; steps that chain, pipe, redirect or
   use `$`/`{}` are skipped; deploy/publish/push steps excluded; steps whose
   words read as verification preferred. `working-directory`, `env` and
   multi-line `run: |` are not understood (plan W3.2).
3. **`package.json` scripts** — `npm run build` and `npm test` (Angular with
   `--watch=false`).
4. **A build system by marker file** — 19 known: Docker Compose, Cargo, Go,
   Maven, Gradle (Groovy and Kotlin), .NET, Swift, Flutter, Elixir, Poetry,
   Python (`pyproject`, `setup.py`, `requirements.txt`), Ruby, PHP, Make, CMake.
   For Cargo, *targeted* runs `--lib` or `--bins`, *full* the workspace.
5. C# projects at the root, then nested Cargo manifests and C# projects.
6. Last, for a page with no toolchain: the **web asset check** (`web.rs`). It
   reads the markup as text and follows every `src` and `href` that names a
   local file; another origin, a fragment, a `data:` URL and a **run-time
   placeholder** (`${…}`, `{{…}}`, `{%…%}`, `<%…%>`, `<?…?>`) are left alone.
   Placeholders were flagged until 2026-09-30, when a working site was reported
   as failing because a script's card template carried `<img src="${car.image}">`.

## Running them

Checks run through the same tool runtime as the agent's commands — the same
sandbox, and the approvals of the session (a check that restores packages
gets the network the person allowed). Their executables are added to the
allowlist for the run.

`classify` sorts a failure into `Compilation`, `Assertion`, `Environment`,
`Provider`, `Policy` or `NonDeterminism` by patterns in the output: sandbox
denials and unreachable networks are environment, not code; warnings are
stripped first; "not found" is environment unless the output points at a
source location. It is a heuristic, not a taxonomy.

`classify_with_reproduction` re-runs a failing check and compares the **failure
identity** (a fingerprint of the failing tests or errors for Rust, pytest, Go,
.NET, Jest, Vitest, and from 2026-10-06 `node:test` and Python `unittest`), not
only the exit code (plan W3.3); for other
toolchains the output's own shape decides and two different failures with
exit 1 can still look alike.

## After a conversation turn

`executor::close_turn` (`crates/pwr-orchestrator/src/executor.rs`), called from
`run_chat_turn` in `main.rs` with the policy the front end prepared, if the turn
edited files:

- with a baseline from before the turn → the verdict compares before and
  after (green, regressions, still failing);
- with checks discovered only after the edit → their result, "no prior
  baseline";
- a check that ran zero tests → said explicitly, for cargo, pytest, Jest, Vitest,
  Go and .NET (signature detection, not proof of coverage; other runners are
  plan W3.5, partial);
- no checks → *"Independent verification unavailable: this workspace declares
  no automated checks"*.

The verdict is appended to the answer, shown as a note whose mark follows the
verdict (`✓` green, `–` unavailable, already failing or zero tests, `✗` new
failures, `!` could not run), recorded in the turn's typed outcome, and handed
to the model as a message with the purpose `VerificationFeedback` — the
harness's, not an orphan tool result. Tested with a real check that flips
outcome (`executor::tests`). The turn has already ended; a failing verdict does
not send the model back (plan W7.1 decides whether it should).

## Goal acceptance

On each `complete` in Goal mode, the executor asks its host for the full
verification (`verify_goal` in `main.rs`) and decides:

- **passed** only if nonempty technical checks pass and no recognized runner
  reports zero tests, a declared acceptance check exists, and **`.pwr/checks.json` and the frozen acceptance artifacts have the
  hashes they had when the session began** (`acceptance_contract_hash`, `crates/pwr-cli/src/serve.rs`) — so a goal cannot create or relax its
  own contract file;
- checks pass but no acceptance check is declared → *"Technical checks
  passed, but the goal is not verified"*;
- no repository or acceptance checks exist → independent verification
  unavailable, never a passing technical check;
- only checks that failed before the goal fail, none of them acceptance →
  ended, naming them;
- otherwise → back to the model with the evidence; the same failure
  fingerprints three times → *blocked*.

Before ending, one **review round** (also when the technical checks pass and no
acceptance check is declared, from 2026-09-30; the goal then still ends *not
verified*) reads the request against the
changed files (same model, reasoning bounded to 4,000 tokens). Its findings
are guidance, not verification.

**What remains of the gap:** the artifacts an acceptance command runs are
frozen from what the declaration names or, failing that, from conventions
(tests, fixtures, scripts, manifests); a dependency chain the conventions
cannot see is not covered, so owners should declare artifacts explicitly. Under
Full access the files can still be changed — the hashes then detect it and the
goal stops, which is detection and not prevention. The stack-matrix runner does
the equivalent from outside, by restoring the owner's tests in a clean copy
(`evidence/stack-matrix/runner/run.py`).

In Full access mode, checks and acceptance run unconfined.

## In the scripted loop

`pwr run` and `eval run` take a baseline before editing (narrow, broad and
explicitly quarantined checks), verify on `complete`, and recover:
reproduce, classify, give diagnostic feedback, and retry within
edit/verify and context-tier budgets (`RecoveryDecision`); they stop on
environment, policy or non-determinism. With no usable verifier a run ends
**completed, `verified: false, verifiable: false`** — completion is *not* refused, whatever older documents said. A person can
adopt a check the model proposes (`propose_verifier`, with the
`verifier_proposal` grant). With `--mode verifier-supplied` (the default for
`eval run`) the corpus's own check is handed to the agent; `--mode
product-path` lets discovery run as it would for a user.

## What a result may claim

Today each path reports in its own shape (`TurnReport`, `GoalVerification`,
`TaskRunResult`). The contract this is moving to (plan W2.1) separates:
terminal state; whether something was delivered; what the checks said
(including *unavailable*, *ran zero tests*, *could not run*); whether the
baseline held; whether acceptance was reached or its contract changed; and
whether commands ran confined. "Verified" is reserved for acceptance reached
under an unchanged contract.

## Known defects

| Defect | Plan | State |
|---|---|---|
| Acceptance froze the contract file, not the artifacts it runs | W3.1 | fixed on `develop` (conventions inferred when not declared); CI not run |
| Post-turn note `✓` regardless of the verdict; verdict appended as an orphan `tool` message | W2.2 | fixed on `develop` |
| Goal failures compared by check name | W1.5 | fixed on `develop` (fingerprints) |
| Flakiness judged by exit code | W3.3 | fixed for six toolchains |
| CI discovery reconstructs commands without `working-directory`, `env`, multi-line scripts, and is used as acceptance-grade evidence | W3.2 | open; malformed declarations now fail closed |
| Zero-test detection only for Cargo | W3.5 | partial: six toolchains |
| `complete` describes each policy's actual closing checks; unavailable checks never certify completion | W3.4 | IMPLEMENTED 2026-10-02; catalogue regressions |
| Three result shapes | W2.1 | partial: the typed outcome reaches the app; the scripted runner's JSON does not |
| With no declared acceptance check a goal is never *verified*; a review by the same model is the only further check | — | by design, stated in the answer |

Acceptance artifacts are checked again after verification commands finish. A
check that rewrites its own evidence cannot certify a goal. Explicit artifact
paths into excluded dependency/build/state directories are refused rather than
silently omitted.

The CLI verification JSON uses the same typed check evidence as closing turns:
an empty set or a recognized zero-test result makes `checks_passed` and
`verified` false. A zero-test declared acceptance check remains an acceptance
failure even when the same result existed at the Goal baseline. Scripted
completion likewise rejects recognized zero-test evidence and records the cause
in its diagnostic snapshot and terminal failure, retaining the existing
Environment recovery policy. Signature detection
is not a measurement of test quality or completeness.
