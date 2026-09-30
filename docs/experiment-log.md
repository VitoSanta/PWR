# Experiment log

Dated entries, newest first, for every experiment and every change that
alters what a campaign measures (prompt, catalogue, loop, budgets, adapters).
Each entry names: the hypothesis or question, the conditions compared (commit,
deployment, corpus revision, arm, seeds), the outcome with counts —
including an inconclusive or negative one — and what was kept, revised or
removed. Raw artifacts are cited by path even when they are not public.

Entries through 2026-09-17 are in the [archived log](archive/experiment-log.md);
the archived [roadmap](archive/roadmap.md) and [backlog](archive/backlog.md)
hold the campaign notes of 2026-09-18 to 2026-09-28.

---

## 2026-09-30 — Review verified; no experiment run

The technical review of 2026-09-30 was checked claim by claim
([verification](reviews/2026-09-30-verification.md)). No campaign was run. The
Rust suite was run once (one environmental failure, the Docker test). The
decisive comparison is specified in the
[implementation plan, W8.4](plan/implementation-plan.md#w84-the-confirmatory-campaign)
and has not started.

## 2026-09-29/30 — Small models building from scratch

**Question.** Where do 9–20B models fail when asked to build small projects
from nothing, and which harness changes help every model?

**Conditions.** `corpus/small-apps-v1.json` (four tasks: a bank login page, a
unit-converter CLI, an HTTP todo API, an inventory module with tests; hidden
verifiers that run the code), scripted loop, arm B1, seed 1, one trial per
task, grants equal to the desktop's Standard mode; harness changing between
campaigns. Deployments: gpt-oss-20b, Qwen3-14B, Ornith-1.5-9B,
Qwen2.5-Coder-14B (MLX, maintainer's M2 Max). Artifacts under
`.pwr/small-runs/` (local).

**Outcome** (hidden-verifier passes, first campaign → last measured):
gpt-oss-20b 3/4 → 4/4; Qwen3-14B 2/4 → 3/4; Ornith-1.5-9B 1/4 → 3/4 (third
campaign stopped); Qwen2.5-Coder-14B 0/4 → 1/4. One trial per task:
**development evidence, not a comparison**. Qwen3-14B answered the same bank-page
prompt four different ways across four runs.

**Kept** (for every model): an import that does not resolve is answered with
the install to run; edit results carry the changed lines; `complete` over
unread results waits a turn (both loops); a scripted run that built a program
and only syntax-checked it is asked once to run it; a command failing the
same way twice is not run a third time; five failed runs after edits → hand
over; `file://` placeholders named; an `npx` that fetched a missing package
says so; a conversation's `complete` before anything was done is asked about
once (`1d657268`); format repairs (invented tool names and argument
spellings with one reading, quoted `cwd`, Qwen2.5's fenced and unterminated
calls, `<|endoftext|>` as a stop token); Ornith-1.5-9B's profile at the
vendor's temperature 0.6; `eval run --reasoning-effort`.

**Open.** Several of these fixes exist in one loop only; the campaigns
measured the scripted loop, not the app's (plan W2.4). Full record:
[release/v0.2.x-mac-verification.md](release/v0.2.x-mac-verification.md).
