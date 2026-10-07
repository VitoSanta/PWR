# PWR documentation

Rewritten on 2026-09-30 from the code at `develop` `0776ff4f`, after the
[technical review](reviews/2026-09-30-technical-review.md) was checked claim by
claim ([verification](reviews/2026-09-30-verification.md)). Each current
document names the revision it describes. Everything written before is kept,
whole, in the [archive](https://github.com/VitoSanta/PWR/blob/309266d5/docs/archive/README.md).

## Start here

The [product overview](product-overview.md) summarises the current development
features and their limits. It distinguishes published releases from unreleased
code. The [next-release checklist](release/next-release-readiness.md) records
the evidence still needed before a new tag or release.

| Document | For |
|---|---|
| [README](../README.md) | What PWR is, install, status |
| [Product overview](product-overview.md) | Current features, intended use and unreleased changes |
| [MASTER_SPEC](../MASTER_SPEC.md) | The product and research contract: promise, thesis, principles, evidence words |
| [Implementation plan](plan/implementation-plan.md) | Every work item, with acceptance criteria and tests, and the gates |
| [Roadmap](roadmap.md) | The plan in NOW / NEXT / LATER / NOT NOW, with milestone status |
| [Executor parity](plan/executor-parity.md) | What the scripted loop does that the app's turn does not, and the proposed disposition of each |
| [Decisions](decisions.md) | Dated decisions, including what is not built |

## How it works (current behaviour)

| Document | Covers |
|---|---|
| [Architecture](architecture.md) | Components, the production path, the three execution semantics |
| [Agent loop](agent-loop.md) | The turn, Goal mode, the scripted run, every limit |
| [Context](context.md) | Prompt composition, counting, compaction, retrieval, wiki and memory |
| [Tools and sandbox](tools-and-sandbox.md) | The catalogue, execution, edits, permission modes, the Seatbelt profile and its limits |
| [Verification](verification.md) | Check discovery, post-turn checks, Goal acceptance, recovery |
| [Inference](inference.md) | The provider boundary, the MLX engine, llama.cpp, the working window |
| [Models](models.md) | Adapters, statuses, Quick Calibration, Reasoning Effort, the Model Manager |
| [State and persistence](state-and-persistence.md) | Every stored file, what kind of state it is, the event log |
| [Desktop app](desktop.md) | The product surface and what the plan changes in it |
| [`pwr serve`](pwr-serve.md) | The protocol between the app and the core |
| [Command line](cli.md) | Commands and environment variables |
| [Security](../SECURITY.md) | The boundary, for users |

## Quality and evidence

| Document | Covers |
|---|---|
| [Evaluation](evaluation.md) | The measuring machinery, what has been measured, rules for a claim |
| [Initial mission audit](reviews/2026-10-01-audit.md) | Reproduced local checks, verified findings and unexamined F1 coverage |
| [Competitor sources](research/competitors.md) | Partial F2 source/revision/license survey; no matched capability comparison |
| [Experiment log](experiment-log.md) | Dated experiments and measurement-changing changes |
| [Semantic Decision Layer](research/semantic-decision-layer.md) | Unmeasured, model-agnostic hypothesis: candidate generation, semantic selection and deterministic verification |
| [Testing](testing.md) | Suites, CI, what "passed" means, manual passes |
| [Distribution](distribution.md) | Building, releasing, reproducibility gaps |
| [Feature status](feature-status.md) | Every feature, its state and its plan item |
| [Risks](risks.md) | The principal risks and their mitigations |
| [Glossary](glossary.md) | The words, one sense each |
| [Release records](release/) | Release notes, readiness audits, Mac verification passes |
| [Next release](release/next-release-readiness.md) | Candidate checks, packaging and unresolved release work |
| [Reviews](reviews/README.md) | External reviews and audits, and their verification |
| [thresholds.md](thresholds.md) / `thresholds.json` | The evaluation bars the code reads (historical reasoning, still operative) |
| [Product screenshots](product-screenshots/README.md) | The v0.1.0 screenshot set |

## Rules for these documents

1. **A document describes the code at a named revision.** Plans live only in
   the implementation plan and the roadmap. A change that alters described
   behaviour updates the document in the same commit.
2. **Claims carry a status word** (IMPLEMENTED, EXPERIMENTAL, PLANNED,
   HYPOTHESIS, MEASURED) or say `unknown`. Numbers carry their conditions.
3. **Known defects are written where the behaviour is described**, with the
   plan item that fixes them.
4. **Nothing is lost.** A superseded document is removed with the commit that
   replaces it, and that commit's message says what replaced it; the text
   stays in the history. The documents written before 2026-09-30 were kept
   in `docs/archive/` until 2026-10-07 and are read
   [at the last commit that held them](https://github.com/VitoSanta/PWR/tree/309266d5/docs/archive).
5. **Code comments cite backlog IDs** (`C.22`, `D.E2E-21`, `R.4`…): they refer
   to the [archived backlog](https://github.com/VitoSanta/PWR/blob/309266d5/docs/archive/backlog.md); where the item is still open,
   the plan's [carry-over table](plan/implementation-plan.md#old-backlog-every-open-item)
   says where it went.
6. The milestone table in the roadmap is generated from `milestones.json` by
   `scripts/milestones.py`; CI fails if they differ.
7. **Public documentation is for users and contributors.** Personal editorial
   plans, session handoffs, local restore instructions and machine-specific
   release working notes belong outside the checkout. Keep public technical
   findings, negative results and reproducible conditions; remove personal
   paths and private profile data. Unpublished evidence is identified as such,
   rather than linked to a maintainer's filesystem.
8. Historical reviews and release records describe their dated revisions;
   they are not current readiness evidence. Run
   `python3 scripts/check-public-docs.py` before a documentation change is
   proposed for release.
