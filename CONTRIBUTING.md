# Contributing

## Building and testing

```bash
cargo test --workspace
```

Two tests are ignored by default because they need the network. Everything else
is hermetic: no test requires a model, an inference engine, or a particular
machine -- including its memory size or the name of its Xcode.

Before sending anything, run what CI runs, and judge each step by its exit
code rather than by reading its output:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/milestones.py && git diff --exit-code docs/roadmap.md
```

CI runs exactly these on macOS and nothing else; when a test fails there, CI
names it in an annotation that needs no account to read. The desktop app is
not in CI yet (backlog R.8): `npx ng build` in `apps/desktop` at least. The
MLX sidecar's own tests run with the engine's interpreter:
`$POORAI_MLX_PYTHON -m unittest discover -s crates/pwr-mlx/sidecar`.
Anything needing a model or specific hardware is run by hand and its results
recorded in `docs/`.

## What a change should carry

**A test that fails without it.** Preferably one that fails for the reason the
change exists — several fixtures in this project passed for reasons unrelated
to what they were testing, and each is documented where it happened.

**Behaviour, not source.** A test that greps the source proves the code is
written, not that it runs. Two fixtures here did that, passed, and were
replaced by behavioural ones that failed immediately.

**A comment saying why, where the why is not obvious.** This codebase explains
decisions rather than restating code: what was measured, what was tried, what a
number is for. If a reviewer would ask "why this way?", answer it in the file.

## What the commit message is for

The subject says what changed. The body says what was wrong before and what
evidence supports the change. Where a measurement drove it, give the
measurement. Where a previous version was wrong, say so — the log is the record
of how the design was reasoned about, and a change with no stated reason is one
nobody can revisit.

## What the documentation must carry

The direction was redefined on 2026-09-12. [MASTER_SPEC.md](MASTER_SPEC.md) is the
contract; the [glossary](docs/glossary.md) fixes what the words mean; the
architecture, research, evaluation, audit, migration and roadmap documents own
the rest. Everything else in `docs/` is evidence about the revision that wrote
it, and carries a notice saying so.

**A claim carries a status word.** IMPLEMENTED, PROTOTYPED, PLANNED, RESEARCHING
and HYPOTHESIS are defined in the contract and mean different things. A change
that moves a claim up a level cites the source that justifies it in the same
edit. Where no evidence exists, the honest word is `unknown`, and it is allowed.

**Durable decisions amend the document that owns them.** The ADR series is
closed at ADR-012; do not add ADR-013. An experimental decision also needs a
dated entry in [experiment-log.md](docs/experiment-log.md) naming the hypothesis,
the conditions compared, the outcome — including an inconclusive one — and
whether the intervention is kept, revised or removed.

**Superseded text keeps its body.** Add a notice naming what replaced it. The
record of how the design was reasoned about is worth more than a tidy tree, and
a deleted wrong answer is a wrong answer somebody will reach again.

**Python is not banned.** The old prohibition on any external Python tool was a
revision-scoped rule, superseded by principle 9 of the contract: a mature
external parser or tool is allowed behind policy when it is better than a
bespoke one. What is still refused is a new required runtime dependency added
without a reason, and a tool whose effects escape the access policy.

## Things this project is deliberate about

- **A declared value that nothing reads is a defect.** Several existed and were
  removed; a fixture now fails when a declared value stops reaching the request,
  the policy or the decision it names.
- **The harness does mechanical work; the model decides semantics.** Finding a
  file and line in a compiler's output is the harness's job. Choosing which fix
  to make is not.
- **A refusal carries what it already knows.** A stale-hash refusal names the
  current hash; an ambiguous argument list names the list that should have been
  sent. A refusal that withholds what it has costs a turn to rediscover.
- **No shell interpretation.** An executable and its arguments stay separate,
  and a command line where a program name belongs is refused rather than split.
- **Numbers come with their provenance.** A rate without the counts and the
  interval is not a measurement.
- **A measured path and a product path that differ are two products.** An
  improvement measured in one loop and shipped in another has not been measured.
  This is the defect R0 and R1 exist to remove; do not add a third loop.
