> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# CLI Specification

```text
PWR                                  # interactive workspace chat and agent control
pwr chat [--attach PATH]...          # same, with explicit external inputs
pwr doctor [--json]                  # discover host, Ollama and capability probes
pwr models inspect <model> [--probe] [--timeout-secs N] [--probe-trials N]
                                        # create/show ModelDefinition
pwr calibrate <model> [--ladder ...] # measured context/resource ladder
pwr repo index [PATH]                # build/update repository intelligence
pwr run <TASK> [--model TAG] [--profile CALIBRATION] [--dry-run]
                                        [--approve dependency-change,history-rewrite,publish,network-access]
                                        [--turn-timeout-secs N]
PWR verify [RUN_ID] [--scope targeted|full]
pwr eval run <SUITE> --model <TAG> --profile <CALIBRATION>
                                        [--seed N] [--temperature-milli N]
                                        [--turn-timeout-secs N] [--out-dir DIR]
pwr report <RUN_OR_EVAL_ID> [--format json|md|jsonl]
```

The interactive surface stores its model, calibration profile, context budget,
timeout and planning preference in `.pwr/chat-config.json` in the current
workspace. The startup menu and `/settings` use arrow keys and Enter; the
model selector reads Ollama's live model list, so newly installed local models
appear without an application update. The task composer accepts normal typing
and bracketed multiline paste. Press Enter once to submit the composed task.

Use `/attach` to switch the composer into attachment-path mode, or `/attach
PATH` to queue a file directly; Tab completes paths. Quoted paths and
shell-escaped spaces are accepted. `/attachments` shows the queue and
`/clear-attachments` clears it. Attachments are extracted where supported,
snapshotted under `.pwr/chat-attachments/` by content hash, and appended to
the next task only. A normal submitted task grants all available approvals and
provisioning permissions, while retaining the technical boundary that writes
are confined to the current workspace and every action is audited.

`models inspect --probe` executes the capability suite against the live deployment. `--timeout-secs` (default 300) bounds a cold load: a load that outruns it is recorded `unknown`, never as an absent capability. `--probe-trials` (default 3) repeats each sampled trial, because a single sample cannot distinguish "unsupported" from "did not happen this time".

`eval run` takes a frozen corpus file and writes a JSON and a Markdown report. `--seed` and `--temperature-milli` both reach the backend: a seed alone does not make sampling reproducible on every deployment, and both are recorded with the report so it says which kind of run it was.

An approval not granted in advance is asked for at the moment it is needed, when a terminal is attached. Where nothing can answer, the run refuses without asking rather than blocking or assuming consent.

A non-dry `run` requires `--model` — a deployment, or `auto` to let the selector choose one and explain the choice — and refuses a calibration that no longer matches the model digest, deployment fingerprint, hardware compatibility key or harness revision in force. `--profile` is required on a backend that serves a requested context window, because a ladder can be measured there. On a backend that does not accept one no ladder is possible at all, so the run proceeds on a conservative declared capacity, capped well below the window in force, with no measured tiers for context recovery to retreat to; the result reports `capacity_evidence: "declared, not measured"` at the top level rather than leaving it nested inside the execution profile. `--approve` grants effects that reach past the workspace; nothing is granted unless named, and a grant covers only what it names. `--turn-timeout-secs` (default 300) bounds one turn of the action loop, which carries more context each turn than the last.

Commands default to the current repository but require an explicit resolved root in emitted records. `--json` produces schema-versioned machine output. No command performs network access or dependency installation unless explicitly enabled.

A non-dry `run` and an `eval run` also require an active capability artifact for the deployment — one written by `models inspect --probe` whose model digest and deployment fingerprint match what is being addressed, and which observed `chat`, `streaming`, `structured_tools`, `edit`, `cancellation` and `context_boundary`. A tag that Ollama happens to serve is not evidence that the deployment can be driven.

`--backend` names the local inference backend to address: `ollama` (default) or `lmstudio`. It is named rather than inferred from the address, because two backends can answer on the same port with different wire formats, tool envelopes and lifecycle, and a mis-typed endpoint would otherwise be reported as a protocol fault instead of the wrong backend. `--endpoint` left unset follows the named backend's own address (`http://127.0.0.1:11434/` for Ollama, `http://127.0.0.1:1234/` for LM Studio), so selecting a backend never silently addresses the other one. The backend that answered is recorded on every deployment descriptor, so a measurement taken on one backend is not read back as evidence for the other.

