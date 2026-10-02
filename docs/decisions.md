# Decisions

Durable decisions, dated, newest first. A decision is changed only by a new
entry that names the one it replaces and the evidence that changed it. The
ADR series (ADR-001 to ADR-012) and the decisions scattered through the old
roadmap and backlog are in the [archive](archive/README.md); where one of them
still holds it is restated here.

---

## D-2026-10-02-1 — The stack-matrix runner is the product-path evaluator

**Decision, the owner's choice of 2026-10-02** (asked after the review of the
F1 commits, between this, the planned `EvalHost`, and a baseline first).
Product-path measurement for F3/F6 runs through the stack-matrix runner
(`evidence/stack-matrix/runner/run.py`), which drives `pwr serve --stdio` in
Goal mode exactly as the desktop app does and takes its verdict from hidden
tests in the task's container. Parity with the app holds by construction: the
measured code is the shipped code.

**Supersedes** the means of W2.4 (an `EvalHost` so that `pwr eval run --arm b1`
calls the executor), not its goal. `pwr eval run` and `pwr run` keep the
scripted loop as a declared research harness: their results describe that loop,
not the product, and carry no product claim. Removing them is a visible change
and needs its own decision. D-2026-09-30-6 stays as the parity inventory; its
rows matter for which mechanisms a minimal control leaves out.

**What the runner needs before a campaign it can confirm with:** an engine
lease (one inference engine at a time, refused otherwise) and full provenance
per run (binary and sidecar, model revision, engine libraries, effective
sampling and its sources, granted window, machine and load); a hash-frozen
dev/heldout manifest; arms -- PWR, the W8.3 minimal control behind the same
`pwr serve` plumbing, and eligible competitors against a matched endpoint for
the same weights -- with every difference that cannot be equalised listed; and
a paired analysis under the MASTER_SPEC statistics (exact McNemar or task-paired
bootstrap, Wilson intervals, Holm, unbiased pass@1, first-cycle results apart
from nudged ones). Competitor installs and model runs follow the owner-stop and
night-queue rules of the contract.

**Basis.** The stack matrix already measures the app's path; competitors are
external programs that a corpus arm inside `pwr eval` cannot run; an `EvalHost`
would have reproduced the server's turn composition beside it.

## D-2026-10-01-2 — Mission expansion and Windows exit gate

**Decision, approved 2026-10-01.** The owner accepted the F0 proposal and
authorized continuation ("ok procedi"). The [mission contract](../MASTER_SPEC.md)
sets the destination to the reference harness for local open-weight models,
with coding across S/M/L/XL tiers and objectively measured extensions through
a stable permission boundary. It does not assert current frontier-level
capability or approve any particular adaptive mechanism.

**Supersessions adopted with this decision:**

- D-2026-09-30-1: widen the destination beyond bounded 9B/14B repository work;
  retain inspectable effects, independent checks and controlled simplicity.
- D-2026-09-30-2: F1/F2 research and F3 measurement infrastructure overlap;
  correctness fixes proceed with failing tests, while claims and confirmation
  still require the product executor and G1/G2 evidence. G3/W8.4's old
  two-deployment decision is insufficient for the new all-tier exit gate.
- D-2026-09-30-3: reopen extension interfaces, memory, exploration subagents,
  context/cache and inference optimization as hypotheses requiring objective
  tasks, threat models and controlled cost/benefit evidence. This does not
  authorize a marketplace, automatic routing or a new PDF stack by default.
- D-2026-09-30-4: Windows waits for the new F6 all-tier exit criterion, with
  owner-approved numerical thresholds after the initial baseline, rather
  than merely the former G3 decision. Portable core interfaces continue now.
- D-2026-10-01-1 §5: clarify that a gate-directed push to `develop` may trigger
  CI; CI green is required to claim the gate, not before dispatching CI. No
  release is authorized by this amendment.

**Basis.** The owner's mission mandate of 2026-10-01 changes the intended
destination and sequencing. The [initial audit](reviews/2026-10-01-audit.md)
checks current executor divergence, context errors and evidence limitations;
it is not a measured argument that expanding scope improves performance.
The old decisions remain preserved below. No implementation/default change,
download, campaign, gate completion or release follows from contract adoption.

**Adoption evidence.** The former MASTER_SPEC is preserved in the archive;
the detailed plan retains W IDs and expands G3/W8.4 to the all-tier exit gate.
G1/G2 remain unpassed. Local checks are recorded in the initial audit; no
remote CI or capability campaign ran. Other sessions' uncommitted code and
research changes remain outside this mission commit.

## D-2026-10-01-1 — A compatibility verdict is about PWR's reading as much as the model; campaigns run one model at a time

**Decision.** From the day's direct tests on the maintainer's Mac:

1. **Quick Calibration asks whether a model can use tools**, not which tool it
   reaches for first (`tool_selection`, `tool_arguments`; `tool_choice` is
   recorded, not critical; suite `quick-calibration-6`). A verdict records the
   adapter revision that read the replies, and a **failure** recorded under
   none or under another revision is void: the adapter may have been the cause.
