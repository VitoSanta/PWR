# Contributing

## Building and testing

What CI runs (`.github/workflows/ci.yml`), in three jobs on macOS:

```bash
cargo fmt --all -- --check
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace --no-fail-fast
```

```bash
python3 scripts/milestones.py && git diff --exit-code docs/roadmap.md
```

```bash
cd apps/desktop && npm ci && npm test -- --watch=false && npm run build
```

```bash
PYTHONPATH=crates/pwr-mlx/sidecar "$PWR_MLX_PYTHON" -m unittest discover -s crates/pwr-mlx/sidecar
```

```bash
python3 -m unittest discover -s evidence/stack-matrix/runner
```

Judge each by its exit code, not by reading its output.

**The Rust suite is not hermetic.** Several tests exercise the macOS sandbox
with real toolchains and skip when the host lacks one (Docker with a running
daemon, .NET, a browser, a route to the network). A skip prints
`PWR-SKIP <test> <reason>` and, with `PWR_SKIP_LOG=<file>`, is appended to that
file; CI turns the lines into annotations. Read a local pass as "passed what
this machine could exercise", and check the log. Six tests are `#[ignore]`d
because they need the network or a live model;
[docs/testing.md](docs/testing.md) says how to run them. Nothing in CI runs a
model. A new host-dependent test skips through `common::skip`, never with a
bare `return`.

## What a change carries

**A test that fails without it** — preferably one that fails for the reason
the change exists. Several fixtures here once passed for reasons unrelated to
what they tested.

**Behaviour, not source.** A test that greps the source proves the code is
written, not that it runs.

**A comment saying why**, where the why is not obvious: what was measured,
what was tried, what a number is for.

**The document that describes the behaviour, updated in the same commit.**
Current documents describe the code at a named revision
([docs/README.md](docs/README.md)); a change that makes one wrong fixes it.

**A plan item.** Work is ordered by the
[implementation plan](docs/plan/implementation-plan.md). A change names the
item it advances; a new problem gets an item there, not a new backlog. When an
item is done, mark it DONE with the commit, and for a gate update
`docs/milestones.json` and regenerate the roadmap table.

## Commit messages

The subject says what changed. The body says what was wrong and what evidence
supports the change; where a measurement drove it, give the measurement;
where an earlier version was wrong, say so.

## Evidence and claims

- **A claim carries a status word** — IMPLEMENTED, EXPERIMENTAL, PLANNED,
  HYPOTHESIS, MEASURED — or says `unknown` ([MASTER_SPEC](MASTER_SPEC.md#evidence-vocabulary)).
- **Numbers come with their conditions**: counts, deployment, commit, corpus
  revision. A rate without its denominator is not a measurement.
- **A change that alters what a campaign measures** (prompt, catalogue, loop,
  budgets, adapters) gets an entry in the [experiment log](docs/experiment-log.md).
- **Removing an environment defect is not raising capability**; report which
  one a change is.
- **Durable decisions** are entries in [docs/decisions.md](docs/decisions.md).
  The ADR series stays closed at ADR-012.
- **Nothing is lost from the record.** A superseded document is removed by
  the commit that replaces it, whose message says what replaced it. The
  `docs/archive/` this repository kept until 2026-10-07 is read
  [at commit 309266d5](https://github.com/VitoSanta/PWR/tree/309266d5/docs/archive); a mention of `docs/archive/…` in a comment or
  an older document means that tree.

## Things this project is deliberate about

- **One execution path.** An improvement measured in one loop and shipped in
  another has not been measured. Do not add a loop; the plan (W2) removes the
  divergence that exists.
- **The harness does mechanical work; the model decides semantics.** Finding a
  file and line in a compiler's output is the harness's job; choosing the fix
  is not.
- **The harness never invents a fact for the model.** In particular it never
  substitutes the current version of a file for the one the model read.
- **A protection holds on every path or is reported as not held.**
- **A refusal carries what it knows**, so the model does not spend a turn
  rediscovering it — but never a value that lets it bypass the check it failed.
- **No shell interpretation.** An executable and its arguments stay separate.
- **A declared value nothing reads is a defect.**
- **Fixes help every model.** Per-model workarounds only through profiles and
  adapters.
- **Python is allowed** behind policy when a mature tool is better than a
  bespoke one; a new required runtime dependency needs a stated reason.
