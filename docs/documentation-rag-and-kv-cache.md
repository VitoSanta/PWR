# Documentation RAG and KV-cache quantization

**Research proposal, discussed with the maintainer on 2026-09-27.** This note
records an analysis of the current code and existing experiment reports. It
does not introduce an implementation, a new benchmark result or a roadmap
priority change. Track it as C.22c in [the backlog](backlog.md), alongside
C.22 (document retrieval) and C.22b (evidence after compaction).

## Intended use

Give PWR a versioned set of documentation and examples so it can build a new
.NET / Angular / Docker application, including multiple services, while
retrieving the evidence relevant to each implementation step. Investigate
whether better evidence selection, durable task state and a smaller KV cache
allow local models to complete longer tasks within the machine's memory.

Success means verified repository work, not simply more available tokens.
Retrieval supplies knowledge; it does not establish a model's ability to
design service boundaries, reconcile contracts or debug a distributed system.

## Existing foundations and evidence

| Area | Current implementation | Boundary |
|---|---|---|
| Repository retrieval | `pwr-repo::retrieve_with`: symbols, paths, terms, shallow import/test neighbours; Markdown sections ranked with BM25 | Shallow relationships are not a complete semantic code graph |
| Semantic ranking | `SectionRanker`, local `multilingual-e5-small` sidecar, reciprocal-rank fusion, content-hash cache | Experimental, enabled with `PWR_SEMANTIC_RETRIEVAL=1`; unavailable encoder falls back to lexical ranking |
| Context composition | Typed sections in `pwr-orchestrator/src/context.rs`; default five retrieval excerpts and one sixth of the supplied context budget | Code and documents share excerpt slots; estimates are not exact tokenization |
| Compaction | Mechanical record in `compaction.rs`, preserving bounded requests, actions, changed-file state, failures and checks | First request bounded to 800 characters; later requests and records also bounded; details can be lost |
| Working window | `window.rs` uses model shape, weights, memory and prefill estimates | Cache size generally assumes two-byte elements; this is not measured quantized-cache allocation |
| Inference | MLX sidecar with custom prefill, cache reuse, trimming and checkpoints; experimental llama.cpp adapter | Examined paths do not expose a KV-quantization configuration |

Source entry points:
[retrieval](../crates/pwr-repo/src/lib.rs),
[embedding ranker](../crates/pwr-cli/src/semantic.rs),
[encoder](../crates/pwr-mlx/sidecar/pwr_embed.py),
[context compiler](../crates/pwr-orchestrator/src/context.rs),
[compaction](../crates/pwr-orchestrator/src/compaction.rs),
[window calculation](../crates/pwr-orchestrator/src/window.rs),
[MLX sidecar](../crates/pwr-mlx/sidecar/pwr_mlx.py),
[llama.cpp adapter](../crates/pwr-llama/src/lib.rs).

C.22 records a modest retrieval gain, not a demonstrated general coding gain.
The isolated fusion experiment reported P@3/R@3 of 0.28/0.22. The product
comparison over twelve requests reported P@3 0.19 to 0.24, recall 0.16 to
0.17 and 11% fewer delivered tokens; these are different experimental
conditions. In the subsequent small-model FAQ task, both arms verified
(one run each). Semantic retrieval used fewer tokens and actions but more
wall time. These observations do not establish faster or more reliable
multi-service development.

## Proposed documentation layer

Start with a local reusable module, imported Markdown and persistent metadata.
A separate vector service is not a prerequisite; consider one only when
corpus size or sharing requirements justify its operational cost.

Separate three kinds of input:

- **Project requirements and decisions:** chosen versions, service boundaries,
  data ownership, API contracts, conventions and acceptance criteria. Preserve
  the original requests and revisions durably; keep mandatory constraints in
  the active task state rather than leaving their presence to similarity rank.
- **Reference documentation:** sources tied to the selected framework/library
  versions, with origin, acquisition date, content hash and section identity.
- **Verified examples:** small templates or examples with reproducible builds
  and checks. Record which versions and checks were actually validated.

Import, normalize and index the references once, then combine lexical and
semantic retrieval. Preserve heading ancestry and source metadata with each
fragment. Return inspectable references and provide access to the full source.
Documentation is evidence, not authority to change tool permissions or task
scope; imported instructions must not override the user's requirements.

Expose retrieval during the task, for example through proposed `search_docs`
and `read_doc` tools. The current step, relevant files and actual diagnostics
should guide searches: the original request to "build a microservices app"
cannot identify all evidence needed later. Use bounded retrieval to avoid
repeated searches filling the context with redundant passages.

### Gaps to address first

1. The encoder truncates inputs at 512 tokens while it receives whole Markdown
   sections. Important text near the end may never affect the embedding.
   Delivered excerpts are separately bounded by lines. Split on meaningful
   boundaries within the encoder's token budget, keep code examples intact
   where possible, and make truncation and continuation explicit.
2. Add source/version filters and conflict handling. A relevant passage from
   the wrong framework version can be worse than no passage.
3. Budget requirements, repository code and external references separately;
   tune allocation against actual tasks rather than increasing top-k blindly.
4. Extend embedding-cache identity beyond model name and text hash to include
   model revision, tokenizer, pooling and chunking version.
5. Measure cold indexing/loading and warm retrieval separately. Cached vectors
   do not eliminate reading/ranking work over a growing corpus; the current
   JSON cache and exhaustive comparisons may need replacement at scale.