2. **A model refused as Limited is first a question about PWR.** The raw reply
   (`PWR_MLX_TRACE`) is read before the verdict is believed. Seven models were
   refused for PWR's own reasons on 2026-10-01 (call formats, a template's
   needs).
3. **Campaigns run one model at a time** on a machine: a second engine overruns
   the GPU's working set and both write nonsense without an error. Runs made so
   are discarded and kept apart; wall-clock times are comparable only for runs
   made alone.
4. **The mechanisms added that day are hypotheses until measured**, each
   recorded in [experiment-log.md](experiment-log.md): the review round without
   a declared acceptance check, the clean start after collapsed replies, the
   presence window, the rewrite note and the 32,768-token compaction ceiling.
5. **Nothing is pushed or released until the CI gates have run on it.**

**Why.** Seven working models were hidden behind "Limited"; a measurement made
with two engines was worthless; and none of the day's mechanisms had been
tested for its effect when it was written.

**Reversible by** a new entry with campaign evidence.

## D-2026-09-30-7 — Defaults that keep a local model usable

**Decision.** Under the maintainer's mandate of 2026-09-30 to make the agent
work fluidly on this Mac and to verify it, with every change general, tested and
logged, three defaults changed (all reversible by a workspace setting):

1. **Sampling floor.** When no profile, card, generation config or person
   declares a temperature, the request is 0.6 / top_p 0.95 / top_k 20, reported
   as source `pwr_sampling_floor`, instead of the engine's greedy decoding. A
   card listing one set per mode gives its thinking, coding set. Explicit
   temperature 0 stays greedy.
2. **Actions per turn** 26 → 100 (`actions_per_turn`); a goal's own limit is
   no longer capped at 26 inside a turn.
