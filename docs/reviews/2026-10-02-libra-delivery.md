# Libra manual-test delivery repair — 2026-10-02

## MEASURED failure

The owner tested Nemotron 3.5 Lightning 30B A3B using the prepared C01–C15 desktop build. Its workspace remained empty while PWR displayed “1 edited” and “Work delivered”. The read-only journal contains a failed write to invented `/Users/roberto/LibriEcommerce`, a tree listing and 45 commands, mostly filesystem discovery and private `.pwr` reads. No project-file creation followed. Evidence: `/Users/vitosantanelli/Desktop/pwr-evidence/logs/libra-manual-20261002/REPORT.md` and `events-summary.json`.

## IMPLEMENTED corrections

- The ordinary executor separates observed file changes from `edited`, which conservatively means possible effects requiring checks. Nonempty completion rationales and command results do not manufacture artifact delivery. Read-only prose answer turns can still deliver an answer. An artifact-free structured `complete` remains only a model claim, including after the empty-completion reminder. A successful file change is partial artifact evidence, never acceptance of the whole requested task; command-only artifacts without a file receipt remain unknown.
- ACP preserves the executor's delivery evidence rather than recomputing it from the answer plus `edited`. Verification summaries cannot manufacture delivery.
- The desktop unavailable-check message says “Turn ended” and describes the missing verification. It does not assert the requested work was delivered, including for legacy journal entries with `delivered: true`.
- Snapshot replay recognizes `tool_failure`, explicit failure results and calls marked not run/not completed. Failed edits no longer reappear as successful edits. Live nonzero command exits are shown as failures too. The UI count says “failed” for its combined error/refusal category, rather than implying all failures are policy refusals.
- A `complete` after failed file writes and no successful file write is held once, with a failure-specific recovery result. The next explicit attempt can recover or report the obstacle. A subsequent completion claim is not a verified success.
- The prompt exposes a JSON-quoted workspace anchor, default/relative cwd, empty-project bootstrap and the private-state boundary. Failed file results name these rules without claiming all project writes are blocked. This changes model-facing behavior; effectiveness with the real Nemotron deployment is **UNKNOWN** until the owner repeats the manual task.
- The failed-command handover message no longer invents files “you wrote” from conservative command-effect flags.

## Regression evidence

Before repair, deterministic fake-provider tests failed because README.md was never created after a failed first write and delivery was true after only failed writes/listing. The replay test failed because an I/O failure was labeled completed. The desktop test failed on the “Work delivered” message. A separate real shell exit-1 regression failed because the live trace labeled it completed (`command-failure-red.log`). The repaired bootstrap test creates a real README.md in a temporary workspace; no model inference is used.

The old unit contract forbidding an absolute root in the prompt is superseded
by D-2026-10-02-2: it now checks a JSON-quoted location anchor, default cwd and
the retained relative tool-path convention, including quote/newline escaping.
This model-facing choice is a hypothesis to measure, not an established gain.

The ACP golden transcript changes only the second turn's `delivered` true→false: its requested edit was refused. The first, successful edit remains delivered. This corrects evidence semantics; verification and acceptance checks are unchanged. Stop/timeout intent uncertainty and the existing investigation/no-progress guards remain intact.

Validation logs live in the same evidence directory. No sandbox permissions were expanded and the owner's Libra workspace/journal was not changed. The remaining command-variant loop is not claimed solved generally; the prompt/recovery changes need a fresh manual run before the battery.

## HYPOTHESIS / next measurement

The trusted location plus failure-specific recovery should reduce outside-path
discovery and increase project-file delivery on this task. The manual re-test is
exploratory; a later paired dev comparison must hold the prompt, initial empty
workspace, deployment, sampling and engine fixed, with serial runs. Record real
file artifacts, outside/private-state attempts, actions and duration. Reject any
artifact-delivery claim without receipts; retain the recovery intervention only
if the paired product-path comparison supports it under the mission's declared
statistical plan. No heldout task is used for this repair.

## Validation

MEASURED on this working source: full Rust workspace/all-targets 1,457 reported
passes, zero failures, five live-inference ignores (91 targets); Clippy all-targets
with warnings denied and formatting passed. Desktop 110 tests/14 files and
production build passed; pinned offline Python sidecar 43 tests passed. The
Docker boundary test ran and passed against the available daemon. Milestone
manifest and whitespace checks passed. No inference or battery was started.