`pwr` with no subcommand opens a conversation about the workspace, and the conversation is where the work happens: one loop, every capability available, no handoff to a separate run. A turn may answer, read, edit, run a command, or any sequence of those. There is no action budget: an action count is what bounds a run nobody is watching, and this one has an operator and a stop key. Esc ends a turn in flight, closing the connection the backend is generating into rather than waiting it out. When the conversation reaches three quarters of its context budget it is compacted — the instructions and the recent exchanges survive verbatim, and everything older becomes a record of what was asked and what was called, since tool output is the bulk of the tokens and has already been used by the turn that asked for it. The summary is carried in the user role, because a chat template with no user turn will not render. A turn that compacts twice without finishing is looping rather than working, and is stopped saying so. Every action goes through the same audited executor as a scripted `run`, under one conversation id, so `pwr report <id>` reads the whole thread. A turn that changed the workspace faces the repository's own checks: the baseline is captured before the first edit and compared after, and the verdict is appended to the answer rather than left for the operator to ask about.

Selecting a model in the console raises the context budget to what the backend reports that deployment serves, bounded by a declared profile's maximum where the workspace declares one. The capability probe is required before a model may be used and can be turned off; a model prepared without one is reported as `READY · UNPROVEN` wherever it is reported as ready, because "ready" and "ready, but nothing has shown it works" are different claims. Turning the requirement back on withdraws a readiness that was never justified, rather than leaving it standing under a rule it did not meet.

A model reference names one artifact. Where a backend serves several under one name -- LM Studio lists `qwen/qwen3.5-9b` for both an MLX 4-bit artifact and a GGUF Q4_K_M one, with different sizes and architectures -- the variant-qualified form (`qwen/qwen3.5-9b@4bit`) is the deployment, and the bare name is refused with both variants named. They are different deployments with their own fingerprints, so a calibration or certification of one is not evidence for the other; resolving the ambiguity by picking one would attach every measurement to whichever the backend listed first.

`models select` explains which deployment automatic selection would choose, and why, without loading or running anything: it reports the decision, the resource budget it was made against, every rejected candidate with its reason, and the artifacts eligible for this host that no backend currently serves. A capability nobody probed is unknown rather than absent, so a deployment is rejected for want of evidence rather than admitted on a declaration. `run --model auto` uses the same selector and additionally requires a calibration for the deployment it picks; an explicit `--model` is never substituted, though the selector may still reject it and say why.

`models certification MODEL` reports the certification level in force for the live scope, and `models certify MODEL --level ... --rationale ...` files a record. The scope is taken from live observation rather than from arguments, so a record cannot be filed against a backend, adapter or host other than the one measured. Records are appended, so a later regression demotes rather than erasing the evidence it contradicts; `compatible` and `certified` are refused without evaluation runs and raw artifact hashes; and a backend that publishes no version cannot be scoped at all.

`--allow-remote-endpoint` is required before `--endpoint` may name anything but this machine. A prompt carries repository excerpts with it, so choosing a non-local backend is a disclosure decision and is named rather than inferred from the address; redirects are refused, since one can change host after the address was judged. `--ollama-endpoint` remains a compatibility alias during the backend migration.

Any command that loads a model takes a host-wide runtime lease, so a second `run`, `calibrate`, `eval` or live probe is refused while the first holds it, naming the operation to wait for. The lease lives outside any repository: two workspaces on one machine contend for the same hardware.

## Where this specification is ahead of the implementation

Two claims here are not yet true, and are kept rather than quietly deleted because they are the intended surface.

**Exit codes.** The specification calls for 0 verified/success; 1 task or verification failed; 2 invalid input; 3 policy denied; 4 provider unavailable; 5 internal error. The implementation returns 0 on success and **4 for every error**, whatever its category. The category is already carried on each error and is what the mapping would use.

**`report --format`.** `json`, `md` and `jsonl` are accepted as of 2026-09-03. The JSONL form is the run's trail as one record per line, with a replay report and the chain verdict beside it.
