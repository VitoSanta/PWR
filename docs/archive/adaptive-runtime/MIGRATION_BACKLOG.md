> **Historical record, superseded as current design on 2026-09-12.** The body is kept as evidence of the design it records; it is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md).

# Migration backlog

| ID | Priority | Task | Depends | Areas | AC/tests | Size | Status |
|---|---|---|---|---|---|---|---|
| AR-001 | P0 | Freeze request/event/corpus fixtures | - | CLI, eval | reproducible baseline | M | done — fixtures and corpus frozen |
| AR-002 | P0 | Extract CLI runtime construction | 001 | CLI | no direct provider construction in command handlers; fixture parity | L | done — `pwr-runtime` composition root |
| AR-003 | P0 | Define backend capability/discovery contracts | 002 | provider/domain | fake + Ollama contract tests | M | done — `InferenceBackend` |
| AR-004 | P0 | Implement Ollama backend adapter to new contracts | 003 | ollama | streaming/cancel/inspect parity | M | done — Ollama and LM Studio |
| AR-005 | P1 | Version/split model registry evidence schema | 001 | domain, strategies | provenance/schema migration tests | L | done — versioned registry, provenance required |
| AR-006 | P1 | Define typed canonical tools/request/response | 005 | domain | serializer/parser properties | L | done — `ToolCatalog`, canonical reply |
| AR-007 | P1 | Generic compatibility adapter | 006 | adapter | malformed/action normalization fixtures | M | done — `GenericAdapter` |
| AR-008 | P1 | Qwen family adapter and profiles | 006 | adapter, registry | legacy request parity | M | done — `QwenFamilyAdapter` |
| AR-009 | P1 | Extract task profile and compatibility execution policy | 006 | orchestrator | legacy policy fixture | L | done — `TaskProfile` |
| AR-010 | P1 | Move context composition into context engine | 009 | CLI/orchestrator | compaction fidelity tests | M | done — `orchestrator::context` |
| AR-011 | P2 | Cross-platform profiler/resource budget | 003 | runtime | platform fixture matrix | L | done — `runtime::hardware`, `ResourceBudget` |
| AR-012 | P2 | Explainable Auto selector | 005,009,011 | runtime/CLI | reject/fallback tests | L | done — `models select`, `run --model auto` |
| AR-013 | P2 | Certification registry/report | 004,005 | eval/store | promotion/demotion tests | L | done — `models certify` / `certification` |
| AR-014 | P3 | Granite proof adapter | 008,013 | adapter | core untouched assertion | M | done — `GraniteFamilyAdapter` |

## Deviations from the plan as written

`--profile auto` was specified as the routing switch, but `--profile` already
names a calibration artifact on `run`, and overloading it would have made one
flag mean two unrelated things. Routing is `--model auto` instead, with
`pwr models select` as the read-only surface that explains a decision
without taking one. Explicit choice precedence is unchanged: an explicit
`--model` is never substituted, though the selector may still reject it and
say why.

Routing requires a calibration for the deployment it picks. Admission alone is
not enough to start a run: the execution profile is derived from measured
stable context tiers, so a candidate with no measurement is refused by name
rather than run at a size nobody has verified.

Certification cannot be scoped to a backend that publishes no version. Ollama
answers `/api/version`; LM Studio answers none of `/api/version`, `/v1/version`
or `/api/v0/version`, so a result measured there is refused certification
rather than attributed to an unknown build.
