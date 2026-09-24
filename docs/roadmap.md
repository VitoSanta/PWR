# Research roadmap

**Update — 2026-09-24: first public alpha preparation.** Work resumed on 2026-09-18 and the pause below ended with it. PWR v0.1.0-alpha is prepared for a public alpha release for Apple-silicon Macs; the GitHub release has not yet been published. The desktop app bundles the core, the MLX engine's scripts and an installer for the engine's Python; Revert goes through the core (`_pwr/revert`); the webview has a Content Security Policy; models live in `~/.pwr/models`; the app runs MLX only on a Mac, with llama.cpp kept for Windows and bound to loopback. The order of work after the release is the 2026-09-23 update's, below. The current description of the product is [PWR_PRODUCT_SOURCE_OF_TRUTH.md](PWR_PRODUCT_SOURCE_OF_TRUTH.md).

**PAUSED, 2026-09-17 (ended 2026-09-18).** PWR is paused while the maintainer follows a study plan. Where it stands: [the redesign](redesign-2026-09-17.md) is proposed with its first decisions taken (six evaluation areas, the host's largest window for end-to-end tests, MLX first, a hardware catalogue of models) and six decisions open; R3 is shelved; the MLX engine spike is done (`experiments/engine-spike-mlx-20260917/analysis.md`) and found LM Studio's MLX path as fast as in-process MLX. **First step on resuming is the outside reference, not a build** — see "Where we are and what is left" immediately below, which is the current statement of order and supersedes the sequencing in the milestone sections.

**Old regime, 2026-09-17.** Every campaign recorded below up to `experiments/r3-h2-dev-20260917` ran under the old regime (*sotto vecchia gestione*): one calibrated 16,384-token window for every question, a calibration probe before any run, a single blended completion rate, and models served through Ollama or LM Studio. Their defect fixes and mechanism observations stand; their completion rates are not capability claims and their sample sizes are not carried forward. The proposed replacement -- evaluation sectioned by harness area, models loaded without a probe, an embedded local engine -- is [the redesign](redesign-2026-09-17.md), under review.

**Authoritative sequence, updated 2026-09-16.** R0 and R1 are **COMPLETED** against their engineering exits; R2 is **COMPLETED as a measurement** — 204 trials, no uplift established, the choice rule names `unfinished`, and a quarter of the sampled tasks flip outcome on the seed alone, which is why a confirmation needs about a hundred paired tasks rather than thirty; R3 is **SHELVED** since 2026-09-17 (it read as PREPARED and next until then); R4–R6 remain **PLANNED**. These entries describe implementation readiness, not model-quality results. The old M0–M6 table and dated campaign notes are preserved below as historical evidence. Their labels do not establish readiness for the new project.

Implementation follows an experimentally justified boundary, not a calendar. [Research hypotheses](local-agent-research.md) provide rejection decisions; [evaluation](evaluation.md) specifies pairing, resource accounting and uncertainty. Numeric gates below are proposed practical criteria to freeze before confirmation, not measurements of current performance.

## Update — 2026-09-23: context filter, dependencies, review, permissions

A day and a half of work with the maintainer after the product loop, each item
recorded in [`backlog.md`](backlog.md) with its evidence. What was decided and
what is now true:

**Decisions of the maintainer**
- **No external cloud services.** TypeSafe's Jev (the "System One" model
  behind C.22's idea) is hosted, so it is a reference for the *shape* -- typed
  decisions with calibrated confidence, deterministic code deciding -- not a
  dependency. By the same rule, a PII anonymiser such as rizzo-pii is not
  needed: nothing leaves the machine.
- **Two permission modes**, Ask (default) and Auto (backlog R.2).
- **Research evidence off the public repository for now** (R.9); which of it
  to publish, anonymised, is to be decided.
- **Small models are the target of the context work**: the value of a filter
  is the window it frees for a model that has little.
- The general-agent direction -- MCP, apps, the web -- stays on the intended
  path (R6, C.12), one extension at a time and after a measured need; the
  offline half of C.12 (installed dependencies) came first.

**Built and measured**
| Item | State |
|---|---|
| Unattended end to end | `pwr run` on an empty crate: verified by the workspace's own tests in 5 minutes, no steering (D.E2E-28). Inside another repository the same task *declined* correctly: the sandbox denied the parent workspace. |
| Documents by section | Retrieval ranks Markdown sections (BM25, heading weight): precision 0.03 -> 0.10 on twelve labelled documentation requests (C.22). |
| Local context filter (C.22) | `multilingual-e5-small` on MLX in its own offline sidecar, fused with the section BM25; opt-in `PWR_SEMANTIC_RETRIEVAL=1`. Sections-only P@3 0.19 -> 0.24, 11% fewer tokens. A 1.5B "System One" decider added nothing on top of an encoder. Qwen3-14B at its own 40K window, docs FAQ, n=1 per arm: both verified, the semantic arm with fewer tokens and actions and no whole-document reads. |
| Installed dependencies | `search` with `in_dependencies` (node_modules, site-packages, the cargo registry by Cargo.lock): used unprompted at a run's fourth action (C.12). Dependencies are read-only without `DependencyChange` -- a run had passed its test by editing the library (D.E2E-29). |
| Protection and verification | `pwr run` now honours `protected.json`, and an unreadable one stops the work (D.E2E-30). A suite is judged by exit codes; CI's four steps are run before a change is called green (R.1). |
| External review | Checked claim by claim ([`external-review-2026-09-23.md`](external-review-2026-09-23.md)); its research thesis and paired-uplift metric adopted (R.10). |
| One engine for text and images (C.25) | A model with a vision encoder is loaded **once** through mlx-vlm and driven by the same loop as text -- template, reasoning budget, prompt cache, checkpoints. Qwen3.6-35B-A3B: text identical token for token to the mlx-lm load, same speed, +0.9 GB; an image attached in the app reaches the model through `pwr serve` and is read correctly; follow-up turns reuse the cached image prefix. |

**Ideas recorded, not built**: the rest of the per-model registry learned
from the audit (C.24: form repairs are built, the per-model suffix is not);
the rendered page as a harness screenshot for the models that see (C.25 step
3, D.E2E-12); a single-agent adaptive harness (C.23).

**Order of work from here**
1. Public CI green (push of R.1), sandbox refusal outside macOS (R.3), README
   (R.5), licence inventory (R.6), the app in CI (R.8).
2. **The fixed tool-loop baseline** (R.10) -- the control every small-model
   claim needs -- then a context-filter task the lexical arm fails at a small
   window, with repeats (C.22).
3. llama.cpp server kept alive across turns (R.4), before any Windows or GGUF
   performance claim.
4. The fallback registry, measured on the A1 replay (C.24); then the rendered
   page for the models that see (C.25 step 3).
5. One runtime contract for chat and scripted runs (R.11).

## Update — 2026-09-22: the product loop, end to end

A day of the maintainer driving real tasks through the app, recorded item by
item in [`backlog.md`](backlog.md) (D.E2E-1 to D.E2E-27). What changed in the
state below:

| Track | 2026-09-22 |
|---|---|
| S2 toolkit | **Decided: Tauri 2 + Angular (signals)**, `apps/desktop/`, installed as `/Applications/PWR.app`; Slint kept until parity. Streaming chat, grouped actions, diffs, queue, context meter, permissions. Not yet: Windows, signing, packaging the core inside the app, replaying past actions on resume. |
| Harness in product use | Website task **passed** through the app (Qwen3.6-35B-A3B, contract verified 4/4); a 14-file game with 37 tests **passed** after steering. Structural fixes: no-shell commands (`cwd`, refusals of `cd`/`&&`/`echo "cmd"`/servers), chat decoding through the shared repairs, stall detection across check-ins, read-only reference folders, loose edit matching, engine-silence and orphan-sidecar guards. |
| Model configuration | **The largest finding:** model profiles never loaded outside the checkout (D.E2E-24), so every workspace ran greedy with reasoning on. Fixed; runs before it should be read as unconfigured. |
| Measurement | Unchanged: no capability claim. These were product runs, n=1 each, several steered by a person. |
| Repository | From this date `main` carries the product only: `experiments/` (campaign notes, test workspaces, the MLX engine venv) stays on the maintainer's machine and is no longer tracked; older links into it from these documents point at history. |
| Context filter (C.22) | **Built, opt-in** (`PWR_SEMANTIC_RETRIEVAL=1`): local `multilingual-e5-small` on MLX in its own sidecar, fused with the section BM25. Twelve labelled requests: sections-only P@3 0.19 -> 0.24, 11% fewer tokens. First small-model run (Qwen3-14B, 40K window, docs FAQ, n=1 per arm): both verified, the semantic arm with fewer tokens and actions and no whole-document reads. Jev (TypeSafe) identified as hosted, so a reference only: the maintainer's rule is **no external cloud services**. |
| External review (2026-09-23) | Checked claim by claim ([`external-review-2026-09-23.md`](external-review-2026-09-23.md)): confirmed that the conversation pre-grants dependency changes and network access, that commands run unconfined off macOS, that llama.cpp restarts its server every turn, that the README is stale and the app untested. Found while checking: public CI red since the 2026-09-23 push (fixed locally). Adopted: the small-model research thesis and a paired-uplift metric against a fixed tool-loop baseline. Actions in backlog Part R. |
| Open, next | **First the review's safety and hygiene items** -- push the CI fix (R.1), conversation defaults that ask (R.2), visible sandbox state (R.3), README (R.5), `.gitignore` (R.7). Then the fixed tool-loop baseline (R.10) so the small-model work has its control; then a context-filter task the lexical arm fails at a small window, with repeats (C.22); a per-model registry with fallbacks for errors of form, measured on the A1 replay (C.24); images for the models that can see, starting with the rendered page (C.25, D.E2E-12); read `turn.failed` in the next stall (D.E2E-22); measure the outline-first read (D.E2E-15). |

## Where we are and what is left — 2026-09-17

This section is the current, ordered statement of state. The milestone sections
below (R0–R6, S1–S3) keep their own history and their exits; where their
sequencing disagrees with this, this wins. For the *breadth* rather than the
order — every item not yet built, including the whole product surface, with
its dependencies — see [`backlog.md`](backlog.md), whose Part 0 also sets out
which kinds of measurement a harness change invalidates and which it does not,
what public data already settles without a campaign, and where the
"self-writing harness" idea lands. Everything here dates from the day
R3's development run was stopped and [the redesign](redesign-2026-09-17.md) was
written.

### The one sentence

The harness is built and its correctness debt is largely paid; **what is not
established is anything about capability, because every campaign so far measured
a starved run** — and the starvation came from a calibration number that the
engine spike then showed to be wrong by a factor of six.

### The product thesis it serves

Stated 2026-09-17, and it is the redesign's Part D said as a promise rather than
as a requirement: **PWR runs well on your machine without you knowing how to
make it run well** — it reads the host and the model, chooses a configuration
that fits, and says which limit set it. Not *the maximum the machine can run*
(`MASTER_SPEC.md`'s third principle already refuses that: largest context is a
candidate policy, not a requirement) and not *faster inference* (the spike
measured LM Studio's MLX path prefilling faster than ours). The gain is not
running a bad configuration — which for us was the difference between 2 of 35
tasks resolved and whatever the same harness does unstarved. The argument, the
two words it must not be sold with, and the measurement that would prove it are
in [`backlog.md`](backlog.md), Part 0d. `MASTER_SPEC.md` is untouched pending a
decision on whether the thesis belongs in the contract.

### Where we are

| Track | State | Reading |
|---|---|---|
| R0 accounting | **CLOSED** | Strict pairing, manifests, reconciliation, immutable per-trial artifacts. Holds under any regime. |
| R1 one session contract | **CLOSED** apart from compaction unification | `session::gate`/`perform` is the one step both loops take; 31 `two_loops` fixtures. Compaction stays two algorithms until R3. |
| R2 failure regimes | **CLOSED as an old-regime measurement** | `unfinished` is the largest testable class. Its completion rates are not capability claims. |
| R3 (H2, evidence through compaction) | **SHELVED** | Stopped at 35 of 79 trials. Splits into A5 (mechanism, small window) and A6 (completion, realistic window). |
| R4 adaptation, R5 product gate, R6 extensions | **PLANNED**, untouched | All downstream of a measurement regime that can produce a capability number. |
| S1 protocol (`pwr serve`) | **DONE** | ACP over stdio, schema-validated, golden transcript. |
| S2 toolkit spike, S3 the desktop app | **NOT STARTED** | S2 was waiting on R3; with R3 shelved it now waits on the re-measurement below. S3 also needs Windows execution isolation. |
| Evaluation regime | **REDESIGNED, not implemented** | Six areas A1–A6, each with its own window and metrics. No suite exists yet. |
| Model loading | **Probe still a gate** | `pwr calibrate` is required before `eval` or chat. Part B replaces it; nothing is implemented. |
| Engine | **MLX engine running** | PWR renders the template and controls reasoning itself; on it, PWR matches Bionic on A.1's five tasks (4 of 5, 2026-09-18). Long prefill is slow; llama.cpp and removing the HTTP backends are step 8. |
| Code hygiene | **CLEAN** | Warning-free build, no dead-code markers; the 2026-09-17 pass found four unused items and four unused dependencies and nothing else. |

### What is actually blocking everything

`HISTORY_BUDGET_SHARE = 0.5` at a 16,384-token window leaves the agent roughly
5,000 tokens of history. 35 of 35 development trials compacted at least twice,
one compacted 30 times in 40 actions, 15 died on the stall guard, 2 resolved.
That window was inherited from a calibration recording 7 tokens/s at 16k; the
spike measured 44 and 65 on the same machine and fitted 262k tokens in 64 GB.
Until this is undone and re-measured, no intervention — context policy, engine,
adapter, retrieval — can be credited or dismissed, because every measurement of
it is taken under starvation. See the redesign, Part E.

### The order of work, and why it is this order

1. **The outside reference. DONE 2026-09-18: 4 of 5 in Bionic at 262,144
   tokens against 0 of 5 for PWR at 16,384, same model, same tasks — the gap
   is ours** (`experiments/a1-bionic-reference-20260918/results.md`). Five
   A6-shaped tasks by hand in Bionic, same model, same machine. *Half a day.* It is first because it is the only step
   that de-risks all the others: it says whether the gap is PWR's
   configuration or the model's ceiling, and the answer changes what steps 2–5
   are worth.
2. **Loading without a probe** (redesign Part B) with the computed window
   (Part D). **DONE 2026-09-18 in code, not yet run live**: every command
   computes the window from the model's config and this host's memory when no
   calibration is named, and records every ceiling and the binding one. Open
   decision 1 did not block it after all -- both answers drop the probe as a
   gate -- and now decides only how much of the calibration code F.3 removes.
   Still open inside it: the latency ceiling from observed turns, and the
   prefill-transient constant (backlog B.2).
3. **Re-measure at a realistic window** (Part E): the same tasks, the host's
   window, with `HISTORY_BUDGET_SHARE`, the action budget and the stall guard
   revisited. A **diagnostic**, not a campaign — small n, no preregistered gate,
   no capability claim — asked only whether the starvation hypothesis holds.
   This is the measurement that tells us whether the rest of the plan is the
   right plan. It also yields the first, cheap version of the product thesis'
   own measurement (backlog A.15) for free, since running the old inherited
   16,384 against the computed window *is* a naive configuration against an
   automatic one. **Stopped after three tasks, 2026-09-18** (1 of 3 resolved;
   `experiments/part-e-diagnostic-20260918/analysis.md`): nothing compacted
   at the computed window, and the time went to runaway reasoning instead.
   Backlog A.17 then found why, by controlled tests: sampling is not the
   cause; reasoning is, and through the endpoint PWR uses it cannot be
   switched off. With it off, Qwen3.6 was faster and right on the same
   prompt. **To be re-run after step 4**, with the tool fixes of `6c0cec2`.
4. **The minimal embedded MLX engine -- moved up from step 7 on 2026-09-18,
   and a first version DONE the same day** (`--backend mlx`; capability probe
   on Qwen3.6 in 148 s with edits 3 of 3, against over ten minutes and 1 of 3
   through LM Studio; `experiments/mlx-engine-20260918`),
   by the maintainer, on A.17's evidence. Only a caller that renders the chat
   template controls reasoning -- off, or a real budget that closes the think
   block after N tokens and lets the answer follow -- and without that control
   every measurement of PWR is confounded by runaway turns. Scope: the
   `Engine` boundary, MLX generation with PWR rendering the template,
   reasoning off and a reasoning budget, tool calls parsed by the existing
   family adapters, the computed window. *Interim until it lands:* `/no_think`
   for the Qwen family through the LM Studio path. Not in scope yet: llama.cpp,
   HuggingFace downloads, removing the HTTP backends (step 8).
5. **Re-run the Part E diagnostic -- done 2026-09-18: 4 of 5, all declared,
   the same four tasks Bionic resolved** (`experiments/part-e-orient64-mlx-20260918`,
   binary `9fa4dab`, 64 actions). On the fifth, idna, Bionic declared success
   over two rewritten tests; PWR claimed nothing. Six runs to get there,
   each named by the last one's traces: reasoning control (own engine) →
   files JSON-escaped and a prefill crash (fixed) → orientation cost (check
   commands shown, enclosing definitions, restore_file, destructive
   whole-file replacements refused) → the action budget (64 for this model).
   Full path in the run's `analysis.md`. Open from it: the shared default of
   26 (C.2, decided from A6), and the engine's slow long prefill.
6. **A1–A4 suites -- DONE 2026-09-19 for Qwen3.6-35B-A3B** (`suites/`,
   `pwr eval suite`, `sh suites/run.sh a2|a3|a4`; A1 runs in `cargo test`).
   A1 by replay: 47 cases, seconds; closing its gaps raised the decoded rate on
   794 real replies from 97.4% to 98.2%. A2 navigation: the corpora's nine
   repository questions plus one built from Part E's forty-action search, 10 of
   10 (that one now in 5 turns). A3 editing: 6 tasks, 6 of 6. A4 verification:
   3 shortcut tasks, 3 of 3, no false completion. Building and running them
   found and fixed five harness faults: CRLF files, the shrink guard missing
   small files, numbers for text fields, the engine's loop guard cutting code,
   and nine decoder gaps. **Carried into step 7:** A3/A4 cannot yet tell two
   harness revisions apart on this model (the binary with the escaping bug
   passes A3 too) -- step 7's smaller models are the discriminating runs --
   and A1's live half (valid-call rate on short scripted tasks).
7. **Catalogue measurements**: small, medium and large models on this Mac
   (redesign Part D). *Open decision 3 -- how a 16 GB class is tested -- blocks the
   small tier only.* **MLX catalogue pass closed 2026-09-19** (`experiments/catalogue-20260919/notes.md`):
   Qwen3.6-35B-A3B 9/9 and fast, the practical model here; Qwen3.8-27B 9/9 but
   4-7x slower (dense); Seed-OSS-36B excluded (62 tokens/s prefill, prose instead
   of calls with reasoning off); gpt-oss-20b being re-run after its adapter
   fixes. Building it fixed the engine's window arithmetic, streaming,
   cancellation and two adapters. *Later the same day:* gpt-oss 8/9 after its
   adapter and hint fixes; Nemotron 3.5 Lightning 30B-A3B 8/9 and fast, the
   second choice; GLM-4.7-Flash excluded (loops copying hashes, never
   declares). Table in the notes. This is enough to choose the default and the
   fallback for this host and move to the engine work in step 8. Deferred from
   the catalogue rather than blocking that move: A2 per promising model, the
   small tier (open decision 3), and any quantisation pair at equal memory
   (open decision 6). **Criteria agreed for further models:** decoder-only
   Instruct/Coder with native tool calls, preferably MoE with few active
   parameters, weights within ~35-45 GB, checked on paper before any download;
   the maintainer names the candidates.
8. **The rest of the engine**: llama.cpp for GGUF (and Windows), HuggingFace
   weights, **then Ollama and LM Studio removed** (open decision 4). *The
   removal came first, 2026-09-19, at the maintainer's request:* both crates
   and the endpoint flags are gone, the engine is the only backend and the
   default, and `last-with-http-backends` tags the last revision with them.
   *Started 2026-09-20:* `pwr-llama` is a GGUF/llama.cpp backend reachable
   as `--backend llama`; it discovers `.gguf` files, reads their metadata and
   feeds `ModelFacts` so the same computed-window path applies to GGUF. The
   next increments fixed the planned `llama-server` launch arguments with a
   concrete loopback endpoint, the OpenAI-compatible chat-completions request
   body, an offline parser for OpenAI-style stream events, and the HTTP path
   that turns an SSE response into PWR's `ModelStream` against a local
   fixture. `chat` now starts a managed server on demand with readiness polling
   and kill-on-drop, and `backend_version` records the server's `--version`
   output. Provider cancellation now stops an in-flight SSE stream against a
   live fixture. The real smoke was unblocked 2026-09-20 by installing
   Homebrew's `llama.cpp` 0.4.1 (`llama-server` build 10964) and pointing the
   ignored smoke test at an existing local Qwen3 0.6B Q4_K_M GGUF; it passed.
   This makes llama.cpp usable as an engine path, but not yet a measured model
   path: the smoke model is a tiny base model. The first follow-up, output
   hygiene for GGUF inspect artifacts, is done: large tokenizer arrays are
   summarized in routine inspection evidence while full metadata still feeds
   digest and window computation. Until a proper GGUF candidate is chosen and
   measured, MLX remains the live measured path on Apple silicon. B.8 has
   started with deterministic HuggingFace download plans and a guarded
   downloader: the registry can render pinned Hub URLs and local destinations,
   declare per-file byte counts and blake3 or SHA-256 hashes, and execute only
   artifacts that can be verified, with `.part` resume; that execution path is
   tested against a local HTTP fixture for fresh download, already-present and
   resumed-part cases. The registry now has two real, pinned GGUF candidates
   with byte counts and SHA-256 from HuggingFace resolve headers: Qwen3.6
   35B-A3B Q4_K_M and Nemotron 3.5 Lightning 30B-A3B Q4_0. Nemotron's 18.9 GB
   GGUF has been downloaded, SHA-256 verified, inspected and probed through
   `--backend llama`, and measured with `llama-server`: A4 3/3, A3 6/6, no
   false completions, 0.9-2.9 minutes per task
   (`.pwr/suite-runs/a4-42cc39b-llama-ggml-org_NVIDIA-Nemotron-3.5-Lightning-30B-20260920-155604`,
   `.pwr/suite-runs/a3-42cc39b-llama-ggml-org_NVIDIA-Nemotron-3.5-Lightning-30B-20260920-155936`).
   `suites/run.sh` now accepts `PWR_SUITE_BACKEND`, so that measurement
   uses the same suite wrapper instead of a one-off command. The run found one
   harness gate bug: `llama-server` enforces a controlled context window but
   does not report prompt token counts, so `context_boundary` can be
   unmeasurable while `context_window_control` is observed; the admission gate
   now accepts that exact case. The CLI also has
   a free-space preflight with a 5 GiB margin before download, while
   licence-gated access and product-level disk UX remain open. The evidence
   gathered through the removed backends -- experiments, the Bionic reference,
   the profiles in `strategies/models.json` -- stays as it was. Also the A6
   suite at the host's window, read against step 1, carrying the full form of
   backlog A.15 -- the product thesis stated as a number.
   *CLI repair, 2026-09-20:* the terminal console remains the development and
   research control surface until the frontend lab exists. `chat --model` now
   saves an explicit engine/model choice and opens a supervised chat on its
   computed window without making a capability probe a gate; the probe remains
   an optional catalogue diagnostic. The current manual GGUF instructions are
   in [`current-cli.md`](current-cli.md). Scripted `run` and `eval` still carry
   the old evidence admission and are documented as an explicit follow-up,
   rather than silently presented as the manual product path.
   *B.9 done, 2026-09-20:* when llama.cpp receives the agent's tool catalogue,
   PWR now requires a native tool choice and the server constrains generation
   to that catalogue's tool grammar. An offline request-body test and a live
   Nemotron smoke both hold: asking for an unavailable write with only a read
   tool offered yields one well-formed read call, never an invented tool name.
   The constraint guarantees call syntax, while the harness retains semantic,
   policy and filesystem checks.
9. **R3 resumed**, split into R3-A5 and R3-A6. After B.9, start a narrow
   frontend lab before the polished product: one local app that can select an
   installed model, launch a run, show live actions/logs, stop it, and surface
   artifacts. **B10 started 2026-09-20:** `_pwr/models` now safely selects a
   discovered model for the active workspace. **D.6 status increment:** it also
   exposes declared artifact final/partial byte state to the Slint lab without a
   network request or expensive hash. **2026-09-21:** the Slint bridge now starts
   the core in the selected workspace, keeping registry and workspace-local
   configuration resolution stable when the app is launched elsewhere.
   **2026-09-21:** `_pwr/download` accepts only `cwd` plus a client operation
   id, emits byte progress, and `_pwr/download_cancel` preserves the `.part`
   file; the Slint lab exposes Download / resume. A later app refresh recovers
   the state from disk. Slint now exposes per-artifact resume and a sequential
   Download all queue.
   The conversation also queues a workspace file as a read-only `resource_link`
   for the next prompt; drag/drop and citation rendering remain.
   **Frontend integration checkpoint 2026-09-21:** the planned pre-manual
   surfaces are wired in Slint. The next activity is the consolidated manual
   pass; further work should be driven by findings from that pass.
   **Launcher follow-up 2026-09-21:** MLX remains the preferred macOS engine;
   the Slint app now defaults to MLX on macOS and keeps explicit `llama` for
   GGUF testing. The repository launcher opens the app in the invocation
   directory and preserves direct terminal access through `pwr --cli`.
   **Frontend startup fix 2026-09-21:** the launcher now routes the app's
   development-default `PWR ... serve --stdio` invocation to the core
   binary, preventing recursive app windows and the resulting false
   "Waiting for the local core" state.
   **Frontend UX iteration 2026-09-21:** the Slint lab now has a task-first
   two-level layout: workspace and assistant selection stay visible, while
   context, downloads and permissions move behind Advanced settings; activity,
   review and permissions live in a contextual inspector. The core connection
   state is no longer overwritten by catalog status. This is the first UX
   pass, not the final visual validation; manual desktop review must still
   refine responsive sizing, empty/error states and motion.
   **Desktop window fix 2026-09-21:** the app now uses preferred rather than
   fixed dimensions, exposes an explicit Full screen action, and the global
   launcher resolves the workspace with `pwd -P` at invocation time. The
   previously displayed root workspace came from an older app process started
   from the PWR repository; this path is now deterministic for fresh runs.
   **First-task flow fix 2026-09-21:** selecting a model immediately updates
   the visible assistant state. Sending a non-empty task now creates the first
   session automatically, so the composer is the primary path rather than a
   disabled control behind a separate session-start action.
   **Assistant picker refinement 2026-09-21:** the installed-model list is now
   a native dropdown with an explicit unselected state, reducing sidebar
   density while preserving the full selected artifact name as supporting
   detail.
   **Interaction styling increment 2026-09-21:** primary task actions now use
   a custom accent treatment with hover, press and color transitions; advanced
   settings expose their open state through a checked control. Motion remains
   intentionally state-led and will expand with live action and streaming
   views rather than as decorative animation.
   **Protocol/catalog fix 2026-09-21:** `serve --stdio` no longer writes
   console context-status lines to its JSON-RPC stdout while refreshing a
   workspace. The assistant picker can therefore render the discovered catalog
   reliably; its refresh action is now an option inside the native dropdown.
   **Layout coherence increment 2026-09-21:** assistant artifacts are rendered
   with concise human-readable labels while the bridge retains their canonical
   identifiers. The desktop layout uses stable desktop columns and consistent
   40px action controls, with elision or word wrapping at the text boundary.
   It deliberately avoids width-driven visibility bindings: those caused Slint
   layout feedback loops during resize. A later narrow-window design can add a
   deliberate compact navigation state rather than silently hiding activity.
   **Conversation visibility fix 2026-09-21:** the Slint bridge now renders
   the user's submitted prompt and ACP `agent_message_chunk` updates in the
   central transcript. A turn has an explicit working state until its response
   completes; it is no longer possible for a successful response to appear as
   only the opaque activity message `Turn completed.`.
   **Conversation feedback increment 2026-09-21:** the composer now grows from
   a compact starting height to a capped editor with internal scrolling. While
   a turn is active, a transient status describes the real operational phase
   (thinking, chosen tool, running tool, or reviewing output), then clears when
   the durable agent message arrives. The current core collects each model reply
   before it acts, so this is explicitly progress feedback rather than fake
   token streaming; true token streaming needs a provider-to-ACP chunk relay.
   **Desktop E2E evidence 2026-09-21:** an isolated JavaScript fixture was
   launched through the desktop entry point with the workspace path visible in
   the app, then exercised against Nemotron through the same `serve --stdio`
   core. It read three files, made the requested implementation/test changes,
   ran `npm test`, reread the result and completed Goal mode after 9 actions;
   full verification discovered `npm test --silent` and passed 1/1. This is
   evidence for launch, model selection, tools and goal verification on a small
   task, not a substitute for the planned Angular usability, full-screen,
   resize and long-task acceptance pass.
   **Desktop observability finding 2026-09-21:** app conversations persist in
   `<workspace>/.pwr/state.sqlite` and are correctly listed by the ACP
   `session/list` endpoint (the Angular workspace contained six recorded
   sessions). The legacy CLI `session list` does not yet expose those ACP
   conversation ids, and the desktop interface has no exportable diagnostic
   bundle. Treat this as an R5 frontend requirement: a user must be able to
   copy/export one session's transcript, tool events, verification evidence,
   environment facts and redacted error details for support analysis. Do the
   visual-system redesign before adding that surface, so the export and session
   history belong to the final information architecture rather than another
   temporary panel.
   **Goal-verification correction 2026-09-21:** a real Angular manual task
   produced green `npm run build` and `npm test -- --watch=false` evidence while
   the browser still rendered the generated Angular welcome screen. This is a
   false acceptance: compilation and repository tests establish technical
   health, not that a requested user-facing, API, CLI, desktop or migration
   outcome exists. Goal mode now requires an executed, repository-declared
   `.pwr/checks.json` check with `"kind": "acceptance"` before it may say
   **Goal verified**. The same contract applies across stacks; browser/e2e,
   API contract, workflow, smoke and invariant commands are all valid evidence.
   The contract is snapshotted at session start and must remain unchanged, so a
   model cannot create or weaken the evidence that certifies its own work.
   Without one it reports technical checks passed but leaves the goal
   unverified. R5 still needs held-out semantic acceptance and visual review:
   this prevents the misleading claim; it cannot fabricate a test the project
   has not defined.
   **Angular entrypoint finding 2026-09-21:** the same manual task was stopped
   correctly as unverified, then inspected rather than repaired by hand. The
   app bootstraps standalone `App` from `src/main.ts`, whose `app.html` still
   contains the generated Angular page and a router outlet. The model instead
   concentrated changes in a second `app.component.*` tree and lazy pages;
   it also left a self-redirecting root route and the original unit assertion
   for `Hello, pwr-app`. Build passed because these are valid compiled lazy
   chunks, and unit tests passed because they tested the untouched active root.
   Trace evidence shows `app.component.html` was retrieved nine times versus
   `main.ts` once, with a roughly 62k-token prompt; pre-existing dirty model
   output therefore reinforced a lexical but wrong architecture. This is a
   product-path retrieval/topology finding, not evidence that an Angular skill
   packet alone will solve it. R5 needs an entrypoint map, dirty-tree provenance
   in retrieval and browser-level acceptance before claiming web task success.
   **Entrypoint preflight increment 2026-09-21:** the shared context composer
   now emits a deterministic `workspace_topology` section for Angular
   standalone workspaces, naming bootstrap, active root component/template and
   router configuration before lexical passages. A compact
   `framework_guidance` section reinforces reachable-component, router-import,
   generated-test and browser-check requirements. These are recorded prompt
   sections, not hidden model knowledge or evidence of completion. The remaining
   dirty-tree provenance and browser acceptance work stays explicit; C.21 must
   evaluate the packet against a no-packet control rather than declare an uplift.
   **MLX launcher fix 2026-09-21:** the desktop launcher now exports the
   repository's measured MLX Python when no `PWR_MLX_PYTHON` was supplied.
   The generic macOS `python3` lacks `mlx-lm` on this host; without this
   fallback the catalog could be discovered but every model turn failed at the
   sidecar boundary. An explicitly configured interpreter remains authoritative.
   **Cross-workspace launcher fix 2026-09-21:** the global `pwr` launcher
   now passes PWR's own `Cargo.toml` when it needs to rebuild. Previously a
   stale build attempted Cargo discovery in the user's current workspace and
   failed for non-Rust projects before the app could open. The current directory
   remains the app workspace; only compilation is anchored to PWR itself.
   **Turn outcome and layout containment 2026-09-21:** a terminal action-budget
   outcome now appears in the transcript as a check-in rather than the false
   label "Turn completed." The compose area has a stable height so the
   conversation owns available full-screen space. The activity rail records a
   concise changed-file summary instead of embedding whole diffs, which had
   forced long source text through the desktop layout and made it feel slow.
   A full change-review view remains a required R5 surface.
   **Manual-task preparation 2026-09-21:** the action checkpoint is now a
   visible Continue / Stop here decision in the desktop conversation, so long
   real-workspace tasks preserve their session and can carry on deliberately
   rather than appearing to complete silently. The composer is compact like a
   chat input and uses native file/folder pickers. A selected folder becomes a
   bounded read-only text snapshot (32 files, no `.git`, `.pwr`, dependency
   or build directories), not a live external filesystem grant. This is ready
   for the consolidated manual pass; drag/drop, citation rendering, a full
   diff review and narrow-window navigation remain R5 work.
   **Goal-mode experiment 2026-09-21:** the desktop composer can now opt a
   substantial task into server-owned continuation. The normal 26-action turn
   checkpoint becomes an internal handoff, not a completion decision: the
   model must use structured `complete`, then PWR runs the workspace's full
   declared checks. A failed build/test/lint result is returned as evidence for
   another iteration; prose alone cannot close the goal. Stop remains live and
   a 208-action aggregate guard pauses for review without claiming success.
   This is deliberately a goal-completion experiment, not a claim that generic
   static analysis can prove every product requirement: route coverage and
   dead-code evidence require checks the repository actually declares or that
   the model adds and runs. It needs a real long-workspace manual measurement
   before becoming the normal default.
   **D.9 increment:** the Slint lab can now set the workspace approval policy
   to all supported kinds or none through `_pwr/approvals`.
   **D.8 increment:** it also lists workspace sessions and resumes one through
   `session/list` and `session/resume`; branch drift and interrupted-run detail
   remain to be surfaced.
   **D.11 increment:** the action area can request Changes, Verify, Report and
   Diagnose from the active session and show the core's evidence text.
   **D.12 increment:** the header distinguishes core connection, backend
   unavailability, cancellation requests and request errors; empty and recovery
   states remain to be polished.
   **D.7 increment:** `_pwr/models` now applies a requested context window
   through the selected backend and returns the granted value/options, binding
   ceiling, memory budget and rationale; Slint exposes controls and displays
   those details. A richer calibration history remains.
   **D.6 packaging increment:** the pinned artifact registry is embedded as a
   distribution fallback, with a workspace-local registry taking precedence.
   Automated tests remain part of each implementation increment, but the full
   manual pass is deliberately deferred until the frontend integration is
   complete. That avoids retesting a moving surface after every protocol
   change; the manual checkpoint will cover the whole product flow in one
   coherent pass.
10. **S2, then S3.** S2 can start once step 5 has told us whether a context
   policy is going to survive to be shown in a UI.

Fine-tuning and LoRA appear nowhere in this list on purpose; open decision 5
says when they may.

### The six open decisions

Restated from the redesign so this section stands alone. Nothing above step 2
can start without the first of them.

1. **Needle recall**: keep it once per (model, quantisation, engine, window) as a
   catalogue measurement, or drop it with the probe? *Blocks step 2.*
2. **How the Rust core reaches MLX**: Python sidecar over `mlx-lm`, or the C API
   through bindings? *Blocks step 7 only.*
3. **How the 16 GB class is tested**: a 16 GB machine, or an engine-enforced
   memory cap on this one? *Blocks the small tier of step 5.*
4. **Build the embedded engine now**, together with fetching weights from
   HuggingFace, or keep LM Studio's MLX path until constrained decoding or
   self-containment is needed? **Direction decided 2026-09-18 by the
   maintainer: yes -- MLX and GGUF embedded, and once they work well Ollama
   and LM Studio are removed.** Step 7 therefore exists, and ends with the
   HTTP backends deleted rather than kept optional. Its place in the order
   does not move: it follows the measurement steps, because the spike showed
   no speed gain to win and the gain it buys is self-containment for the app.
   The window computation, `ModelFacts` and the config and GGUF readers built
   for step 2 are the parts of it that already exist.
5. **Fine-tuning or a LoRA adapter — when, and for what?** Not for tool calls
   (one malformed call in 1,027 turns; a grammar gives a guarantee an adapter
   cannot), not from our own traces (2 of 35 resolved would distil a weak
   policy). Possibly for behaviour a grammar cannot impose, which is A2 and A4.
   *Revisit after step 4, never before step 3.* *2026-09-19, reaffirmed with
   the maintainer:* the models chosen are already tool-use trained (Instruct
   with native calls); suite A1 shows 98% of real replies decode, and what
   was lost was the harness's. A LoRA gets a target only if a promising model
   fails A1 systematically.
6. **Quantisation strategy**: a larger model at 4-bit, or a smaller one at
   8-bit, at equal memory? Published work says 8-bit is near-lossless while
   4-bit loses most on exactly the long-context agentic behaviour PWR
   depends on; every plan so far has silently assumed the larger model at
   4-bit. *Decided cheaply inside step 4, not by a campaign; see
   [`backlog.md`](backlog.md) Part 0b.*

### What must not be done again

- Running another campaign at an inherited window and reading its completion
  rate as capability.
- Choosing an operating point from a calibration number nobody re-measured.
- Building a subsystem — engine, adapter, retrieval — before step 3 says what
  the harness does when it is not starved.

## Readiness against the product definition

This table ties the high-level definition — a conversation-driven local LLM
harness that acts on code and the authorized machine around it — to the
milestones below. It is deliberately stricter than "does code for this exist":
a claim is product-ready only when the conversation can drive it, the same
runtime path can be evaluated, and the evidence says what was actually checked.

| Claim area | Current status | What blocks the claim |
|---|---|---|
| PWR as a local agent harness research platform | **Ready to claim as direction and alpha scope.** The workspace has local providers, tools, policy, repository context, verification, event storage and evaluator infrastructure. | Keep the wording experimental; no model-quality or cloud-parity claim follows from this implementation state. |
| One agentic coding session driven from conversation | **Shared boundary in place (R1).** Chat, scripted run and product-path evaluation call one step for every action (`session::gate`, `session::perform`) and share the result envelope, reply-fault handling, context-tier rule, stall and repetition detection and terminal classes; their remaining differences are declared and pinned in `two_loops`. | The two loops are still two drivers; unifying compaction waits for R3, whose control arm is today's compaction. R5 must show the product path holds up on real tasks. |
| Harness uplift over simpler fixed policies | **Not established, and now measured rather than assumed.** The 2026-09-16 rerun ran all three arms on both deployments over thirty paired tasks: GLM 11/11/10 (B0/B1/B2, every comparison p = 1.00) and Qwen 13/18/12, with PWR leading the conventional control 8 tasks to 3 on the discordant pairs (p = 0.23) and the staged control 9 to 3 (p = 0.15, or 0.065 excluding trials a harness defect touched). The direction is positive on the larger deployment only, and nothing is significant at thirty tasks. | A confirmation needs about 100 paired tasks per deployment at the observed discordance, which is a campaign of its own. R3 first tests whether H2 moves `unfinished`, the class the choice rule named. |
| Context, memory and compaction that make local models better | **Implemented but not yet proven as useful-context policy.** There is lexical/shallow-symbol retrieval, typed prompt sections, session ledgers and compaction; semantic/procedural memory and full artifact rehydration remain experimental. | R2 must identify the failure regime; R3/R4 must show the context or memory intervention beats fixed baselines at equal cost before it becomes a durable default. |
| Product reliability on real engineering work | **Not ready.** Conversation checkpoints, resume and steering exist ahead of the product evaluation. | R5 must run the held-out product gate with interruption, continuation, dirty worktrees, actual check discovery and independent acceptance. Until then the product remains experimental. |
| The PWR desktop app as the only front end | **Frontend lab implemented; product not ready.** The Slint desktop prototype is now connected to `pwr serve --stdio`: it can select a discovered model, manage downloads/resume, create and resume sessions, send prompts and attachments, cancel, request Changes/Verify/Report/Diagnose, show actions/diffs/evidence, change approvals and expose context details. The terminal console remains the development and research surface until the manual pass validates the whole flow. | The product-surface track still needs usability and recovery polish, drag/drop and citation rendering, branch-drift/interrupted-run detail, packaging and cross-platform validation. R5 must make the app sufficient for the capabilities admitted so far; R6 capabilities must be drivable from it on entry. |
| Authorized machine, browser, network, MCP and desktop reach | **Planned, not implemented as a complete path.** The destination explicitly includes broader authorized access, but current complete capability is coding-workspace oriented. | R6 admits extensions one at a time after R5, with scoped policy, observable effects and a benchmark showing benefit without degrading the coding regression set. |
| Cross-platform execution isolation | **Partial.** macOS has the meaningful confinement path; Linux and Windows probes exist but not equivalent process isolation. | Either R6 or a security-focused prerequisite must add and test OS-specific isolation before broad host-access claims include those platforms. |

## Work that can close without a live model

The following items do not require Ollama, LM Studio or a live LLM. They can be
closed with fake providers, retained traces, local fixtures, repository
workspaces and CLI/store tests. Closing them improves readiness, but it must
not be described as model capability or harness uplift.

| Area | Offline work that can be closed | Milestone effect |
|---|---|---|
| R1 shared-session extraction | Done, 2026-09-15: one step boundary for actions, evidence, recovery and terminal outcomes, with 31 `two_loops` fixtures. Compaction unification is deferred until after R3. | R1 closed. |
| R1 equivalence matrix | Add or tighten fake-provider fixtures for red baseline, no verifier, denied edit, cancellation, compaction, user redirection, backend fault and context-limit recovery. | Can mark individual R1 exit cases covered. It does not prove the model will use those paths well. |
| R2 accounting and trace classification | Classify retained pilot traces, review every row marked by `scripts/r2_classify_failures.py`, and improve deterministic classifiers where source evidence is enough. The reviewer now reads event traces in the protocol's precedence and links each manifest to its own report; the full 2026-09-14 pilot has `failure-classes.{json,md}` and `analysis.md`. | Can shrink the unknown bucket and prepare the choice rule. It cannot complete R2: the full pilot's comparison is confounded by harness defects D1–D4 and needs a rerun on a fixed binary. |
| R2 controls | Fixture B0/B1/B2 event parity, transcript bounds, completion semantics and scoring inputs with scripted providers. The offline B0/B2 baseline suite passes (`cargo test -p pwr-orchestrator --test baselines`, 8 tests, including both controls preparing their context window), and strict pairing passes (`cargo test -p pwr-eval --test strict_pairing`, 20 tests). | Can prove the controls are comparable mechanisms. It does not measure which arm wins. |
| Context engine and repository intelligence | Test section budgeting, eviction, ledger preservation, stale index invalidation, retrieval freshness and artifact rehydration with local repositories. | Can close correctness debt in the context pipeline. It does not prove useful-context uplift; that remains R3/R4. |
| Store, artifacts and resume invariants | Add schema/migration checks, artifact-retention tests, checkpoint reconciliation and exact action intent/receipt fixtures. | Can harden the persistence/resume contract needed by R5 without running a model. |
| Scripted-loop continuity | Port the conversation checkpoint, receipt and reconciliation invariants onto the scripted loop after or during R1 extraction. | Can close the R5 prerequisite that both product and evaluator share continuation semantics. It is not the held-out product gate. |
| Product UX surface | Make every admitted capability reachable through `pwr serve`, so the desktop app -- the only product front end -- can show and drive it; until the app exists, keep the terminal console able to reach it too, or label it console-only. `/sessions` and `/session <name>` surface session history in the console instead of raw JSON, and `session/list` exposes the same history to the app. `pwr-cli` test executable startup still needs an offline fix before console snapshots can be trusted. | Can reduce the R5 UX blocker. It does not replace product task evaluation. |
| Verification and acceptance | Expand check-discovery, unrunnable-check, baseline-preservation, web-asset and false-acceptance fixtures using local workspaces and wrong implementations. | Can reduce false completion risk before R5. It does not certify semantic task success. |
| Policy and sandbox | Strengthen approval descriptions, path-scope fixtures, protected-state reads, network/local-service distinctions and macOS sandbox tests. | Can support broader authorization claims on macOS. Linux/Windows isolation still needs those OS environments, though not an LLM. |
| Corpus quality | Add task-statement/acceptance consistency checks, wrong implementations, reference-fix validation and semantic-answer fixtures where deterministic. The current `external-v1` slugify task already carries the plural-acronym hidden checks and wrong implementation needed to avoid the false acceptance recorded in the log. | Can improve evaluator validity before live campaigns. It does not create new model measurements. |
| Documentation/readiness claims | Keep README, audit, roadmap and experiment log aligned so alpha/product/research claims do not outrun evidence. | Can be closed by review and diff only; no runtime needed. |

Not offline-closable: deployment capability probes, calibration, local failure
rates, B0/B1/B2 uplift, R3/R4 promotion, R5 product acceptance rates and any R6
extension benefit. Those require live deployments or user-facing capability
experiments and must remain unclaimed until measured.

## R0 — Make the comparison trustworthy

Question: can the evaluator account for every assigned trial and reject an invalid comparison?

Implement the strict paired-trial contract around the existing verifier-supplied evaluator. Freeze the baseline source/binary/config and preserve dirty-build identity. Record all attempts; retain conditional legacy metrics under explicit labels.

Exit: duplicate keys and undeclared task/verifier/deployment/sampling/budget differences are rejected; provider-failed, timed-out and interrupted trials remain represented; raw outcomes reconcile to counts/time and available token totals; missing counters remain unknown. Deterministic corruption/failure fixtures pass and one small campaign can be exported with no unaccounted assigned trial. No model-quality claim follows.

**Immediate engineering task:** the strict paired-trial accounting contract in `pwr-eval`, including all-attempt denominators and provenance validation. It prevents false uplift before further harness design is tested.

Progress, 2026-09-12. IMPLEMENTED: `compare_strict` in `pwr-eval`, reachable as `pwr eval compare --strict --declare <field>`, with twelve fixtures in `crates/pwr-eval/tests/strict_pairing.rs`. It rejects a trial recorded twice on one side, rejects any difference in suite, corpus revision, harness revision, deployment fingerprint, hardware key, execution profile or a named sampling parameter that was not declared as the treatment, refuses two campaigns that differ in nothing, and counts every assigned trial — a provider failure is an unresolved trial rather than an absent one. Token totals carry coverage, because an artifact stores `0` both for a run that generated nothing and for a run whose backend reported nothing. `compare` is unchanged and its reports are read as before.

Completed 2026-09-13. A campaign writes a `TrialManifest` of every assigned trial before the first one runs, and `reconcile` compares it against what was recorded: `unaccounted`, `unassigned`, `duplicated`, and a `complete` flag that decides whether the campaign's rates describe what it set out to do. The campaign's own result carries all of it, so a campaign that lost trials says so rather than reading as a complete one with a smaller denominator. Each trial also writes immutable `started` and `outcome` artifacts as it crosses those boundaries, so completed outcomes do not exist only in the final aggregate report. The next runner writes an immutable `active` artifact when the task has a run id, workspace, budgets and action-loop entry, so an interrupted campaign can say whether a trial never began reasoning or died inside a run. Active runs now also emit immutable event-chain checkpoints every 30 seconds under `traces/checkpoints/`, preserving partial evidence if the process dies before the final trace. `pwr eval run --resume <manifest>` continues a partial campaign only when all recorded conditions still match and skips immutable outcomes already present. `scripts/r2_pilot_status.py` writes an explicit `unfinished-trials.json` export for every started trial without an outcome, including whether it entered the action loop. An interrupted trial is its own class rather than an unresolved one — an interruption says nothing about the deployment and a timeout says it did not answer in time. `turn_timeout_secs` is recorded and is part of the condition, so two campaigns whose turns were allowed different lengths no longer pair silently; an older report that lacks it compares as `unrecorded` rather than as an assumed value.

One earlier claim here was wrong and is withdrawn: verifier and per-task budget hashes were *already* in the trial identity, carried by `corpus_rev`, because `Task` holds `visible_verifier`, `hidden_verifier`, `max_actions` and `time_budget_secs` and the revision hashes every task. Only the campaign-level turn budget was missing.

Closed 2026-09-13. One small campaign ran end to end and its reconciliation was read: `external-v1` on `glm-4.7-flash:q8_0`, manifest `01a09a98`, five trials assigned and five recorded, none unaccounted, duplicated or unassigned. The campaign scored 0 of 5, and reading why is what the accounting is for: four trials had done the work and a harness defect rejected three of them at completion (see the experiment log, "the first campaign outside the Qwen family"). The defect is fixed and pinned. The exit named no quality bar and none follows.

## R1 — Establish one measurable session contract

Question: are chat, scripted tasks and evaluation observing the same actions, evidence and outcomes?

First capture the current chat and scripted behaviors as explicit fixtures. Extract a shared session step/result boundary using the existing executor and scripted runtime. Preserve conversation and terminal UX. Replace the false “all checks passed” message with evidence-specific outcomes; do not secretly impose repair acceptance on diagnosis. Expose treatments through immutable configuration IDs, not forked loops.

Progress, 2026-09-12. IMPLEMENTED: the conversation no longer claims checks passed when it observed only that none newly failed. `check_verdict` in `crates/pwr-cli/src/main.rs` separates every check green, baseline preserved with the still-failing commands named, and a new failure; five fixtures pin it, two of which fail on the behaviour replaced. This is one decision point of the two-loop divergence, not the shared runtime. Also IMPLEMENTED: `crates/pwr-cli/src/two_loops.rs` drives both loops from one script against one workspace and labels each difference DECLARED or DEFECT, which is the fixture capture this milestone opens with. `converse::take_turn` had no end-to-end fixture before it. Two defects were pinned — the conversation bypassed `context::compose` and so received no repository retrieval, and a tool result reaches the deployment in two shapes — and three equivalences are pinned as equivalences. The first is closed: `context::compose_turn` composes a conversation turn from the same sections, retrieval and eviction as a run's opening, against the room the history has left, and compaction now tells retrieved passages from things the operator asked for. The second is closed too: `action_outcome` and `tool_result_message` are the only place an action's outcome and its envelope are built, so a deployment answering either loop reads one protocol, and the conversation now receives the denial-versus-failure distinction and the failure category it never had. The conversation now receives all four sections a run receives: system instructions merged with the deployment's own suffix, a session ledger composed every turn from the files as they are on disk, repository passages ranked against the request, and the request. What remains of the divergence is declared rather than accidental — a conversation sends no `status` block because it has neither a plan nor an action budget — and the structural work is untouched: `converse::take_turn` and `run_action_loop_with_prompt_budget_and_context_tiers` are still two state machines with their own compaction, loop detection, recovery budgets and completion semantics. Eleven fixtures in `two_loops.rs` now watch the boundary from outside, which is what the extraction was missing. The extraction itself has begun where the conversation had less information rather than merely different: `repetition::RefusalStreak` is the first piece of the scripted loop's decision half to become shared, and the conversation gained the repeated-refusal detection it never had — it could previously propose the same stale-hash edit until its turn ran out, emitting no `loop.detected`, which left `pwr diagnose` blind to a stuck conversation. No-progress detection followed it: `stall::record_effect`, the effect signature and the window rule are shared, and the conversation now names a circle of reads or an edit and its revert — while leaving investigation alone, since a window counts only when nothing in it was novel. The run's behaviour is unchanged in both cases: same thresholds, same events, same words. A conversation turn also has an action budget for the first time — MASTER_SPEC's eighth principle was unmet on the product path, where nothing counted between one action and the next. It is soft where the run's is hard, which is the declared difference: a run is unattended and its budget is a cap, a conversation has someone in front of it and the budget is a place to check in. The turn ends with a reason, the work intact, and the next message carries on. The threshold is anchored to `DEFAULT_MAX_ACTIONS` rather than measured, and is labelled as a choice until somebody measures how many actions a turn of chat takes. Backend faults are bounded separately from deployment faults on the same path, and a turn survives them instead of losing its history to an `Err` while its edits stay on disk. Measured-tier context recovery followed, closing the last recovery gap: the turn receives the calibration profile's admitted stable points and drops to a measured window when the backend refuses a prompt, keeping the conversation whole where compaction would have spent part of it. Plan state remains scripted-only, and that is now a decision rather than an omission: the conversation keeps no plan, because the research contract's fourth open question preregisters a comparison of no plan, a model-written plan and deterministic dependency state and states that there is no universal plan-first default. Adopting one in the product before running that comparison would be the default the contract refuses, and would grow the catalogue a small deployment chooses from — the cost H1 exists to measure. A plan substitutes for an absent person; a conversation has one, and the objective is restated every turn by the only party entitled to change it. Open question 4 on long multi-file tasks is what would change this.

Completed 2026-09-13 apart from the structural extraction. The third path exists: `pwr eval run --mode product-path` lets the workspace's checks be discovered as they are for a user, where `verifier-supplied` — still the default, and what every existing report is — hands the agent the check the corpus chose. The two measure different things, are not pooled, and a comparison across them is refused unless the mode is declared as the treatment. It is practicable only because a discovered check that cannot run is now classified at the baseline and exempted: that obstruction is exactly why the corpus verifier was adopted in the first place, recorded in `evaluate_task` as three runs on more-itertools that had correctly fixed their bug being recorded as failures. Cancellation and user redirection have fixtures too, so all six cases the exit names are covered.

Completed 2026-09-15, including the structural extraction. `pwr_orchestrator::session` is the step both loops call for every action: `gate` decides repetition and approval, recording `loop.detected`, `approval.decision` and a denied action the same way for both; `perform` announces a workspace-changing action, executes it against a read history, receipts it and checkpoints what the session has changed. Around it, both loops share one envelope for an unusable reply (`ReplyFault::message`, where the run sent JSON and the conversation bare text), one rule for the measured window a refused prompt retries at, one bound on consecutive malformed calls that no longer charges them as actions in a turn, and one terminal classification — a turn's ending is now recorded as `conversation.turn_ended` with the run's `TerminalClass`, where it had been recorded nowhere. The scripted run gained the intents, receipts and checkpoints the conversation had (the continuity prerequisite R5 names); the conversation gained the read history, the approval gate and a bounded malformed-call stop the run had. Reports built from a trail now say they are a reconstruction and not a resumed run. The product-path evaluator runs the scripted loop, so it is on the same boundary by construction. Each closed difference has a `two_loops` fixture that fails on the old behaviour; 31 fixtures now watch the boundary. What stays different is declared there: a plan, completion verification and a hard cap for the run against a check-in budget for the turn; a backend fault ending a run (measured as `provider`) against three retries in a turn; a malformed-call bound measured per deployment against a fixed three; approvals from the caller's prompt against those Settings grant; and two compaction algorithms, kept apart until R3 because its control arm is today's compaction.

Exit: equivalent objective/action fixtures through chat, run and a product-path evaluator produce equivalent effects, policy decisions and evidence-bearing results. Mode-specific acceptance differences are declared. Red baseline, no verifier, denied edit, cancellation, compaction and user redirection cases pass. A replay report cannot be presented as resumed execution. Keep `verifier_supplied` as a separately identified mode for older corpora.

## R2 — Measure local failure regimes

Question: what prevents the target deployments from completing representative coding tasks?

Inventory actual installed deployments and version their identity/settings; do not reuse old tags as capability facts. Start with at least two model families, ideally including a 7–14B and a 20–35B deployment if the host can serve them. A larger cohort or paired quantizations enters only when fit is observed. Lack of a cohort narrows the claim rather than triggering downloads by default.

Pilot at least 30 distinct development tasks across six repositories and five workflow classes, with three repetitions where useful for sampling variance. These are coverage floors, not statistical power. Include context/output stress, ambiguous localization, edit failures and long multi-file work. Compare B0/B1/B2 at frozen settings; include oracle-context diagnostics and cold/warm cost observations.

Exit: all attempts have outcomes/cost coverage, per-deployment failure counts and uncertainty, a reviewed unknown-cause bucket, and a predeclared choice of the largest avoidable failure to test. Produce a pilot-based confirmation sample-size/stopping plan. No winner is inferred from a single probe or saturated task class.

Progress, 2026-09-13. IMPLEMENTED: the two controls. `baseline::run_conventional_loop` is B0 and `baseline::run_staged_loop` is B2, both in `pwr-orchestrator`, both built on B1's action protocol, adapter, reply faults, executor, result envelope and event log, so the evaluator scores all three the same way; `pwr eval run --arm b0|b1|b2` selects one, the arm is a recorded condition, and two campaigns that differ in it pair only when it is declared. `--oracle-context` hands over the files a change belongs in, as the diagnostic that bounds localization failures. `--only` runs chosen tasks of a corpus without copying them. `TaskKind::Feature` counts features as their own workflow class. `corpus/external-v2.json` adds fifteen upstream-derived tasks in four more real repositories (pyparsing, filenamify, tomli, idna): eight repairs, four features, three diagnoses, each with a deliberately wrong implementation per code task. With `external-v1` that is six real repositories. The pilot is preregistered in `experiments/r2-pilot-20260913/protocol.md`: thirty tasks over five workflow classes, two deployments from two families, B0/B1/B2, and a failure-classification order and choice rule written before any trial. The cohort is `glm-4.7-flash:q8_0` on Ollama and the `qwen3.6-35b-a3b` GGUF on LM Studio, both probed; both are 20–35B, so the 7–14B size is not covered and the claim narrows accordingly.

Progress, 2026-09-15. The full pilot ran once in `experiments/r2-pilot-20260914-b5bd562-full-active-probe`: 46 campaigns, 254 of 254 trials recorded across both deployments, all three arms, the variance seeds and the oracle-context diagnostic, none unaccounted. Its traces show four harness defects that confound the comparison — a cargo warning read as an unrunnable check (D1), an unlistable state directory breaking JavaScript test runners (D2), B0/B2 never preparing their LM Studio window and so running Qwen's controls at 8,192 tokens against B1's 16,384 (D3), and unverifiable completions scored differently per arm (D4). All four are fixed in source with fixtures. `scripts/r2_classify_failures.py` now classifies from traces in the protocol's order of precedence and applies the choice rule to B1 primary trials: 21 harness, 12 unfinished, 8 wrong_change, 1 scope, 18 resolved. On the 19 tasks no defect touches, GLM shows B0 8, B1 8, B2 6 — no uplift. The largest testable class is `unfinished`, with 41% of actions in budget-exhausted B1 runs re-reading unchanged files, mostly after compaction; that maps to H2 and is provisional. `analysis.md` in that directory records the rates, defects, classification and a provisional McNemar-based confirmation size (≥180 task-seed pairs per deployment for a 10 pp effect).

Progress, 2026-09-15, later. The rerun is preregistered in `experiments/r2-rerun-20260915-8648aed` on a frozen binary built from the fixes (`pwr-8648aed`), without the oracle-context diagnostic and split into resumable parts run in sessions of about seven hours. Session a (GLM B1, GLM B0, Qwen B1) recorded 90 of 90 trials: GLM B0 11/30 and B1 11/30, with B1 costing 10% more wall time and 44% more generated tokens; Qwen B1 18/30. The D1 and D2 fixes are visible in the traces: every red-baseline Rust task verifies, and the JavaScript visible checks run. The traces also exposed D5 — the repository index did not exclude `.pwr-scratch`, so a read-only diagnosis whose check wrote npm logs failed verification — fixed in source together with two mechanical losses (`search` refusing calls without `max_matches`, and commands refused for repeating the program in `args`). None of these is in the frozen binary, by design.

Progress, 2026-09-16. Session b (GLM B2, Qwen B0, Qwen B2) recorded its 90 trials, completing the rerun at 180 of 180. Resolved of thirty: GLM B0 11, B1 11, B2 10; Qwen B0 13, B1 18, B2 12. Paired, GLM is a tie in every comparison (p = 1.00) and Qwen has PWR ahead of both controls without significance (p = 0.23 against B0, 0.15 against B2, 0.065 against B2 once the trials D6 touched are dropped). The staged arm loses on both deployments and costs the most on each, so it is dropped as a candidate default. Eleven trials are classified `harness` for D6 — the prompt budget enforced on a character estimate the backend's count exceeded by a median 1.37x, so prompts filled the window and turns returned reasoning with no call — found in these traces, fixed in source with D7 and D8, and in no frozen binary. The choice rule applied: B1's unresolved trials are 21 `unfinished`, 9 `wrong_change`, 1 `harness`, and the hidden check already passes on 17 of 30 trials per deployment, so the work is right and the run cannot declare it in budget. `analysis.md` and `scripts/r2_rerun_analysis.py` carry the tables; the sized confirmation is about 97 paired tasks per deployment against B0 and 72 against B2.

Progress, 2026-09-16, later. Session c ran the six variance tasks at seeds 2 and 3 under B1 on both deployments, 24 trials, completing the rerun at 204. Three of the twelve task-deployment pairs flip between resolved and unresolved on the seed alone, two more keep the outcome and change the failure class, and seven are identical: a quarter of the sampled tasks are not deterministic under a fixed condition. That is the scale of the difference the arms showed, which is what the paired tests said and this measures directly. The confirmation is re-costed accordingly: a task must be scored by a majority of three seeds or analysed at the trial level, which at the measured pace is 582 trials and 35 host hours for Qwen, 48 for GLM. The binding constraint is not host time but the corpus -- about a hundred paired tasks against the thirty that exist, each an upstream fix with a hidden verifier and a wrong implementation shown to discriminate.

REMAINING: A separate B1-versus-B1 comparison declaring `harness_rev` would measure the post-freeze repairs D5–D8. A confirmation of B1 over B0 at the sized corpus is a campaign of its own, and is better judged after R3. B2 used one representation for every deployment; the development-selected best representation per deployment that `evaluation.md` asks for was never selected, and the arm is now dropped rather than improved.

## R3 — Test one mechanism before building a subsystem

**SHELVED 2026-09-17** for the refactor proposed in [the redesign](redesign-2026-09-17.md): evaluation by harness area, models loaded without a probe, an embedded MLX engine, a hardware catalogue. When it resumes it splits into a mechanism test at a small window (A5) and a completion test at the host's window (A6). Everything below is old regime.

Question: does the selected H1–H5 intervention remove the diagnosed loss at equal budget?

Progress, 2026-09-17. The corpus is assembled: `corpus/longhorizon-v2.json` holds 53 tasks -- v1's 17 and 36 from sqlparse, markdown, jsonschema, marshmallow, arrow and tabulate: 18 base tasks mined from recent commits and one injected variant of each. A base task entered only if the project's suite was green at the parent, the commit's own tests were red there and green at the fix, the commit only added tests, and a wrong implementation passed the visible suite and failed the hidden one; 30 commits were selected from 210 candidates and 18 survived. Ten variants withhold the rule their wrong implementation breaks and deliver it as a revision after four actions; the eight whose wrong implementation misses the whole task, or whose rule the statement already implies, get a colleague's comment instead after eight. `pwr check-corpus` passes on every task, each new task installing its project into a `.venv` inside the workspace that the index, the tools and scoring now exclude. The build is `experiments/r3-h2-preparation/builder/assemble_v2.py`. The corpus is 27 independent base tasks, not 53: a variant shares its base task's code and hidden tests, and the sample-size calculation must count it so.

Progress, 2026-09-16, evening. The post-freeze repairs were measured against the rerun in `experiments/r2-harness-revision-20260916`, preregistered: the same thirty tasks, the same arm, deployment and seed, `harness_rev` the only declared difference. They remove what they targeted -- no turn now returns reasoning with no call against eleven, malformed calls fall from fifteen to four, the mechanical refusals are gone -- and a run costs 122k generated tokens against 202k and 68 minutes against 107. The paired outcome does not move (14 against 17, p = 0.375). What remains is one loop, now isolated: compactions rise from 52 to 136, the re-read share after a compaction is unchanged at 56%, and **208 of the campaign's 226 re-reads are of a file whose earlier read a compaction folded away**, at 3.7 MiB of content shown twice. That is H2's target, measured on its own.

Design, **selected by R2's choice rule on 2026-09-16**: [H2 — evidence that survives compaction](r3-h2-evidence-state.md). The rerun leaves B1 with 21 `unfinished` trials against 9 `wrong_change`, and the hidden check passing on 17 of 30 trials per deployment while only 11 and 18 were resolved: the work is right and the run cannot declare it before its budget ends. H2 is the treatment aimed at that class, and R3 is now the next milestone rather than a conditional one.

Progress, 2026-09-15. PREPARED, not started. Decided: three arms at equal history budget (today's compaction, recency fill, evidence state), a 60% token target, one deployment at confirmation. Implemented offline with fixtures: `pwr_orchestrator::evidence` (context policies, an evidence section rendered from the audit and the workspace, stale-content accounting, an action-boundary hook), `eval run --context-policy/--context-share` recorded as a pairing condition, task injections (`revision`, `external_edit`) that leave existing corpus revisions unchanged, and per-trial mechanism metrics. An end-to-end fixture shows a revision lost to today's compaction and kept by the evidence state. `corpus/longhorizon-v1.json` holds 17 tasks from pycparser, bottle, pyasn1 and sqlglot — nine base tasks and eight injected variants — all passing `check-corpus` with a wrong implementation each.

Stopped, 2026-09-17, 11:43, after 35 of 79 trials (`experiments/r3-h2-dev-20260917`, old regime): every task forced at least two compactions and all 17 injections landed exposed to one, but 2 of 35 resolved and one task compacted 29 times in 40 actions -- at 16,384 tokens completion measured context starvation. Two defects found by its smoke trials are fixed in `f02b223`. R3 is to split into a mechanism test at a small window and a completion test at a realistic one; see [the redesign](redesign-2026-09-17.md).

REMAINING (old regime, superseded by the redesign once decided): the development run and the campaign. The R2 rerun has confirmed `unfinished` as the largest testable class (21 of B1's 31 unresolved trials, 2026-09-16), so that precondition is met; the corpus stands at 53 tasks (2026-09-17), short of the provisional sixty and with 27 independent base tasks; a development run on `longhorizon-v2` must show the tasks force at least two compactions and that the injections land where intended, and must re-derive the sample size counting variants with their base tasks; then preregistration and a campaign of roughly fifty host hours on one deployment.

Use the smallest implementation behind the existing boundary. Run contemporaneous paired controls; use the prompt × output factorial if the change alters both. Include clean cases and injected failures. Preserve raw evidence and all failed attempts.

Exit: a complete report labels the result positive, negative or inconclusive against its frozen practical/uncertainty gates. State retain, revise or remove. A negative report completes this milestone; it does not justify building the rest of the proposed feature. A positive development result must still pass confirmation before promotion.

## R4 — Test adaptation rather than a better fixed default

Question: can measured behavior select a policy that beats the best fixed policy on unseen repositories?

Freeze the candidate policies and selector using only development data. Reserve a repository-disjoint holdout and pilot-sized repetitions; pay for probes, loading, retries and cache effects. Apply H6 against both best global fixed and best fixed-per-deployment controls. Re-run under a second feasible deployment/backend condition to scope transfer; do not pool away opposite effects.

Exit for promotion: H6 achieves at least 15% all-attempt wall-time reduction, a positive paired reduction interval, and completion noninferiority within −3 pp for each claimed cohort, with no observed new scope/policy/false-acceptance violation. Otherwise keep a fixed policy and publish the narrower result. Only mechanisms with confirmed value become durable architecture.

## R5 — Prove sustained engineering in the actual product

Question: can a user carry a meaningful task through investigation, changes, checks, interruption and continuation?

Add exact checkpoint/reconciliation and session steering on the shared runtime, then evaluate actual check discovery and acceptance. Use at least 20 distinct held-out tasks from five repositories covering diagnosis, feature work, multi-file refactor/migration and environment/dependency work. Include pre-existing user changes, running services, interruption at action boundaries, and tasks spanning repeated context compactions. Model swap is a separate compatibility scenario, not a promised default.

Proposed product gate: at least 70% independent task acceptance with the task-cluster 95% lower bound above 50%; at least 80% of accepted tasks require no intervention beyond preregistered grants; zero observed lost user changes, duplicate destructive effects or false acceptance. Predeclare 60-minute ordinary and 120-minute long-task caps on the recorded host, plus aggregate token/operation budgets informed by R2; report misses rather than extend budgets after failure. These are research targets, not a release guarantee or cloud parity.

Exit also requires restart/cancellation/steering invariant fixtures and an inspectable conversation showing what changed and what was checked. If the quality gate fails, publish the failure classes and stay experimental.

Progress, 2026-09-13. IMPLEMENTED in the conversation loop, ahead of the evaluation: a turn ends with a snapshot of the messages, each action boundary with a checkpoint of the files the conversation changed and the content it left, and each workspace-changing action is announced before it runs and receipted after (`pwr_orchestrator::conversation`). `pwr chat --continue` and `/resume` restore the last complete turn and reconcile it against the workspace, telling both the deployment and the person what an interrupted turn did after that point, which files were edited or deleted since, and which writes may or may not have happened. A message typed while a turn works is delivered at the next action boundary as a revision of the objective instead of being refused. `/changes` shows the files the conversation changed and git's diff of them. Commands that load a model release it on exit when they were the ones that loaded it. Fixtures: `tests/conversation_continuity.rs` and the steering and continuity cases in `two_loops.rs`.

Progress, 2026-09-14. IMPLEMENTED: the TUI command list and summaries now expose `/sessions` and `/session <name>`, so a user can inspect saved sessions, branch drift, interrupted-run state and the first ledger line without falling through to JSON output.

REMAINING: the same checkpoint and receipts on the scripted loop, which is where they belong once R1's extraction gives both loops one runtime; the held-out product evaluation itself — twenty tasks in five repositories disjoint from R2's, with scripted interruption, restart, steering and pre-existing user changes — and its preregistered gate. None of this is evidence that the product meets the gate.

## R6 — Admit one extension at a time

Question: which additional capability improves the coding workflow enough to justify its integration and cost?

Choose vision/browser inspection, one MCP integration, another OS sandbox, or H7 scoped workers based on observed coding failures. For vision compare screenshot versus DOM/text evidence with a supported deployment and a clean unsupported path. For workers compare serial context isolation before parallel throughput. For host access test concrete scopes, policy inheritance and observable external effects.

Exit: one separately preregistered capability benchmark passes its acceptance/resource gate and policy fixtures without degrading the R5 regression set. No extension is needed to claim an H1–H6 result. No general-purpose desktop product or plugin marketplace is implied.

One at a time is a statement about evidence, not about the destination. Browser and computer control, network research and MCP adapters are all on the intended path; admitting them one at a time is what makes it possible to say which one helped. A capability that arrives here should arrive drivable from the conversation, since a capability the user cannot ask for plainly has not shipped.

## Product surface track — S1 to S3

**Decided 2026-09-16: PWR's product is its own desktop app, for macOS and
Windows, and that app is its only front end.** A standalone application in the
manner of the ChatGPT and Claude desktop apps -- a simple interface that makes
clear what the agent is doing -- not a plugin for code editors or IDEs. Editor
integration is not a goal. The terminal console stays as the development and
research tool the campaigns run through until the app covers it; it is not a
product surface.

Question: can a native app drive the same session the terminal console drives,
without a copy of the loop, and make a local agent's work legible enough to
trust?

Runs beside R2–R5 rather than after them, because the app is a client of the
session boundary R1 produced and does not change what the research milestones
measure. Design: [`pwr serve`](pwr-serve.md).

- **S1 — protocol. Done 2026-09-16.** `converse` lives in `pwr-orchestrator`,
  shared by the CLI and the server; `pwr serve --stdio` speaks the Agent
  Client Protocol -- sessions created, loaded, resumed, listed and closed; prompt
  turns with attachments, tool calls and diffs; permission requests from
  `session::gate`; cancellation; the console's commands; steering; models and
  approval settings -- over the console's own turn, resume and attachment
  handling. **B10, 2026-09-20:** `_pwr/models` can also select a model from
  the active backend's discovered catalog for that workspace and returns its
  computed window. ACP was chosen for its specification and schema, not for the editors
  that also speak it. File system and terminal stay inside PWR's sandbox and
  audit; the app shows files and diffs and never writes them. Exit met: protocol
  tests, validation of every message against the published schema, and a golden
  transcript of a real turn. The manual end-to-end pass moves to S3, with the app.
- **S2 — toolkit spike.** GPUI and Slint build the same screen (conversation,
  action feed, diff, permission prompt) as `serve` clients on macOS and Windows,
  judged against criteria fixed before the spike, including how clear the
  permission prompt and action feed are to someone who has not used PWR. The
  clarified product -- conversation-first, Windows required -- favours Slint;
  the spike decides. **B10 started:** the catalog/selection seam has a Slint
  client at `crates/pwr-app-slint/`; it is a separate workspace while its
  compiler conflicts with Ratatui's exact terminal dependency. **GPUI 0.2.2 is
  provisionally excluded:** upstream documents macOS/Linux only, failing the
  required Windows path before a duplicate screen adds evidence. Slint still
  needs Windows and fixed-manual-check evidence. Download progress and recovery
  remain separate protocol work. See [`frontend-spike.md`](frontend-spike.md).
  **Revised 2026-09-22 by the maintainer: Tauri 2 with an Angular frontend
  replaces Slint as the candidate; "no web UI" is withdrawn.** The Slint client
  reached the manual pass and the maintainer's website rerun succeeded through
  it, but a conversation-first agent app needs what Slint is weakest at and the
  web is mature in: markdown streamed as it is generated, highlighted code,
  large diffs, long virtualised lists, and rich motion. Measured the same day:
  three crashes and freezes in the Slint client, two of them layout binding
  loops on the conversation. What the original decision protected is kept:
  the app remains a client of `pwr serve --stdio` started by a Rust process
  (Tauri's), the core and its sandbox are unchanged, protocol types are
  generated from the Rust ones, and Tauri uses the system webview (WebKit,
  WebView2) rather than a bundled browser -- memory beside a 20 GB model
  matters, which is why Electron stays excluded. Angular because its signals
  suit a UI driven by a stream of protocol events, and it is the maintainer's
  stack. The Slint prototype has since been removed; its criteria and
  observations remain in `frontend-spike.md` as a historical decision record.
- **S3 — the PWR app.** The chosen toolkit, for macOS and Windows, covering
  everything the terminal console does, including model download and preparation
  progress -- model selection now crosses the protocol, while progress reporting
  and recovery still need a request first. The Windows build also requires the core on Windows, and above
  all execution isolation there: today the sandbox is macOS Seatbelt only, and the
  harness does not ship where it cannot confine what the agent runs. Exit: the
  manual pass with the app recorded, every admitted capability drivable from it,
  Windows isolation passing the same confinement tests as Seatbelt, and signed
  installable builds for both platforms.
  It enters R5's product evaluation; it does not replace it.

## Historical roadmap and campaign record

The remainder is preserved from the pre-redefinition roadmap, including its generated M0–M6 block and historical corrections. “Current,” “complete,” “missing” and model rankings below refer to those recorded revisions. The [2026-09-12 audit](adaptive-runtime/CURRENT_ARCHITECTURE_AUDIT.md) supersedes their implementation descriptions; R0–R6 above supersede their ordering. `milestones.json` remains the historical M-series manifest and `scripts/milestones.py` must still reproduce its block exactly.

What PWR is now, and what is left. The measurements behind every line here
are in [experiment-log.md](experiment-log.md), which keeps the failed
experiments and invalidated results as well as the ones that held.

## Current implementation status

Generated from [milestones.json](milestones.json) by `scripts/milestones.py`.
Milestone state used to be asserted in three places and they disagreed — the
same milestone declared complete and in progress in one file. Edit the
manifest; do not edit the table. CI fails if they drift apart.

<!-- generated:milestones -->
<!-- Generated from docs/milestones.json by scripts/milestones.py. Edit the manifest, not this table. -->

| Milestone | Status | Evidence recorded | What remains before advancement |
|---|---|---|---|
| M0 Foundation | **complete** | Cargo workspace; versioned domain contracts; SQLite event log with a per-run hash chain and a verifier; typed run events; JSON, Markdown and JSONL reporting. Property tests cover serde round-trips, Observation variant integrity, deployment fingerprint sensitivity, UUIDv7 ordering, hash determinism, calibration sampling floors, execution-profile authorisation, typed execution budgets and event round-trips. | Real migrations beyond the two in place, and an artifact table. |
| M1 Discovery | **complete for the seven target deployments** | doctor captures host and backend facts. models inspect --probe runs the full capability suite with a content-addressed, non-overwritable artifact each. A run requires a matching artifact, and the measured emission rate shapes how much malformed-call patience it gets. Cancellation closes the connection the backend writes into, asserted from the server's side. | The trial count is not calibrated against measured variance. |
| M2 Calibration | **v5 implemented; deployment revalidation pending** | v5 gates exact needle recall, measured occupancy and complete streams; pressure before and after generation participates in admission. Offline adversarial provider tests cover wrong/missing evidence, incomplete streams and post-generation pressure. Previous v4 profiles remain historical measurements. | Recalibrate the MVP cohort under v5 before new runs. Needle recall does not measure repository reasoning; unknown pressure/load observations remain unknown. Linux and Windows host probes. |
| M3 Safe execution | **complete on macOS** | Gitignore semantics through one walker shared by the index and the tools; reads denied outside the workspace with a measured allowlist; writes confined; network denied without a grant; bounded incremental I/O with process-group kill; every attempt audited allowed or denied, in five outcome classes; corpus preparation and external verifiers under their own bounded policy; services owned by a supervisor that kills them on drop. | Linux and Windows adapters. Under --provision an arbitrary executable with a network can still read the system and toolchain paths; that wants a separate process or VM. |
| M4 Agent task loop | **complete** | Baseline includes narrow, broad and explicitly quarantined checks before editing. Normal completion requires passing non-exempt checks and preserved baseline; no-verifier completion is refused. Compaction retains the typed task under the production prompt layout. Regression tests cover unchanged red checks, broad regressions, explicit exceptions and task retention. Typed transitions, plans and replayable log remain implemented. | Resuming into replayed state, semantic user clarification and large-repository effectiveness remain open. Baseline diagnostic equality is conservative and cannot establish individual test identity. See local-agent-research.md. |
| M5 Evaluation | **historical campaigns; current revision unmeasured** | Previous campaign results are retained. New source fingerprints distinguish dirty builds; lexical symbol mentions are named honestly; ambiguous terminal symptoms remain unattributed. Eval now honors planning and observes runtime pressure but still receives the corpus verifier. | Validate current revision on predeclared held-out tasks and repetitions. Product-path verification discovery/escalation, per-task durable campaign outcomes, semantic answer scoring and task/repository-level uncertainty remain open. No current model ranking is established. |
| M6 Beta | **not ready** | Historical task rates do not demonstrate large-repository competence or parity. Offline harness regression checks do not establish deployment quality. | Held-out large-repository evaluation, MVP cohort revalidation, semantic evidence assessment and usability beyond --json. Follow benchmark-design.md and local-agent-research.md; keep prior measurements distinct from current claims. |

Campaign evidence: Historical campaigns through 2026-09-05 are retained in experiment-log.md and roadmap.md. The 2026-09-06 v5 changes have two development campaigns and no held-out measurement: the local-v5 pilot (thirty trials, three deployments) and its read-only follow-up (nine trials), both under experiments/ with their protocols, frozen binaries and per-trial reports.
<!-- /generated:milestones -->

## Current research scope — 2026-09-06

The MVP continues with the installed local deployments, using qwen, ornith and
gpt-oss as the main comparison cohort. The [research contract](local-agent-research.md)
records implemented corrections, remaining limitations and the next controlled
experiments. No new model campaigns accompany this revision. Calibration v5
requires new profiles, and changed acceptance/evaluation contracts mean earlier
numbers are historical, not measurements of the current system.

The dated observations below are retained as the project's research record.
Statements assigning a budget/protocol failure to a deployment are hypotheses
unless a controlled comparison isolates that cause. The current milestone table
and 2026-09-06 measurement amendments supersede older readiness claims.

## What remains

Every item the audit of `cee5ebd` raised has been closed; these are what is
open now, and each says what would settle it rather than when it will happen.

### Before any number here is quoted again

- **A campaign is all-or-nothing.** The report is written at the end, so a
  campaign stopped part-way leaves nothing: no per-task outcomes, no malformed
  kinds, no action counts. The challenger's first campaign was stopped at 2 h 26
  against qwen's 49 minutes for the same thirteen tasks, and everything except
  the timing was lost. The run that most needs partial results is the one too
  slow to finish.
- **Resident footprint during a campaign is unmeasured.** The challenger held
  41 GB by the backend's own account and 44.9 GB resident, against roughly 18
  for the primary. Calibration observes memory pressure; evaluation observes
  none. On a 64 GB machine that is the difference between a deployment that fits
  and one that merely runs, and the host-wide runtime lease was written before
  anything had measured how close to the edge that is.
- **Every measurement taken before 2026-09-05 went through a broken adapter.**
  Tool calls were read from the backend nested under `function` and echoed back
  flat, so every assistant turn in a conversation's history read as a call with
  no name. qwen tolerated it; gpt-oss:20b mirrored it and scored 1 of 8 where it
  scores 6 of 8 with the messages correct. The rates already recorded measured
  the system as it stood and are not void, but none measured what it would have
  measured correctly, and nothing compares across the fix.
- **`muse-glimmer:30b-mlx` is refused at admission** for lacking an observed
  `edit` capability, on three probe trials. The gate is right to refuse; three
  trials are not enough to write a deployment off, and the probe's own
  documentation says so.
- **Five deployments are measured on the floor corpus, one seed each.** qwen,
  ornith and gemma4 at 8 of 8; nemotron and gpt-oss at 7. One seed says who
  continues and never who is better, so this orders nothing — but four of five
  now clear a floor that two cleared yesterday, and the difference was the
  adapter rather than the models.
- **The three carried forward do not separate on capability.** Thirty-nine runs
  each on `m6-hard-v1`: 38 of 39 for ornith and qwen, 30 of 31 for gpt-oss, with
  intervals overlapping almost entirely. They separate on cost, and on which
  cost — ornith at half the wall clock, qwen at 60% of the tokens and 74% of the
  turns. Naming a winner without naming the scarce resource would be inventing a
  result.
- **`gpt-oss:20b` lost a fifth of its runs in transport, and no longer does.**
  The cause was ours: a backend rejecting a generation it could not parse does
  so both on the stream and while opening it, and only the first was handled.
  Fixed, the eight become zero and the corpus completed rises from 0.769 to
  0.949. It also corrects the ranking — it looked fastest at 27.2 minutes partly
  by stopping early, and measured whole it is 34.6, the slowest of the three
  with the noisiest action channel.
- **The GPU is saturated by one generation, measured twice.** With the backend
  serving one request at a time two agents are queued; with
  `OLLAMA_NUM_PARALLEL=2` they genuinely run together at half rate each. Both
  configurations aggregate to 1.04× a single generation, so a second concurrent
  sequence takes from the first exactly what it returns. Splitting work between
  two agents therefore buys latency hiding, bounded by the 16–30% of wall clock
  spent outside the model, and not throughput. `gpt-oss:20b` at 12 GB remains
  the only deployment two of which fit at once, which is now a fact without a
  use.
- **The harness does not bound throughput.** All three deployments generate in
  campaign at or above their calibrated rate at the smallest tier — gpt-oss 48.6
  t/s against 47.8, ornith 54.2 against 52.8, qwen 19.9 against 17.8 — with
  82–84% of wall clock inside the backend.
- **Scope has discriminated nothing across nine deployment-campaigns.** Every
  deployment respected it on every run, including the ones set down. It is a
  real property and it is not a selection criterion on this corpus.
- **Three deployments carried forward, three set down.** `ornith-1.5:35b` at 13
  of 13 in 11.3 minutes, `qwen3.8:27b-mlx` at 13 of 13 on 17,597 tokens — the
  most economical by half — and `gpt-oss:20b` fastest at 8.9 minutes with two
  runs lost in transport. Set down: nemotron at 11 of 13 even with its budget
  doubled and 2.5× ornith's tokens, gemma4 at 11 of 13 in 78 minutes, granite at
  10 in 115. Each was checked for a harness cause first and each limit was its
  own: the budget for nemotron, the ladder's measured decay for gemma4, two time
  budgets and a broken action channel for granite. None is deleted; they are not
  being measured further until something changes that would move them.
- **Six deployments are measured on `m6-hard-v1`, one seed each.** ornith and
  qwen at 13 of 13, gpt-oss at 11 of 11 with two provider failures, gemma4 11 of
  13, granite 10, nemotron 9. Cost spans thirteenfold, 8.9 minutes to 115.2 on
  identical work. One seed does not rank, and the spread on cost is far past
  what variance explains.
- **`nemotron` failed three of four on the action budget, which is ours.** One
  of them had the repository green when it ran out. Whether it is verbose or the
  budget is tight has not been measured, and it cannot be discarded until it is.
- **Only one deployment has been measured across seeds.** `qwen3.8:27b-mlx` has a
  calibration under the occupancy ladder and one campaign. The challenger and
  the five controls have neither, so nothing here compares deployments.
- **The harness-failure bar is met, on `m5-frozen-v1` only.** The nine failures
  were read and none was a broken tool: every one was a command the deployment
  ran on purpose exiting non-zero. The metric is split, the bar attaches to
  `harness_failure_rate`, and two campaigns measured 0 of 64 and 0 of 71 with
  the whole interval below 0.10. `external-v1` has never cleared it — its 34
  attempts fell one short — so the bar is met on the corpus that cannot produce
  the failures, and undecided on the one that can.
- **The bar cannot be re-derived on the corpus it judges.** A pilot has to be
  disjoint from what it calibrates, and `external-v1` has four tasks — one of
  which the provider failed on. So corpus size, not the derivation rule, is
  what blocks it.
- **A quarter of `external-v1` never ran.** One task of four ended in a
  provider failure with zero turns, `[0.046, 0.699]`. An interval that wide
  says only that it is not obviously rare.
- **A corpus can score a rule it never states, and did so twice.** A hidden
  verifier asserted a contract the statement omitted, and `scope_respected`
  judged four of eight tasks against an `allowed_files` list that reaches the
  deployment nowhere. Both are now fixed and both are pinned by a test, but
  nothing checks the general property: that every criterion a task is scored on
  follows from the words the deployment is given. Each was found only because
  an artefact was kept, one campaign apart.
- **Every attack-task result before 2026-09-04 measured the harness.** The
  deployment recognised a path-traversal exfiltration and a prompt injection and
  refused both — and had no way to say so, since none of the nineteen
  capabilities expressed a refusal. The loop counted each refusal as a malformed
  call, charged it to the malformed budget, and killed the run on the third.
  `decline` is now an action and a declined attack task is resolved, but no
  campaign has yet run with it.
- **Taking the first of several calls is not free, and a smaller sample said it
  was.** Five occurrences in one campaign were all parallel reads; the next
  produced `replace_text, replace_text`, where taking the first performs one
  edit, drops the other, and leaves the deployment believing both landed. The
  detail now carries each call's target, and the decision waits for enough of
  them to be worth making.
- **Fifteen of twenty-six capability areas still have no task.** Testing, root
  cause analysis, error recovery, performance and navigation were added on
  2026-09-05. Concurrency, DevOps, dependencies, git awareness, long context,
  ambiguity, planning and long-horizon autonomy remain untouched.
- **A read the harness allowed was discarded before it could be used.** One read
  at the 64 KB output bound estimates at 16,384 tokens and the history budget is
  half of a 32,768-token context, which is 16,384 — two limits set independently
  in two crates and exactly equal. Compaction fired before the next request and
  dropped the result. Fixed: the exchange the deployment is about to act on is
  kept, bounded to half the budget so compaction still shrinks. `monotonic-table`
  goes from 0 of 1 in 39 turns to 1 of 1 in 13, and the peak prompt from 3,697
  to 32,703 of 32,768 — the first time any run has entered the regime the
  calibration ladder measures.
- **A bigger repository does not make a bigger prompt.** Thirty-two files peaked
  at the same 12,485 tokens as twenty-seven, because the context compiler bounds
  repository excerpts. What grows a prompt is the conversation, and the largest
  thing in it is a tool result carrying a file read. `monotonic-table` is built
  for that — a 96 KB table against a 64 KB read bound, so it must be windowed —
  and it is the first task expected to force compaction, which until now only
  fixtures have exercised.
- **The corpus is saturated again at eighteen tasks.** ornith resolves 18 of 18
  in 16 minutes, all five of the areas added on 2026-09-05 included. Two are blocked rather than unwritten: git
  awareness needs a checkout in the workspace, which the corpus format cannot
  express, and mutation-based coverage needs a verifier that is more than one
  command.
- **Four of the five harness torture tests are written, and three found
  something.** The output bound kept the head and threw away the verdict every
  test runner puts at the end. Search could not report that it had stopped at
  its cap, so a partial list read as a complete one. Compaction preserves six of
  the eight things a resumed run needs. Reading an 8,000-line file was already
  correct. H04 — whether the original requirement survives thirty tool calls —
  is done too: thirty tool calls under a context small enough to force repeated
  compaction, with the requirement asserted on every turn, and the fixture
  proved by reintroducing the historical off-by-one that dropped it.
- **A run records no diagnosis.** Compaction can carry what was read, changed,
  run and refused, and cannot carry *why* — there is no capability that says
  "the defect is here", so a root cause found before a compaction is found again
  after it. Also absent: an edit that turned out wrong is recorded as a change
  like any other. Both are measured gaps, not yet built on: deciding from too
  little is the mistake `multiple_calls` taught.
- **The Fabricated Evidence Rate is built and unmeasured.** A task names the
  symbols the repository does not contain, and a rationale containing one has
  fabricated evidence — deterministic, no judge. `no-retry-exists` is the first
  such task and no campaign has run it.
- **A diagnosis is scored by one substring, over a repository that fits in the
  prompt.** `diagnose-do-not-fix` resolves when the rationale contains `window`
  and no file changed (`TaskOutcome::resolved`, `RepositoryQuestion` arm). Its
  five source files all reach the prompt, and the planted defect carries a
  comment naming itself. On 2026-09-06 the read-only follow-up resolved all six
  trials, one of them after a single action that read nothing, while the same
  task resolved none of six the campaign before. What those six establish is the
  completion contract that had been rejecting correct answers; they do not
  establish that a deployment located the defect. `diagnose-two-calls-away`
  replaces it with thirty-three files, no announcing comment, and an answer that
  must name the file and the function; it resolves six of six as well. Harder,
  and not a discriminator: retrieval ranks the intermediate hop into the prompt
  and that file names the function, so the distance the task claims is one read.
  `diagnose-past-the-delegation` then took the name off the retrieved path, with
  the retrieval reproduced to confirm it, and resolved six of six as well: every
  trial listed the tree and opened `money/convert.rs` and `money/scale.rs`
  directly. The module names are the answer, and a thirty-four-file tree is one
  action to enumerate. What the indirection bought is cost — five to fifteen
  actions against three to seven — not discrimination. The third attempt left
  the synthetic tree: `external-running-min-stability` asks the same question of
  more-itertools at the parent of upstream fix `d992be0`, where the defect is a
  strict comparison in a private helper of a 1,621-line file. Six of six again,
  every trial searching `def running_min` and reading what came back, in three
  to five actions. Resolution is saturated for this cohort across three task
  shapes, so the open item is no longer a harder diagnosis task: it is that this
  area should be scored on cost. The same correct answer cost 33–43 seconds and
  about 700 generated tokens for one deployment, and 168–316 seconds and
  1,800–5,800 for the other two, with peak prompts from 4,628 to 21,226 tokens —
  a spread resolution does not show at all. Two trials per deployment is a
  range, not a ranking.
- **The product path has one measurement behind it.** Every campaign runs
  through `eval`, which is handed its verifier by the corpus. `run` discovers
  the repository's checks, escalates at completion and applies its own timeouts,
  and the whole evidence for it is a single end-to-end run on 2026-09-06: a
  one-line arithmetic defect in a 34-file crate, gpt-oss:20b, 47 seconds, the
  right line changed and the repository's own tests green. Check discovery in
  particular is unmeasured on anything but that. [manual-testing.md](manual-testing.md)
  is the plan for exercising it by hand; findings from it are not rates and do
  not belong in this table.
- **Cost is measured and nothing is judged by it.** Every gated metric is a
  proportion, and on 2026-09-06 that was the wrong instrument: three
  deployments resolved one task at the same rate spending 659 to 5,771
  generated tokens. Reports now carry actions, tokens and seconds over resolved
  and over all measured runs, and `pwr eval compare` pairs two campaigns by
  deployment, task and seed instead of a throwaway script per campaign. No bar
  is attached, because a bar comes from a dated derivation over observed
  numbers, and the numbers do not hold still: priced across three repair tasks
  as well, the cheapest deployment on all three is the one that is second
  dearest on the question, and the spread falls from 6.9× to 1.8–3.3×. A budget
  stated from either alone would be a budget for one task class. What it needs
  before a bar is a cost that has been shown to hold across classes, which is
  three tasks and one question so far.
- **The first cost optimisation was measured and failed.** With resolution
  saturated, `search` was made to return the lines around its first five
  matches, to save the read every trial spent after a search. Six preregistered
  pairs on `external-running-min-stability`: resolution unchanged at six of six,
  generated tokens +31%, wall time +30%, and reads went from ten to eleven. The
  context arrived in every search, covering the lines the defect is on, and the
  deployment read the region anyway. Reverted; the source that was measured is
  commit `4e0b8c3` and the evidence is under `experiments/search-context-20260906`.
  The hypothesis it left — that the instruction, not the tool, asks for the
  read — was then tested over twelve pairs at four seeds, and holds: with the
  read-only prompt saying that lines a search returned are an answer rather than
  a pointer, reads fell from 26 to 16 where the tool alone had moved them from
  10 to 11. It is still not promoted, because the cost it buys is not the same
  for every deployment: generated tokens −26% for qwen3.8:27b-mlx, −1% for
  ornith-1.5:35b, **+59% for gpt-oss:20b**, which answered five searches and two
  reads in ten turns where it had answered in four. That was read at the time as
  the instruction helping the slow, verbose deployments and hurting the fast,
  terse one; the repair baseline since measured says it described who is verbose
  *on questions*, since qwen3.8:27b-mlx is the cheapest of the three on all
  three repairs. So what is open is not per-deployment tuning from those
  numbers, which would tune for a ranking that reverses with the task class. It
  is measuring the instruction where per-deployment prompt text already lives —
  the strategy's `prompt_suffix` — across both classes at once. The
  measured sources are commits `4e0b8c3` and `8d03eb7`; the evidence is under
  `experiments/search-context-20260906` and `experiments/read-instruction-20260906`.
- **Failures are attributed, and the residue is named.** `attribute()` assigns a
  failed run from what it recorded — a broken tool, a budget hit with the checks
  passing, which side of the action channel failed — and what nothing claims is
  `Unattributed` rather than the deployment's. That default is the assumption
  that was wrong five times in two days. What remains unbuilt is emission from
  the retrieval and context side: a search that hit its cap and a compaction
  that dropped something are recorded as facts and never as faults, so they
  cannot yet claim a failure they caused.
- **The framework and language sweeps are blocked on the toolchain question.**
  Running the same task across nineteen backends and thirty languages needs
  thirty toolchains, the sandbox denies network, and `--provision` grants
  network and arbitrary executables together. Blocked until a per-language image
  is pinned or `--provision` is split.
- **The seed does not reproduce a run.** Same corpus revision, harness revision,
  model digest, sampling parameters and seed; different outcome and different
  turn count. The rates survive — a Wilson interval measures a distribution —
  but `corpus_rev + seed` does not identify a result. Measured over eight
  identical invocations: a task that always resolves did so in three, four and
  five turns across a 2.8× duration spread, and a task near the boundary
  resolved six times of eight. Path variance is present where outcome variance
  is not, so every run is a draw and not only the hard ones. Reports now say so.
  Open: the latency rule takes a percentile from as few as five samples, and a
  2.8× spread on identical input makes five visibly too few.
- **The AGENTIC row discriminates, once.** `m6-hard-v1` with four adverse
  conditions resolves 12 of 13. The three that held: someone else's unfinished
  work survived byte for byte, a README promising six priority levels was
  ignored in favour of the three the code knows, and the fabrication task drew
  an honest "there is none". The one that failed introduced an edge-case bug
  while fixing an edge case — `normalise_path("")` returns `"/"` — and did so
  without touching the quarantined test in any of the four ways it was built to
  catch. Over three seeds it is 38 of 39: twelve tasks pass every seed and that
  one passes two of three, failing the second time with a different and worse
  bug — an unconditional pop that breaks `"a/b"`, which the statement plainly
  covers.
- **A statement states its rules; it does not enumerate its inputs.** That line
  had to be drawn by argument this campaign, because a hidden verifier asserted
  a case the statement did not name — the same shape as the `bugfix-parse`
  defect and not the same thing, since there the term itself was undefined and
  here every operation was. `corpus_is_sound` proves a task is solvable and
  cannot prove a statement is complete, and nothing else does either.
- **Both corpora are saturated, and the second premise failed too.**
  `m5-frozen-v1` resolves 24 of 24 against a bar of 0.40. `m6-hard-v1` was then
  built on the axis the evidence pointed at — a twenty-seven-file crate whose
  failing test says nothing about where the defect is, and a library built from
  a specification — and resolves 9 of 9. They cost far more (25 turns for
  `deep-defect` against 2.9 on m5) and cost is not difficulty. Read against the
  ladder even these are MEDIUM and HARD: none has an ambiguous requirement, a
  dirty working tree, a tool that fails, partly failing tests or stale
  documentation. The AGENTIC row is untouched, and it is where the failures are.
- **A threshold of 1.0 can be falsified but never met**, for the same reason a
  threshold of 0 can. A Wilson lower bound is below 1 for any finite run of
  successes, so `hidden verification` and `scope respected` are reported as
  `not falsified` with the bound their clean runs establish. Both rules are
  compiled into the binary from `docs/thresholds.json` rather than applied by
  hand, and the first report to use them agreed with the hand-written verdicts.
- **The hidden-verification falsification was withdrawn, and the withdrawal is
  confirmed.** `bugfix-parse` was rejected twice against a statement that never
  said whether 0 is an out-of-range port while the hidden verifier asserted
  `parse_port("0") == Some(0)`. With the statement fixed it resolved on all
  three seeds, so the defect was the corpus and the measurement says so rather
  than the argument. Results on that task do not compare across the revision.
- **Malformed calls are `multiple_calls` and `no_tool_call`, and nothing else.**
  Measured at 0.120, 0.111 and 0.155 of turns across three campaigns, intervals
  overlapping, so the rate itself is still noise. Every other kind is zero: the
  deployment understands the schemas it was given. Whether the loop should take
  the first of several calls rather than refuse them all cannot be settled from
  a count — three reads of different files and an edit followed by its
  verification are the same number and not the same situation — so the event now
  carries the call names and the prose, and the next campaign decides it.
- **The capability probe overstates the action channel.** It records
  `structured_tools` as reliable from three trials of a trivial call. Three
  trials cannot predict a fifth of real turns being unusable, and the probe's
  own documentation says a unanimous three can report a coin flip as a fact.
- **A campaign before 2026-09-04 cannot be re-examined, only re-run.** Reports
  now carry the failure classes, the malformed kinds and what those turns
  contained, and keep the files behind a rejected completion or an out-of-scope
  change. Everything measured before that is a count with no artefact.
- **Trial count is a stopping condition, not a number.** The interval rule made
  it one: run trials until the interval is decisive. What is still unstated is
  when to stop trying — a metric that stays inconclusive after many campaigns is
  telling you something, and nothing says what.

### Capability

- **A large multi-file build does not finish.** Measured on a fifteen-file PWA:
  the harness improved across three configurations — writes from none to nine,
  reads down by two thirds, the build from not running to producing real
  errors — and the run still did not complete. Whether that is the harness or
  the deployment is answered by running the same task on another of the seven.
- **The loop does not start from a replayed state.** `RunState::replay` rebuilds
  a run's state from its log and `session show` displays it; resuming into it is
  the step left.
- **Linux and Windows have no sandbox adapter**, so on those platforms a run
  either refuses or records that it ran unconfined.
- **`--provision` grants an executable and the network together.** The pair is
  what installs a toolchain and also what an exfiltration is made of. Running it
  in a separate process or VM is the rest of that answer.

### Structural debt

Declared rather than hidden, and none of it blocks an alpha.

- `pwr-orchestrator/src/lib.rs` is past five thousand lines and
  `pwr-cli/src/main.rs` past three and a half thousand. The CLI still holds
  orchestration — hardware probing, profile resolution, prompt construction,
  the evaluation runner — that belongs behind the orchestrator's boundary.
- The event log's chain is per run and verified, but there is no artifact
  table, and migrations are two `execute_batch` calls rather than a mechanism.
- A persisted artifact's `schema_version` is compared, and there is no
  migration path: an older artifact is refused rather than upgraded.

### Work the harness should absorb from the model

The division this project should hold to: the deployment decides semantics — what is wrong, what to change, which fix is right — and the harness does the mechanical work. Several things on the model's side of that line today are mechanical, and each one costs actions from a budget meant for thinking.

Manifest and dependency discovery, finding the call sites and tests related to a file, ranking and de-duplicating what goes into the context, token accounting, selecting which checks a change requires, correlating a diagnostic to a file and a line, generating a diff, counting edits and recovery attempts, classifying a failure, reproducing a flaky one, detecting that a run has stopped progressing, and retrying at a lower context tier — all of these are the runtime's to do, and several are the P1 and P2 rows above under a different name.

Two boundaries are deliberate. **Which semantic correction to make stays the deployment's**, and inferring that a plan step is finished stays out of the harness — the harness recording that work happened would be the harness doing it. **Whether the conclusion is accepted is the harness's**, and that one moved on 2026-09-03: a completion is now refused where nothing can verify it.
