# PWR

**A local coding agent for Apple-silicon Macs. It runs open-weight models on its own engine, works inside your repository through sandboxed tools, and checks its work with your repository's own tests.**

[![CI](https://github.com/VitoSanta/PWR/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/VitoSanta/PWR/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-2f718e.svg)](LICENSE)

PWR is an open-source desktop app and Rust core that turns a local language model into an agent working in a folder you choose. You talk to it; it reads, searches, edits files, runs commands and starts local services through typed tools. Every change arrives as a diff, and every action is recorded in a local, hash-chained log. After it edits, PWR runs the repository's own checks and tells you what they said.

It is also a research project. The question behind it is **same model + same task + different harness: what improves, and at what cost?** PWR does not claim that local models equal frontier models, or that it already competes with cloud coding agents.

> **Public alpha (0.1.1-alpha).** PWR is usable today and changes quickly; it is open source and developed in the open. It runs a model's output as commands against your files: read [SECURITY.md](SECURITY.md) first, and do not point it at anything you cannot afford to lose.

## What it does today

- **A conversation that does the work.** Replies and reasoning stream as they are written. Actions appear as they happen and can be steered mid-turn or stopped. **Goal mode** keeps working until the goal is verified.
- **Your repository's own checks.** PWR finds them itself: `.pwr/checks.json`, CI configuration, `package.json` scripts, or one of 19 build systems such as Cargo, Go, Maven, Gradle, .NET, Swift, Python and CMake. It runs them before and after it edits. Goal mode is verified only by an acceptance check you declared before the session began.
- **Sandboxed tools.** Commands run under macOS's sandbox, confined to the workspace, with no shell. Credentials folders are never readable. In **Ask** mode (the default), PWR asks before it changes dependencies, reaches the network, installs toolchains, rewrites Git history or publishes. **Auto** mode grants those without asking.
- **Changes and evidence you can inspect.** The Changes panel shows per-file diffs and lets you **Revert** a file or all of them. Revert goes through the core: a file you edited afterwards is never overwritten, and every revert is logged. The Evidence panel shows Verify, Report, Diagnose and Doctor, and the Core log panel shows the core's output.
- **Models that fit your Mac.** The **Model Manager** searches Hugging Face for MLX models and rates each one for this machine's memory. Downloads are checksum-verified and resumable, into `~/.pwr/models`. PWR computes the context window from your memory and the model's configuration, and names the limit that set it.
- **Unknown models welcome.** A model PWR has never seen is **Provisional** and usable at once. A nine-request **Quick Calibration** marks it *Locally calibrated* or *Limited*. **Reasoning Effort** (Low / Medium / High) bounds how long a model may think, enforced in the engine.
- **Context under control.** A context indicator shows what fills the window. Compaction runs automatically or on demand, and it is mechanical: no model writes the summary, and changed files, open errors and the last check result are kept.
- **Chat without a workspace**, with attached files, folders and images for models that can see.

Inference runs on your Mac through PWR's own MLX engine, with no cloud API and no account. The network is used when you browse or download models from Hugging Face, once to install the engine, and by the agent only when you allow it.

## Install

Requires a Mac with Apple silicon (M1 or later). A model needs free memory roughly the size of its download plus room for context; the Model Manager rates each variant for your machine.

### The app (DMG)

1. Download [PWR-macOS-arm64.dmg](https://github.com/VitoSanta/PWR/releases/download/v0.1.1-alpha/PWR-macOS-arm64.dmg) and [SHA256SUMS.txt](https://github.com/VitoSanta/PWR/releases/download/v0.1.1-alpha/SHA256SUMS.txt) from the [v0.1.1-alpha release](https://github.com/VitoSanta/PWR/releases/tag/v0.1.1-alpha). Run `shasum -a 256 -c SHA256SUMS.txt` beside the DMG, then open it and drag **PWR** to **Applications**.
2. This alpha is ad-hoc signed and not notarized by Apple. On first open, macOS may block it. If you trust the copy you downloaded, use **System Settings → Privacy & Security → Open Anyway** after trying to open PWR, then confirm. This is [Apple's documented procedure](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac). Do not disable Gatekeeper globally.
3. On first launch, PWR installs its engine: a private Python with the pinned MLX packages, about 1.2 GB, in `~/Library/Application Support/ai.pwr.desktop/engine`. No system Python or Xcode tools are needed.
4. Open the **Model Manager** from the model chip and download a model. Then open a folder, trust it, and ask.

### From source

Requires Rust 1.88+, Node.js with npm, `python3`, and `uv` (`brew install uv`) for building the app bundle.

```bash
git clone https://github.com/VitoSanta/PWR.git
cd PWR
sh scripts/setup-mlx.sh                  # the engine's Python environment (.venv-mlx) and the local encoder
cargo build --release                    # the core, target/release/pwr
cd apps/desktop && npm install
npx tauri build --bundles app            # src-tauri/target/release/bundle/macos/PWR.app
```

To reproduce the public Apple-silicon DMG and checksum locally, run
`sh scripts/release-macos.sh`. It cleans the Rust build outputs, installs the
locked npm dependencies, builds and verifies the DMG, and writes the final
files to `dist/release/`. It does not publish anything. See the
[alpha release notes](docs/release/v0.1.1-alpha-release-notes.md) for the
release procedure and download links.

`npx tauri dev` runs the app from the checkout. From a terminal, `target/release/pwr chat` opens a conversation in the current directory and `pwr run "<task>"` runs a task unattended; `--help` on any command lists its flags, and [docs/current-cli.md](docs/current-cli.md) is the command-line guide.

Models live in `~/.pwr/models/<publisher>/<name>`; set `PWR_MLX_MODELS` to use another folder. Report bugs and request features in [GitHub Issues](https://github.com/VitoSanta/PWR/issues).

## Status

What exists, as of **2026-09-24**. The evidence for each line is in [docs/PWR_PRODUCT_SOURCE_OF_TRUTH.md](docs/PWR_PRODUCT_SOURCE_OF_TRUTH.md), the [roadmap](docs/roadmap.md) and the [backlog](docs/backlog.md). Live runs are single diagnostic runs on one machine, not benchmarks.

| Status | What exists |
|---|---|
| **Implemented** | The desktop app (Tauri 2 + Angular) as a client of `pwr serve --stdio` (Agent Client Protocol). PWR's MLX engine: templates, streaming, reasoning budgets, a prompt cache across a turn. The conversation loop with goal mode, steering, approvals, stall and loop detection. Typed, audited tools for files, search (including installed dependencies, read-only), edits, commands, services and Git. Check discovery and baseline comparison. Model Manager, fit ratings, verified downloads. Provisional models, Quick Calibration, Reasoning Effort. Context panel and compaction. Chat mode. Hash-chained event log with `pwr report` and `pwr diagnose`. Ask/Auto permission modes. |
| **Experimental** | Images for models with a vision encoder (loaded once through `mlx-vlm`). A semantic context filter for documentation (`PWR_SEMANTIC_RETRIEVAL=1`). Plan-first and toolchain-provisioning runs in the command line. The llama.cpp/GGUF engine, which is command-line only and off in the app; it is meant for Windows. |
| **Next** | Notarized builds. One runtime shared by the conversation and scripted runs. A measured study of the harness on 7–14B models against a fixed tool-loop baseline. Windows, with its own sandbox. |

Known limits of this alpha: macOS on Apple silicon only; the sandbox exists only on macOS; no model carries PWR's *Verified* status yet; context composition is estimated at four characters per token; the conversation and the scripted runner still differ in planning, compaction and recovery.

## Read the design

| Document | What it is |
|---|---|
| [PWR_PRODUCT_SOURCE_OF_TRUTH.md](docs/PWR_PRODUCT_SOURCE_OF_TRUTH.md) | What PWR is and does, feature by feature, with evidence and the claims not to make |
| [MASTER_SPEC.md](MASTER_SPEC.md) | Project definition, design principles, scope and evidence vocabulary |
| [SECURITY.md](SECURITY.md) | The permission model and where the boundary ends |
| [Glossary](docs/glossary.md) | What each term means |
| [Research](docs/local-agent-research.md) | Prior art, hypotheses and falsification decisions |
| [Architecture](docs/architecture.md) | Candidate runtime, context, tools and policy |
| [Evaluation](docs/evaluation.md) | Controls, measurements and promotion gates |
| [Roadmap](docs/roadmap.md) · [Backlog](docs/backlog.md) | Order of work, and everything not yet built |

Dated implementation notes, ADRs and [experiment logs](docs/experiment-log.md) remain as evidence about the revisions that produced them. PWR was called *poorAI* until September 2026.

## Contributing

Issues and pull requests are welcome. Run what CI runs (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and the milestone check) and judge each by its exit code. A change carries a test that fails without it. See [CONTRIBUTING.md](CONTRIBUTING.md).

Built and maintained by [Vito Santanelli](https://github.com/VitoSanta). Licensed under [Apache-2.0](LICENSE).
