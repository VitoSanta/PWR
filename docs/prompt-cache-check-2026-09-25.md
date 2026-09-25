# Prompt cache check — 2026-09-25

What to run on the Mac to confirm two changes to how PWR keeps the model's
prompt cache, and what to report back. Everything runs locally; no network.

## What changed

1. **Reasoning is no longer dropped when the person writes again.** Since the
   Ornith fix, each step's reasoning was handed back within a turn and cleared
   at the next person message. The sidecar renders with `preserve_thinking`
   (D.E2E-21), so clearing it changed the history part way; for Qwen 3.5/3.6,
   whose cache cannot be cut back, the whole conversation was then prefilled
   again for every message (at ~230 tok/s: ~45 s at 10K tokens, ~3 min at
   40K). It now stays on its step until a compaction folds it away.
2. **The cache is copied only where it has to be.** The sidecar copied the
   whole KV cache after every prefill, to restore it next time. For a model
   whose cache can be cut back (dense models, e.g. Seed-OSS-36B) it is now cut
   back to where the new prompt parts from the last one instead: no copy held
   beside it, and a history changed part way reuses what comes before the
   change. Qwen 3.5/3.6 (linear-attention layers) and vision models keep the
   copy, as before.

With `PWR_MLX_TRACE` set, every request now records `prompt_tokens`,
`cached_tokens`, `prefilled_tokens`, `prefill_s`, `copied_cache` and
`peak_memory_gb`.

## 1. Automated checks

```
cd /Users/vitosantanelli/Desktop/projects/PWR
git pull origin develop
cargo test -p pwr-cli --bin pwr a_new_message_keeps_the_reasoning
cargo test -p pwr-orchestrator --lib
cargo test -p pwr-mlx
"$HOME/Library/Application Support/ai.pwr.desktop/engine/venv/bin/python" -m unittest discover -s crates/pwr-mlx/sidecar
```

Expected: all pass. In the sidecar suite, `TrimmedCache` (2 tests) is new;
`StableHistory` and `FusedAttention` need a model or MLX and may skip.

## 2. Measurement in the app

Run each scenario twice: on `develop` (after) and on commit `2f53651`
(before), same model, same workspace, same messages, a fresh conversation each
time. Quit the app between runs.

```
cd /Users/vitosantanelli/Desktop/projects/PWR/apps/desktop
rm -f /tmp/pwr-trace.jsonl
PWR_MLX_TRACE=/tmp/pwr-trace.jsonl npm run tauri dev
```

**Scenario A — Qwen3.6-35B-A3B (cache is copied).** In a small workspace,
Reasoning Effort on:

1. "Read the main source files and tell me what this project does."
2. When it has answered: "Which function would you change first to add
   logging, and why?"
3. When it has answered: "Show me the change, do not apply it."

**Scenario B — a dense model installed on this Mac** (for example
Seed-OSS-36B, or any model that is not Qwen 3.5/3.6). The same three messages.

After each run:

```
jq -c '{prompt: .prompt_tokens, cached: .cached_tokens, prefilled: .prefilled_tokens, prefill_s: .prefill_s, copied: .copied_cache, peak_gb: .peak_memory_gb}' /tmp/pwr-trace.jsonl
```

## 3. What passes

- **The first request after messages 2 and 3** (the first line after each
  answer): on `develop`, `prefilled` is about the new message plus the last
  answer — hundreds to a few thousand tokens — not the whole `prompt`. On
  `2f53651` with Qwen3.6 it is expected to be the whole prompt
  (`cached` = 0). This is the main result.
- **Steps within a message:** `prefilled` stays small on both commits (it
  already did; this must not regress).
- **Scenario B:** `copied` is `false` on `develop` and `true` on `2f53651`;
  `peak_gb` on `develop` is lower, by up to the size of the KV cache.
- **Scenario A:** `copied` is `true` on both.
- **Answers:** comparable quality on both commits. Report anything that
  reads worse — the model now sees its earlier reasoning on later messages.

## 4. What to report back

For each scenario and commit: the `jq` output, the model name, and the total
time of each message. A failed automated check: the test name and its output.
