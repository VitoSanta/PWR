> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../../README.md) (status), the roadmap's latest update ([`roadmap.md`](../roadmap.md)), the backlog's *At a glance* ([`backlog.md`](../backlog.md)), [`current-cli.md`](../current-cli.md), [`pwr-serve.md`](../pwr-serve.md) and [SECURITY.md](../../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Hardware, budget and tiers

`HardwareProfile` describes facts: OS/architecture, CPU, total/available RAM,
accelerators, per-device VRAM or unified memory, Metal/CUDA/ROCm availability,
storage and probe gaps. `ResourceBudget` is policy: OS reservation, PWR
memory allowance, model/context CPU/GPU allowance and concurrency. Keep them
separate; `probe_hardware` is CURRENT but macOS-only and lacks most facts.

Calibration runs after discovery: detect local backends and models, calculate a
conservative budget, warm/probe candidates, measure context and rate, then
write a versioned device profile. It is optional for discovery but required for
high-confidence automatic admission.

Lite, Standard, Pro, Max and Ultra are UX labels generated from feasible
measured capability (context, speed, reliability and policy), never RAM or
parameters alone. A model tier is a registry label; performance profile is
user intent (`Auto`, `Fast`, `Balanced`, `Quality`, `Max Quality`); neither
overrides a user-specified model/backend/context. Explain exclusions with
required/available budget and alternatives.

## What is implemented (AR-011)

`pwr_runtime::hardware::probe_hardware` replaces the macOS-only probe that
lived in the CLI. macOS reads `uname`/`sysctl`/`df`, Linux reads
`/proc/cpuinfo`, `/proc/meminfo`, `df` and `nvidia-smi` where present, and
Windows reads one `Get-CimInstance` batch. `HostMemoryProbe` reports pressure
from `memory_pressure -Q`, `MemAvailable` or `Win32_OperatingSystem`
respectively.

Absence is never a default. A fact that could not be read names itself in
`unavailable_fields`, and an accelerator list is only reported as observed when
the probe actually ran -- a host with no `nvidia-smi` may still have a GPU this
build cannot see. `ResourceBudget::derive` refuses outright when memory was
never observed, so an unreadable machine cannot be admitted as an empty one.

The macOS field spellings are deliberately unchanged (`uname -s` reports
`Darwin`, not `macos`) because `compatibility_key` is hashed from them and
every calibration on disk is matched by that key. Tidier spellings would have
silently invalidated every measurement taken so far.

Tier labels (Lite, Standard, Pro, Max, Ultra) remain unimplemented: they are UX
generated from measured capability, and nothing is measured until a
certification suite has run.

## Context windows are not uniformly controllable (measured)

`ModelProvider::prepare_context` asks a backend to serve one context window
and returns the window actually in force, because those two can differ. Ollama
takes `num_ctx` per request and honours it. LM Studio 0.4.x does not: the chat
API has no per-request equivalent, and `/api/v1/models/load` accepts
`context_length`, answers `"status": "loaded"`, and brings the instance up at
whatever the model's saved settings say -- measured twice, asking for 16384 and
for 4096 against an artifact that came up at 262144 both times.

The consequence is why the contract returns a number instead of `()`. A
calibration ladder run against such a backend without checking produces stable
points labelled 4096, 8192 and 32768 that were every one of them measured at
262144: the numbers look like a ladder and are one measurement repeated. A tier
that was not granted is now rejected as `context_window_not_granted`, carrying
the window that was served instead, and is never sampled.

A deployment shown once not to honour the request is remembered, so a ladder
does not unload and reload the model for several seconds a tier to arrive at
the same window every time.
