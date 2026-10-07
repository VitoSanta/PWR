# Reviews

Technical reviews and audits of PWR, and the checks made of
them. Historical records describe their dated revisions. Personal paths in
public copies are redacted; technical findings and results are retained.
What the project concluded from a review is
in the verification beside it and in [decisions.md](../decisions.md).

| Date | Document | Outcome |
|---|---|---|
| 2026-10-07 | [Metal interactivity failure](2026-10-07-metal-interactivity.md) | Driver workaround, smaller adaptive chunks and failure diagnostics; local long-context validation recorded separately from release gates |
| 2026-10-02 | [Core audit and correctness repairs](2026-10-02-core-audit.md) | C01–C14: effects, retries, parsing, memory, context preflight; deterministic regressions, owner manual test next; effectiveness unknown |
| 2026-09-30 | [Technical review](2026-09-30-technical-review.md) (Italian, as delivered) and its [verification](2026-09-30-verification.md) | All code claims confirmed, one corrected (`uv` is bundled in release builds); eleven further findings. Adopted: [MASTER_SPEC](../../MASTER_SPEC.md), the [implementation plan](../plan/implementation-plan.md), decisions D-2026-09-30-1 to -5 |
| 2026-09-25 | [Engineering audit](2026-09-25-engineering-audit.md) | Read-only audit of the shipped desktop path and model folders; its recommendations fed the v0.2.0 work (historical) |
| 2026-09-23 | [External review](2026-09-23-external-review.md) | Verdicts on a senior engineer's report; actions were backlog Part R (historical; open items carried into the plan) |