6. Confirm integration in the actual desktop/conversation path and the
   evaluation path. Existing conversation/runner differences must not turn a
   benchmark-only improvement into a product claim.

Expected benefits are fewer version errors, unsupported API guesses and whole
document reads. Costs include indexing, encoder memory, latency, stale-source
handling and retrieval misses. More retrieved text can also distract the model
and increase prefill work. Keep stable prompt prefixes where possible to avoid
unnecessary cache invalidation.

## Different ways to increase usable context

| Technique | What it changes | What it does not guarantee |
|---|---|---|
| RAG | Amount of knowledge accessible outside the prompt | All knowledge simultaneously available to attention |
| Compaction / text compression | Information represented in fewer prompt tokens | Lossless preservation of requirements or exact syntax |
| KV-cache quantization | Memory used by cached attention keys and values | A larger native window or unchanged quality |
| Weight quantization | Memory and numerical precision of model weights | The same capabilities or a smaller KV cache |
| Native-window extension | Supported sequence length through model-specific techniques | Reliable reasoning beyond the validated length |

For standard growing attention, cache memory is approximately proportional to
tokens × layers × KV heads × (key dimension + value dimension) × bytes per
element. Hybrid, sliding-window and latent-attention architectures need their
own accounting.

Moving quantizable cache elements from 16 to 8 bits roughly halves that
component; 4 bits roughly quarters it. Scales, metadata, unquantized state,
weights and transient allocations remain. This is not a twofold or fourfold
reduction in total process memory. A larger window helps only when memory was
the binding constraint and the model/backend support that length.

### Proposed KV integration

Upstream MLX-LM exposes `kv_bits`, `kv_group_size` and
`quantized_kv_start`; llama.cpp exposes K/V cache-type options. Verify these
against PWR's pinned dependency versions and supported model architectures
before implementation; upstream support is not evidence of PWR integration.

PWR prefills through its own model calls before `stream_generate`. Passing a
quantization option only to generation could leave the initial prefill peak
unchanged. Integrate cache conversion in the prefill lifecycle, and verify
reuse, trimming, snapshots/restoration and cancellation, including models
whose caches cannot be trimmed.

Report the actual cache configuration through the provider and account for it
in `window.rs`. Do not scale prefill scratch-memory estimates by the same
factor as the KV cache: attention kernels and chunk sizes have separate costs.
Include backend version, model identity, weight and cache formats, group size
and conversion settings in experiment identity and compatibility evidence.

Begin with an opt-in 8-bit experiment; consider 4-bit only per validated
deployment. Unsupported configurations need an explicit result or reported
fallback, never a silent claim that the requested precision is active.
Keep native-length bounds unless window extension is a separate experiment.

## Durable state before more aggressive compression

Preserve full requirements and revisions outside the bounded compaction record,
with a compact active contract and links back to their sources. Keep decisions,
open failures and verification state traceable. Store bulky evidence so it can
be recovered; validate file hashes or re-read before delivering it as current.

C.22b already proposes comparing current compaction, recency fill and
encoder-ranked evidence at equal delivered-token budgets. Extend its tasks
with multi-service requirements and changes made after earlier compactions.
An encoder ranks evidence; low similarity alone must not erase a requirement.

Token-level text compression is a later experiment on prose. For code, API
signatures and configuration, first prefer selection of intact passages:
removing a small syntactic detail can change behaviour.

## Evaluation sequence and decision gates

Keep the roadmap's existing runtime-consolidation dependencies. This proposal
does not start a campaign or change their order.

1. Freeze a baseline with pinned stack versions, model/backend settings and
   protected acceptance tests for .NET / Angular / Docker tasks. Include
   service contracts and end-to-end behaviour: a valid Compose file alone
   does not establish a working system.
2. Compare current retrieval, manually selected relevant documentation as a
   diagnostic control, and hybrid document retrieval with comparable context
   budgets. This separates evidence-selection failure from model limitations.
3. Compare memory policies on long tasks with requirement revisions, multiple
   compactions and external edits, following C.22b.
4. Compare cache precision at the same window and weight quantization first;
   then compare feasible windows under the same memory budget. Do not confound
   cache precision, weight precision and context length in one treatment.
5. Evaluate the combined configuration only after the individual contributions
   are understood. Repeat paired tasks with fixed settings/seeds where
   supported and use held-out tasks for confirmation.

Measure verified completion, requirement violations, wrong-version/source use,
stale evidence, post-compaction re-reads, tool actions, delivered and generated
tokens, time to first token, total wall time, indexing/loading cost, cache
reuse, peak memory and memory failures. Retrieval recall is a diagnostic, not
the primary completion metric. Check meaningful multi-file reasoning and
tool use, not only needle recall or perplexity.

Before confirmation, preregister sample size, completion noninferiority margin
and minimum useful efficiency gain using [evaluation.md](evaluation.md).
Promote a configuration only with adequate evidence of acceptable completion
and requirement retention plus the claimed resource benefit. An ambiguous or
underpowered result stays experimental. Retain the simpler baseline when the
extra machinery does not earn its cost.

## External references

- [KIVI](https://arxiv.org/abs/2402.02750) studies asymmetric KV quantization;
  its results do not establish compatibility or quality for every deployment.
- [LLMLingua-2](https://arxiv.org/abs/2403.12968) is a reference for a separate
  prompt-compression experiment, not a guarantee of exact code preservation.
- [MLX-LM generation source](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/generate.py)
  and [llama.cpp server options](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)
  document upstream capabilities. These moving references must be pinned to
  the tested revisions when an implementation or experiment is prepared.
