# Competitors and runtime sources

**Initial F2 source pass, 2026-10-01.** IMPLEMENTED below means an inspected
upstream path or documented feature; it is not a measured local-agent capability.
This is a partial survey of six projects plus focused primary sources. No competitor was installed or run,
no benchmark claim is adopted, and PWR superiority remains **unknown**.
The lead independently fetched release metadata/licenses and reopened selected
sources after a read-only research subagent's pass.

## Revision and license ledger

Latest stable release metadata was fetched from each upstream repository's
`releases/latest` API on 2026-10-01. This is the API's stable release selection,
not necessarily the newest rolling/prerelease build. License links refer to
the checked release. Docs/current source examined below are a separate snapshot;
do not attribute their features to a stable tag without checking that tag.

| Project | Stable release / published UTC | Opened license |
|---|---|---|
| mini-SWE-agent | [v2.4.6](https://github.com/SWE-agent/mini-swe-agent/releases/tag/v2.4.6), 2026-07-23 03:12:33 | [MIT](https://raw.githubusercontent.com/SWE-agent/mini-swe-agent/v2.4.6/LICENSE.md) |
| Aider | [v0.86.0](https://github.com/Aider-AI/aider/releases/tag/v0.86.0), 2025-08-09 17:42:19 | [Apache-2.0](https://raw.githubusercontent.com/Aider-AI/aider/v0.86.0/LICENSE.txt) |
| OpenCode | [v1.18.34](https://github.com/anomalyco/opencode/releases/tag/v1.18.34), 2026-09-30 22:39:45 | [MIT](https://raw.githubusercontent.com/anomalyco/opencode/v1.18.34/LICENSE) |
| Codex CLI | [rust-v0.159.3](https://github.com/openai/codex/releases/tag/rust-v0.159.3), 2026-09-30 22:57:34 | [Apache-2.0](https://raw.githubusercontent.com/openai/codex/rust-v0.159.3/LICENSE) |
| mlx-lm | [v0.31.3](https://github.com/ml-explore/mlx-lm/releases/tag/v0.31.3), 2026-04-22 07:43:57 | [MIT](https://raw.githubusercontent.com/ml-explore/mlx-lm/v0.31.3/LICENSE) |
| llama.cpp | [v0.5.0](https://github.com/ggml-org/llama.cpp/releases/tag/v0.5.0), 2026-09-23 20:50:06 | [MIT](https://raw.githubusercontent.com/ggml-org/llama.cpp/v0.5.0/LICENSE) |

No upstream code was copied into PWR. Dependency licenses, exact reuse scope
and NOTICE obligations still need review before any reuse; a top-level license
is not a transitive supply-chain audit. Retrieved metadata/source copies are
local evidence under `~/Desktop/pwr-evidence/logs/mission-20261001-f0/upstream/`.

## Architecture and local deployment evidence

**mini-SWE-agent — IMPLEMENTED simple control reference.** Inspected
[agent at `04d809ce`](https://raw.githubusercontent.com/SWE-agent/mini-swe-agent/04d809ceab9df28f9adaed044884180159172930/src/minisweagent/agents/default.py):
model/environment separation, appended messages, pre-query step/cost/wall
limits, environment action execution and serialized trajectories. The
[README](https://github.com/SWE-agent/mini-swe-agent) describes a bash-only
interface, linear history and independent subprocess actions, with local and
container environments. Exact effective limits must be pinned in the evaluation;
the presence of a limit class does not establish that a default enables it.
Same-engine local success, tool token cost and sandbox equivalence to PWR are
**unknown**. Use it as a minimal baseline design, not as an assumed measured
winner on local weights.

**Aider — IMPLEMENTED edit-format and map choices documented.** Official
[edit documentation](https://aider.chat/docs/more/edit-formats.html) describes
whole-file, SEARCH/REPLACE, fenced diff, simplified unified diff and
architect/editor variants. The [repository map](https://aider.chat/docs/repomap.html)
ranks file dependencies/symbols inside a token budget (documented default 1k,
with dynamic expansion). [Compatible endpoint configuration](https://aider.chat/docs/llms/openai-compat.html)
and [Ollama configuration](https://aider.chat/docs/llms/ollama.html) are
documented; matched tool rendering, prompt policy, context/sampling and local
quality remain **unknown** until an actual deployment is tested.

**OpenCode — IMPLEMENTED tools/extensions documented; edit semantics need code
inspection.** [Tools](https://opencode.ai/docs/tools/) include bash, write/edit,
windowed read, grep/glob, patch, skill and todo; LSP is labeled experimental.
Custom tools/MCP and allow/deny/ask configuration are documented. The docs
describe exact edit replacement, while [edit source at `aa481b8f`](https://raw.githubusercontent.com/anomalyco/opencode/aa481b8f5652f5576c55f914a64ed270e7daa7e0/packages/opencode/src/tool/edit.ts)
contains a sequence of exact, line-trimmed, anchor and whitespace-normalized
replacers. [Providers](https://opencode.ai/docs/providers/) describe local
Ollama through a compatible endpoint. Model-specific reliability, unintended
fuzzy edits, effective context and full prompt/compaction paths are **unknown**
in this pass; configuration support is not behavioral verification.

**Codex CLI — IMPLEMENTED local provider configuration documented; wire parity
must be explicit.** The [official CLI reference](https://learn.chatgpt.com/docs/developer-commands?surface=cli)
documents `--oss` and `--local-provider lmstudio|ollama`. The
[configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference)
currently permits only `responses` for `wire_api`. Therefore a server exposing
only Chat Completions cannot be assumed compatible. These mutable official docs
were opened on the retrieval date; the exact prompt, edit, compaction and local
capability of the named stable release were not audited here.

**mlx-lm — IMPLEMENTED runtime facilities documented.** Its
[README](https://github.com/ml-explore/mlx-lm) documents Apple-silicon generation,
prompt caching and rotating KV cache. The pinned
[server at `53b9af37`](https://raw.githubusercontent.com/ml-explore/mlx-lm/53b9af378278d6dd5dacac447edeb0f523b16253/mlx_lm/server.py)
routes POST only to `/v1/completions`, `/v1/chat/completions` and
`/chat/completions` (retrieved source lines 1105–1107), returning 404 for other
paths. Thus Responses is absent in that inspected router. PWR integration
overhead relative to pure mlx-lm is **unknown**, not established by using the
same library. Pure generation must receive the same rendered/tokenized prompt
and sampling for the integration-cost comparison.

**llama.cpp — IMPLEMENTED server facilities documented.** Pinned
[server documentation at `68e79bd8`](https://raw.githubusercontent.com/ggml-org/llama.cpp/68e79bd8cd6b7995f8fce8da252b249bbf237e63/tools/server/README.md)
documents shared-prefix KV reuse, timing metrics and `/v1/responses`, converting
requests to Chat Completions. It warns batch-size-dependent logits can make
cache-enabled output non-bit-identical. Responses transport availability is
evidence to test Codex integration; streaming, tools and cancellation parity
are **unknown** until tested. Actual comparisons must pin builds and state
which weight/quantization equivalences cannot be met across MLX/GGUF formats.

## Initial feature matrix against PWR

`documented` means the linked upstream material above, not a PWR-local run.
PWR entries refer to the [initial code/test audit](../reviews/2026-10-01-audit.md).

| Dimension | PWR at ae1e36c1 + starting diff | mini-SWE-agent | Aider | OpenCode | Codex CLI local |
|---|---|---|---|---|---|
| Local model transport | IMPLEMENTED managed MLX; EXPERIMENTAL per-call llama server | documented pluggable model provider; endpoint parity unknown | documented compatible endpoint/Ollama | documented compatible endpoint/Ollama | documented local providers; Responses required |
| Tool/edit interface | IMPLEMENTED typed calls, hashes, replace/patch/write | documented bash-only | documented multiple edit formats | documented read/edit/search/patch; inspected normalization fallback | exact pinned architecture unknown in this pass |
| Context | IMPLEMENTED approximate accounting, compaction, prefix cache | inspected linear appended history | documented ranked map | exact prompt/compaction behavior unknown in this pass | exact pinned context behavior unknown |
| Extensions | stable MCP/skills boundary PLANNED in new contract | unknown | unknown | documented custom tools/MCP/skill | unknown in this pass |
| Effects | MEASURED local sandbox cases; Docker case skipped | documented container option; equivalence unknown | equivalence unknown | documented permissions; OS isolation unknown | documented sandbox options; matched semantics unknown |
| Same-model controlled capability | unknown | unknown | unknown | unknown | unknown |

Runtime differences are deployment factors, not harness wins. A compatible
endpoint is insufficient: verify streaming, tool call/result IDs, templates,
stop tokens, output cap, reasoning mode, effective sampling and context. Do not
pool a transport adapter intervention with a harness-policy intervention.

## Candidate hypotheses in the detailed plan

All **HYPOTHESIS**, unimplemented/unmeasured as new treatments. Impact and cost
are planning estimates; provisional dev rules are not F6 acceptance thresholds.
The current implementation remains the simpler control.

| Proposed plan item | Control, metric and provisional reject rule | Expected impact / confidence / cost estimate |
|---|---|---|
| W5.7 stable prefix instrumentation/treatment | Same app executor/current cache vs preserved rendering; paired histories, identical prompt semantics/settings. Cached fraction, warm TTFT, peak memory, correctness. Reject if warm median TTFT falls <10%, or correctness/peak memory fails preregistered tolerance | Latency / medium / medium |
| W4.9 capped ranked symbol map | Same executor with/without a fixed 1k-token candidate map; paired dev tasks. Exploration calls, pass@1, total task tokens/time. Screening reject if median exploration falls <10% or task time/tokens rise >5%; no capability claim without power | Exploration / low–medium / medium |
| W2.8 declarative edit-format comparison | Current edit interface vs SEARCH/REPLACE/whole on identical models/tasks; invalid edits, success, emitted tokens. Screening reject if invalid-edit rate falls <20% relatively, success regresses or total time rises >5%; specify zero-denominator handling before runs | Edit reliability / low–medium / medium |

These cutoffs are **HYPOTHESIS** choices to preregister/refine before dev runs,
not measured optimal values. A small screening run cannot prove non-regression;
keep/removal capability decisions need the power and paired-interval protocol.
First priority remains correctness and product/eval parity, not these treatments.

## Context and mode sources reopened during the F1 follow-up

Retrieved 2026-10-01 by the lead after the read-only research pass. These are
source observations; PWR effects remain **unknown** until matched dev runs.
No recipe or paper threshold becomes a PWR default.

| Opened primary source | What it supports | Limit / implication for planned work |
|---|---|---|
| [Lost in the Middle, arXiv v3](https://arxiv.org/html/2307.03172v3), Liu et al., revision 2023-11-20 | MEASURED by the paper: multi-document QA and key-value retrieval can vary with relevant-information position. Some tested models nearly solve the synthetic retrieval case; it is not a universal failure law. | The tested deployments are older than today's local models. PWR effective context is unknown; sample positions and task types in W5.3 rather than importing a universal context ceiling. |
| [RULER, arXiv v3](https://arxiv.org/html/2404.06654v3), Hsieh et al., Table 3 / Appendix B–D | MEASURED by the paper: 13 controlled configurations across retrieval, variable tracing, aggregation and QA. Its effective length is the largest tested length above the Llama2-7B-at-4K reference score, 85.6%. | This is a benchmark-specific score criterion, not physical input capacity or PWR coding/exit thresholds. QA includes SQuAD/HotpotQA; do not describe every task as entirely synthetic. W5.3 needs a prospective local criterion. |
| [Qwen3.6-35B-A3B card at 995ad96e](https://huggingface.co/Qwen/Qwen3.6-35B-A3B/raw/995ad96eacd98c81ed38be0c5b274b04031597b0/README.md), sampling / preserve-thinking sections | DOCUMENTED separate general-thinking, precise-coding-thinking and non-thinking recipes. Historical thinking retention is controlled by preserve_thinking; default retention concerns the latest user exchange. | A card does not prove PWR's active mode. W5.6 must receive effective mode and distinguish unknown mode; W5.7 must compare actual rendered token prefixes. Claimed cache/cost benefits remain unmeasured in PWR. |
| [mlx-lm tokenizer wrapper at 53b9af37](https://raw.githubusercontent.com/ml-explore/mlx-lm/53b9af378278d6dd5dacac447edeb0f523b16253/mlx_lm/tokenizer_utils.py), apply_chat_template | IMPLEMENTED mapping of the thinking keyword for custom renderers; an omitted keyword defaults from tokenizer thinking support. Custom template output is encoded without adding special tokens. | Rendering settings and tokenization are part of provenance. Compare the same token IDs for pure/runtime baselines; API labels or identical message strings do not establish prefix identity. |

These observations support the **PLANNED** controls in W5.3/W5.6/W5.7; they do
not demonstrate that today's compaction ceiling or preserved reasoning improves
coding. The context fixes above are justified by deterministic error-path and
budget regressions, independently of model capability claims.

## Outstanding F2 work

Claude Code, Cursor, Goose, Cline/Roo/Kilo, Continue, OpenHands, Crush, Qwen Code,
LM Studio and Ollama architecture/version/license/local limitations remain
**unknown in this pass**. So do issue reproduction, public benchmark provenance,
recent additional competitors and full current prompts/context paths. Complete
their rows from opened primary sources, not remembered feature lists.

The mandate's primary-literature list (agent interfaces; benchmark contamination;
constrained decoding; long context; speculative decoding/prefix cache/KV
quantization/roofline; selection/verification; statistical estimators; MCP/ACP
and AGENTS.md) remains **PLANNED verification work** beyond the opened long-context
sources above. No unverified paper assertion
is imported here. The preexisting Semantic Decision Layer proposal remains a
hypothesis; its citations still require independent verification before reuse.
