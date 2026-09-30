# Glossary

The words the current documents use, in one sense each. The earlier glossary
is [archived](archive/glossary.md).

## Models and engines

| Term | Meaning |
|---|---|
| **Model** | Weights plus their tokenizer and chat template, as a folder (MLX) or a GGUF file |
| **Deployment** | One servable configuration: the model artifact (digest), quantization, engine and its version, adapter, window and runtime options. Two deployments of the same model name can behave differently |
| **Engine / backend** | What runs a model: PWR's MLX sidecar, or `llama-server` for GGUF |
| **Sidecar** | The Python process (`pwr_mlx.py`) that loads a model and generates, driven by the core over JSON lines |
| **Adapter** | The code in `pwr-compat` that reads a family's tool-call and reasoning conventions |
| **Profile** | Declared facts and sampling for an exact artifact (`strategies/models.json`) |
| **Status** | Verified, Locally calibrated, Provisional, Limited, Incompatible ([models.md](models.md#status-of-a-model)) |
| **Quick Calibration** | A nine-request compatibility smoke test; not a capability measure |
| **Reasoning Effort** | Low / Medium / High: the budget of a model's thinking phase per generation |
| **Window** | The context length a generation may use, computed from the model and the host |

## The agent

| Term | Meaning |
|---|---|
| **Turn** | One response of the agent to one message: generations and tool calls until it answers, completes, or is stopped |
| **Goal** | Goal mode: turns repeated until the work is verified, paused, blocked or out of budget |
| **Scripted run** | `pwr run` / `eval run`: the unattended research loop, a different loop from the turn |
| **Executor** | (planned, W2.3) the one component every entry point will call to run a turn or a goal |
| **Action** | A canonical tool call after normalisation (`ActionProposal`) |
| **Catalogue** | The tools offered to the model on a path |
| **Hold** | A `complete` the harness does not carry out, telling the model why (unseen results, nothing done, never ran) |
| **Detector** | A check that stops or redirects a turn (silence, unreadable calls, repetition, no progress…) |
| **Steering** | A message delivered into a running turn (*Send now*); it opens a new objective revision |
| **Compaction** | Replacing older history with a mechanical record when the prompt nears the threshold |
| **Objective** | What the person asked for, and its revisions. Today compaction may shorten it; W4.1 keeps it whole |

## Effects and permissions

| Term | Meaning |
|---|---|
| **Workspace** | The folder a conversation works in |
| **Policy** | The rules a tool call is checked against: paths, programs, approvals, protected paths |
| **Approval / grant** | A permission kind (`network_access`, `dependency_change`, …) the person gives once, for the session, or by mode |
| **Permission mode** | Protected (`ask`), Standard (`auto`), Full access (`full`) |
| **Sandbox** | The macOS Seatbelt profile a command runs under; absent in Full access |
| **Confined / unconfined** | Whether a command ran under the sandbox |
| **Protected path** | A path in `.pwr/protected.json`: readable, not changeable by the file tools (commands: not yet, W1.3) |
| **Known version** | The hash of a file as the conversation last saw it — read, created or edited (`Continuity::known`); a whole-file rewrite is checked against it |

## Verification

| Term | Meaning |
|---|---|
| **Check** | A command that verifies something about the repository (build, test, lint) |
| **Acceptance check** | A check declared `"kind": "acceptance"` in `.pwr/checks.json`: the owner's evidence that the requested behaviour works |
| **Acceptance contract** | The acceptance checks and — after W3.1 — the files they run, frozen at session start |
| **Baseline** | The checks' results before the agent edits, to tell regressions from pre-existing failures |
| **Verified** | Acceptance reached under an unchanged contract. Passing checks without it is *checks passed*, not verified |
| **Unavailable** | No check exists or none could run; stated, never rounded up |
| **Fingerprint** | (planned, W1.5) the identity of a failure: failing test names, or a digest of the normalised output |
| **Hidden verifier** | In evaluation, a check the agent never sees, used to score |
| **False acceptance** | The agent or PWR said done or verified, and the hidden verifier failed |

## State

| Term | Meaning |
|---|---|
| **Event log** | `.pwr/state.sqlite`: every event, hash-chained — a verifiable log, not a tamper-proof one |
| **Checkpoint** | A conversation's record of changed files and the objective's revision |
| **Snapshot** | A conversation's whole message list, written each turn |
| **Projection** | State derived from the workspace or the log (index, wiki, graph, summaries), deletable and rebuilt |

## Evaluation

| Term | Meaning |
|---|---|
| **Arm** | B0 (conventional loop), B1 (PWR), B2 (staged workflow) |
| **Simple loop** | (planned, W8.3) the baseline of the decisive campaign |
| **Campaign** | A set of trials under one manifest |
| **Intervention / nudge** | Any message to the agent after the brief; a success after one is not unattended |
| **Split** | `dev` tasks PWR is improved against; `heldout` tasks only measured |
| **Uplift** | PWR's success minus the baseline's, paired, per deployment, in percentage points |

## Status words

IMPLEMENTED, EXPERIMENTAL, PLANNED, HYPOTHESIS, MEASURED, `unknown` — defined in
[MASTER_SPEC](../MASTER_SPEC.md#evidence-vocabulary).

PWR was called *poorAI* until September 2026.
