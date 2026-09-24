# Sidecar smoke test — 2026-09-18

Qwen3.6-35B-A3B (MLX 4-bit), the A.17 prompt, temperature 0.6, top-p 0.95,
top-k 20, seed 1, cap 12,000 tokens. LM Studio unloaded first.

| Run | Seconds | Finish | Completion | Reasoning | Answer | Right mechanism |
|---|---:|---|---:|---:|---:|---|
| load | 2.4-4.9 | | | | | |
| reasoning off | 40-41 | stop | 2,828 | 0 | 10,227 chars | yes |
| reasoning on, budget 2,000, no loop guard | 175.4 | length | 12,000 | 2,000 | 36,316 chars | yes, then looped: one code block 63 times |
| reasoning on, budget 2,000, loop guard | 67.8 | repetition | 4,864 | 2,000 | 10,351 chars | yes |
| turn 2, reusing turn 1's checkpoint | 6.0 | stop | 16 | 0 | one sentence | -- |

- **Reasoning off works where LM Studio's endpoint ignored it**, and it was
  faster than LM Studio with reasoning off (53-74 s on the same prompt).
- **A reasoning budget is worse than no reasoning for this model and task**:
  cut off mid-thought, it carries on reasoning in the answer and loops. The
  loop guard stops that; it does not make the budget a good default.
- **Prompt reuse works**: turn 2 prefilled only the new tokens (2,883 of
  4,451). Prefill runs at about 535-625 tokens/s on prompts this short, the
  same order as LM Studio's MLX path at 4k in the engine spike (about 680);
  larger prefill steps did not change it.