3. **Compaction ceiling** of 32,768 tokens (65,536 until 2026-10-01, lowered on
   the evidence of a reported silent KV-cache corruption near 60k tokens on the
   pinned MLX and of PWR's own collapse at 27k) under the percentage of the window,
   unless the person set a window (`context_tokens`) or a threshold. A
   hypothesis: nothing shows yet that a 4-bit model stays coherent above that
   size; what is certain is that every compaction or model switch re-reads the
   whole prompt, minutes at 100k+ on a 30B.

**Why.** Manual passes of 2026-09-29/30 (Qwen3.5-9B, GLM-4.7-Flash, Nemotron,
Qwen3-14B, Qwen2.5-Coder-14B): greedy repetition, a working model stopped at 26
actions, 32-minute prefills. Evidence and A/B in
[experiment-log.md](experiment-log.md).

**Reversible by** a new entry with campaign evidence; the compaction ceiling in
particular is to be tested against the window it replaces (plan W4).

## D-2026-09-30-6 — Dispositions for converging the scripted loop

**Decision.** The maintainer accepted, on 2026-09-30, the dispositions
proposed in [plan/executor-parity.md](plan/executor-parity.md), to be followed
rigorously on `develop` (with `stage` as the fallback if something goes wrong):

- Rows 1, 2, 4, 7, 17, 19: one budget, one sequence of checks, one approval
  interface — the executor's.
- Rows 5 and 6: both completion holds (nothing done; built a program and never
  ran it) apply on every path.
- Row 14: reasoning effort is always on in campaigns; a campaign that wants the
  old behaviour declares it as the treatment.
- Row 8: `record_progress` leaves the scripted catalogue; `propose_verifier`
  stays where a person can answer.
- Rows 3, 9, 10: the recovery cycle, `--plan` and the task ledger/`--session`
  stay with the scripted control (`legacy` arm) until W8.4 measures them; they
  are not part of the executor.
- Row 11: one compaction (the conversation's); `recency-fill` and
  `evidence-state` stay experiments until W4.7.
- The product arm `b1` is the executor; the earlier scripted B1 remains
  available as the `legacy` arm so older campaigns stay reproducible, and pairs
  with executor campaigns only under `--declare harness_rev` and `mode`.
- `pwr run` becomes a thin call of the executor.

**Why.** A campaign must measure the path the app ships (review §2, §3.2;
plan G2), and every behaviour that exists in only one loop is a fix a user
does not get or a measurement no user benefits from.

**Reversible by** a new entry naming the row and the evidence.

## D-2026-09-30-1 — Adopt the review's direction

**Decision.** PWR is positioned as a dependable local coding agent for
Apple-silicon developers, doing bounded repository changes with inspectable
effects and independent checks ([MASTER_SPEC](../MASTER_SPEC.md)). The
adaptive-harness thesis is a hypothesis to be tested once, on the product
path, before more is invested in it.

**Why.** The [technical review](reviews/2026-09-30-technical-review.md), every
claim of which was checked ([verification](reviews/2026-09-30-verification.md)):
the local runtime is real; the uplift is unproven (R2: 11/30 vs 11/30 on one
deployment, 18 vs 13 with p = 0.227 on another); the measured loop is not the
shipped one.

**Replaces.** The 2026-09-12 contract's destination of "an unrestricted coding
agent" as the framing of current work. Reaching further (browser, other
applications, network research) is not forbidden; it is sequenced after the
decision in D-2026-09-30-2.

## D-2026-09-30-2 — The order of work is effects, one path, then the benchmark

**Decision.** Work follows the [implementation plan](plan/implementation-plan.md):
safe effects and honest reporting (G1), then one executor for the app, the CLI
and the evaluator (G2), then the confirmatory benchmark (G3). New capability
work waits for G1; the benchmark waits for G2; everything adaptive waits for G3.

**Why.** A benchmark of a loop the product does not ship measures nothing the
product can use; a product that can overwrite a person's edit or certify a
weakened test is not safe to measure on real repositories.

## D-2026-09-30-3 — What is not built now

**Decision.** Not built until a dated decision reopens it with new evidence:

| Direction | Reason |
|---|---|
| Multi-agent execution | Adds context isolation, integration, conflicts and accounting before the single agent is proven |
| Automatic model routing (old C.23) | No reliable predictor of the best deployment per task exists |
| A richer 3D knowledge graph | Does not address completion, safety or context failures |
| Generalised semantic memory (old C.6) | Adds staleness and unverified facts |
| Critic/consensus with extra model calls | Buys cost and perceived safety without independence |
| A general browser or computer agent (old C.9) | Widens the surface and the semantics of effects drastically |
| A plugin or MCP marketplace | A new trust boundary and maintenance, with no demonstrated need |
| Windows/Linux parity now | See D-2026-09-30-4 |
| A new PDF/OCR stack | Outside the core; the current parser already needs specialist care |
| Optimising for the maximum context | Available memory is not quality or interactivity |
| A full certification system | A badge without broad evidence is not value; the Verified registry stays empty until evidence of its kind exists |
| Enterprise audit features | The journal and the basic contracts must be correct first |
| A learned small-decision classifier (old C.4), staged clean-context execution (old C.14), skill-pack library (old C.21) | Adaptive mechanisms without evidence; revisit after G3 |
| MoE expert-routing and speculative-decoding experiments (listed in the v0.3.0 plan as post-0.3 candidates) | Inference speed is not shown to be the binding constraint on the benchmark's question |

## D-2026-09-30-4 — Windows moves after the decision

**Decision.** The Windows engine, command isolation on Windows and a Windows
installer — the main item of the [v0.3.0-alpha plan](archive/v0.3.0-alpha-plan.md)
of 2026-09-28 — move to *Later*, after G3. macOS stays the only platform with
a sandbox and the only supported one.

**Why.** A second platform doubles the surface on which the effect boundary
(W1) and the one-path executor (W2) must hold, before either holds on the
first. A portable policy needs a stable core to port.

**Reversible by** the maintainer at any time with a dated entry; the plan's
W1–W3 items are platform-neutral in design and do not block a port.

## D-2026-09-30-5 — Documents describe the code, at a revision

**Decision.** The documentation was rewritten from the code at `0776ff4f`.
Current documents say what the code does and name the revision they were
checked against; plans live only in the implementation plan and the roadmap;
older documents moved, whole, to [archive/](archive/README.md). The rule in
CONTRIBUTING that superseded text keeps its body is kept, by the archive.

**Why.** Current documents contradicted the code and each other (verification
N7, 9.4, 16.2).

---

## Decisions from earlier revisions that still hold

Restated from the archive, with where they were first recorded.

| Decision | First recorded | Still holds because |
|---|---|---|
| Rust core in one Cargo workspace; no microservices | ADR-001 | Nothing measured argues otherwise |
| PWR runs models itself: MLX on Apple silicon; llama.cpp for GGUF; Ollama and LM Studio removed (2026-09-19) | archived roadmap | The engine is the most distinctive part (review §11) |
| The desktop app (Tauri 2 + Angular) is the only product front end; it is a client of `pwr serve` over ACP; not an editor plugin | archived `pwr-serve.md` (2026-09-16) | Unchanged |
| On a Mac the app runs MLX only (2026-09-24) | archived `models-and-context.md` | Unchanged |
| No shell interpretation: an executable and its arguments stay separate | CONTRIBUTING | Unchanged |
| A command that cannot be confined is refused unless `PWR_ALLOW_UNCONFINED=1` (2026-09-23) | SECURITY.md | Unchanged; Full access mode (2026-09-29) is the person's explicit, visible opt-out |
| A model may propose a memory; only the person saves it | archived `memory-and-wiki.md` | Unchanged |
| PWR never runs code from a model repository (`trust_remote_code` off) | archived `model-compatibility.md` | Unchanged |
| Compaction is mechanical; no model writes the summary | archived `models-and-context.md` | Unchanged; W4.1 adds that the objective is never shortened |
| Research evidence under `experiments/` and `.pwr/` stays off the public repository until a publishing policy exists (2026-09-22/23) | `.gitignore` | Holds for old campaigns; W8.5 publishes the confirmatory campaign |
| The ADR series is closed at ADR-012 | archived MASTER_SPEC | Durable decisions are entries in this file |
