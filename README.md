# PWR

**A local coding agent for Apple-silicon Macs. It runs open-weight models on its own engine, works in your repository through sandboxed tools, and checks its changes with your repository's own tests — and tells you plainly what it did and did not verify.**

[![CI](https://github.com/VitoSanta/PWR/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/VitoSanta/PWR/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-2f718e.svg)](LICENSE)

PWR is an open-source desktop app and Rust core that turns a local language model into an agent working in a folder you choose. You talk to it; it reads, searches, edits files, runs commands and starts local services through typed tools. Every change arrives as a diff you can revert, every action is recorded in a local log, and after it edits, PWR runs the repository's checks and reports what they said.

It is built for **bounded changes in existing repositories** — diagnosing and fixing a bug, a small feature, a limited refactor — with small and medium models on your own machine, with no cloud API and no account.

> **Latest release: [v0.2.0-alpha](https://github.com/VitoSanta/PWR/releases/tag/v0.2.0-alpha)** (2026-09-28), a prerelease for macOS on Apple silicon. PWR runs a model's output as commands against your files: read [SECURITY.md](SECURITY.md) first, use it on repositories you can recover, and watch what it does. It is a supervised agent, not an autonomous one.

## What it does today

- **A conversation that does the work.** Answers and reasoning stream as they are written; actions appear as they happen; you can steer a turn or stop it. **Goal mode** keeps working until the goal's acceptance check passes, or it pauses and says why.
- **Your repository's own checks.** From `.pwr/checks.json`, CI configuration, `package.json` scripts or one of 19 build systems. They run after every turn that edits. A goal is *verified* only by an acceptance check you declared before the session began.
- **Sandboxed tools.** Commands run under macOS's sandbox, confined to the workspace, with no shell and your credential folders unreadable. Three permission modes: **Protected** (asks before network, dependencies, new programs, history rewrites and publishing), **Standard** (asks only before what cannot be undone or leaves the sandbox), and **Full access** (no sandbox at all — your own environment).
- **Changes you can inspect.** Per-file diffs, Revert through the core (it refuses a file you edited afterwards), a Terminal, a web preview, and a log of every action.
- **Models that fit your Mac.** The Model Manager searches Hugging Face for MLX models, rates each for this machine's memory, and downloads with checksums and resume. PWR computes the context window from your memory and the model. Unknown models are usable at once; a nine-request **Quick Calibration** checks that PWR can operate them.
- **Chat without a workspace**, with attached files, folders and images for models that see.

## What it does not do, yet

- **Prove that it makes a small model better.** The one paired comparison on record found no gain on one model and an unconfirmed one on another; a development run on the app's own path (2026-10-01) moved one model from 3/8 to 6/8 tasks, one trial each. The decisive test is planned: [evaluation](docs/evaluation.md), [plan W8](docs/plan/implementation-plan.md#w8--the-decisive-benchmark).
- **Solve hard tasks with a small model unattended.** Models of 9–30 billion parameters can loop on a difficult task, rewriting one file dozens of times, or lose the thread in a very long conversation; PWR notices and says so, and it is not yet measured whether its recoveries help.
- **Use two models at once.** One engine at a time on a Mac: a second one overruns the GPU's memory and both write nonsense. Switching model in a long conversation re-reads it from the start, which takes minutes.
- **Run anywhere but macOS on Apple silicon.** Windows is planned after the core is proven ([decision](docs/decisions.md)).

**Fixed on `develop`, not yet in a release** (the latest release, v0.2.0-alpha, still has these limits; CI has not run on them): commands could change files the edit tools refuse; an overwrite could replace an edit made after the model read the file; writes were not atomic; a goal could be reported verified after its tests were changed. See the [changelog](CHANGELOG.md) and the [plan](docs/plan/implementation-plan.md#w1--safe-predictable-effects).

Known limits are listed where the behaviour is described; start from the [documentation index](docs/README.md).

## Install

Requires a Mac with Apple silicon (M1 or later). A model needs free memory roughly the size of its download plus room for context; the Model Manager rates each variant for your machine.

1. Download [PWR-macOS-arm64.dmg](https://github.com/VitoSanta/PWR/releases/download/v0.2.0-alpha/PWR-macOS-arm64.dmg) and [SHA256SUMS.txt](https://github.com/VitoSanta/PWR/releases/download/v0.2.0-alpha/SHA256SUMS.txt) from the [release](https://github.com/VitoSanta/PWR/releases/tag/v0.2.0-alpha). Beside the DMG, run `shasum -a 256 -c SHA256SUMS.txt`, then open it and drag **PWR** to **Applications**.
2. The alpha is ad-hoc signed and not notarized. If macOS blocks the first open and you trust your copy, use **System Settings → Privacy & Security → Open Anyway** ([Apple's procedure](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac)). Do not disable Gatekeeper.
3. On first launch PWR installs its engine: a private Python environment with pinned MLX packages, about 1.2 GB, in `~/Library/Application Support/ai.pwr.desktop/engine`. No system Python or Xcode is needed.
4. Open the **Model Manager** from the model chip and download a model. Then open a folder, trust it, and ask.

### From source

Rust 1.88+, Node.js with npm, `python3` and `uv` (`brew install uv`):

```bash
git clone https://github.com/VitoSanta/PWR.git
```

```bash
cd PWR && sh scripts/setup-mlx.sh && cargo build --release
```

```bash
cd apps/desktop && npm install && npx tauri build --bundles app
```

`npx tauri dev` runs the app from the checkout; `target/release/pwr chat` opens a conversation in the terminal. See [building and distributing](docs/distribution.md) and the [command line](docs/cli.md).

## Where it is going

PWR's bet is that taking mechanical work off a small model — finding the file and line, keeping file versions, reading a check's output, recovering from a malformed call — makes it resolve more real tasks than a simple loop does, at the same cost. That bet is tested once, on the path the app actually runs, before anything more is built on it. The order of work is: **make every effect safe and honestly reported, run one execution path everywhere, then run the decisive comparison** ([roadmap](docs/roadmap.md), [contract](MASTER_SPEC.md)).

## Contributing

Issues and pull requests are welcome. Run what CI runs and judge each step by its exit code; a change carries a test that fails without it, and a change in behaviour updates the document that describes it. See [CONTRIBUTING.md](CONTRIBUTING.md).

Built and maintained by [Vito Santanelli](https://github.com/VitoSanta). Licensed under [Apache-2.0](LICENSE). PWR was called *poorAI* until September 2026.
