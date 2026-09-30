# PWR: product and research contract

**Adopted 2026-09-30**, replacing the contract of 2026-09-12
([archived](docs/archive/MASTER_SPEC-2026-09-12.md)). It follows the
[technical review of 2026-09-30](docs/reviews/2026-09-30-technical-review.md),
whose claims were checked against the code before it was adopted
([verification](docs/reviews/2026-09-30-verification.md)). This contract
states what PWR is for, what it promises, and how a claim about it is allowed
to be made. It does not describe the code; the [documentation index](docs/README.md)
does, and says against which revision.

## What PWR is

> **PWR is a dependable local coding agent for Apple-silicon developers,
> optimised for bounded repository changes with inspectable effects and
> independent checks. Its harness removes mechanical work and execution errors
> from small and medium models, and keeps adaptive mechanisms only where
> controlled evaluations show a practical benefit.**

| | |
|---|---|
| **Core promise** | Local changes a person can control, with the outcome and the limits of their verification stated plainly. |
| **Target user** | A developer on an Apple-silicon Mac who values privacy and working offline, and accepts a bounded capability. |
| **Primary use** | Diagnosing and repairing bugs, small features and limited refactors in existing repositories. |
| **Technical differentiator** | An interactive local runtime (PWR's own MLX engine) plus a harness whose value is measured: fewer execution errors and lower cost for the same model. |
| **Success metric** | Tasks accepted by an independent check, per hour of use, reported with the human interventions they needed and the false acceptances they produced. |

**Non-goals.** General autonomy; replacing frontier agents; distributed or
multi-agent execution; a universal personal assistant; a chat skin over
someone else's runtime; a model trainer; a benchmark leaderboard.

## The thesis, and what has to be proven

PWR's bet is that **taking mechanical work off the model compensates for a
measurable part of its limits**. That is a hypothesis, not a result. The
evidence on record (R2, 2026-09-15) shows no uplift on one deployment and an
unconfirmed one on another ([evaluation.md](docs/evaluation.md)).

The one thing that must be proven:

> **On the runtime the app actually uses, a 9B/14B model with PWR beats a
> simple loop on new tasks, at equal budget, without more false acceptance or
> more human intervention.**

The protocol and the decision rule are in the
[implementation plan, W8.4](docs/plan/implementation-plan.md#w84-the-confirmatory-campaign).
If the answer is no, the adaptive complexity is removed and PWR stays a
dependable local agent without the compensation claim.

Two things are kept apart in every claim: **removing a defect of the
environment** (a template bug, a malformed call, a lost file hash) and
**raising what the model can do**. Both have value; only the second supports
the thesis.

## The minimum core

What stays even if every adaptive mechanism is falsified:

1. **One bounded agent session**, with a persistent objective, Stop and steering.
2. **A managed MLX engine**: correct templates, prompt cache, cancellation,
   the metrics a person needs.
3. **A few robust tools**: search, windowed read, precise edit, command, local service.
4. **Diffs and a verification contract**, with a baseline and protection of
   the person's own work.
5. **Evaluation of that same path**, with hidden acceptance and full costs.

Everything else is either infrastructure for these five or an experiment that
has to earn its place (see [feature-status.md](docs/feature-status.md)).

## Principles

1. **The model proposes; the harness keeps the facts.** File versions, tool
   outcomes and check results are recorded outside the model. A model's claim
   is never a verified fact.
2. **An effect is bounded the same way whichever path causes it.** A
   protection enforced for file tools and not for commands is not a
   protection; one that cannot be enforced is reported as not enforced.
3. **A write is based on what the model saw.** The harness never substitutes
   the current version of a file for the one the model read.
4. **One execution semantics.** The app, the command line and the evaluator
   run the same executor and mean the same thing by *complete*. Experimental
   controls are declared as such.
5. **The objective is not compressed.** A person's request and its revisions
   reach the model whole, or the turn stops and says why.
6. **Verification claims only what it checked.** Passing checks are evidence
   about what they check, under a contract whose artifacts are frozen. Missing
   checks, zero tests, unconfined runs and changed acceptance files are stated,
   never rounded up to "verified".
7. **Every path is bounded.** Actions, time, recoveries and model calls have
   limits that hold on every branch.
8. **Measure the product path.** An improvement measured in one loop and
   shipped in another has not been measured.
9. **Keep what earns its cost.** A mechanism that does not beat the simpler
   alternative under a controlled comparison is removed.
10. **Negative results are results.** Campaigns, including failed ones, are
    kept and cited with their conditions.

## Evidence vocabulary

Every claim in a current document carries one of these words, or `unknown`:

- **IMPLEMENTED** — a reachable path exists, cited by source; says nothing
  about model quality.
- **EXPERIMENTAL** — implemented, off by default or explicitly labelled,
  benefit not established.
- **PLANNED** — an item of the [implementation plan](docs/plan/implementation-plan.md).
- **HYPOTHESIS** — a falsifiable claim with its control, metric, threshold and
  rejection rule.
- **MEASURED** — a result with its conditions, counts and provenance (commit,
  deployment, corpus revision).

A document that cannot cite evidence says `unknown`. Changing a status cites
the evidence in the same edit.

## Authority

1. This contract — purpose, promise, principles.
2. [decisions.md](docs/decisions.md) — dated decisions, including what is not built.
3. [The implementation plan](docs/plan/implementation-plan.md) and
   [roadmap](docs/roadmap.md) — order of work.
4. The current technical documents indexed in [docs/README.md](docs/README.md)
   — what the code does, at a named revision.
5. [SECURITY.md](SECURITY.md) — the operational boundary.
6. Everything in [docs/archive/](docs/archive/README.md) and
   [docs/reviews/](docs/reviews/README.md) — evidence about earlier revisions,
   never a description of the current one.
