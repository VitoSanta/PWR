#!/usr/bin/env python3
"""PWR's MLX engine sidecar: one model, driven over JSON lines on stdio.

Why it exists (backlog A.17, roadmap step 4): the reasoning of a model like
Qwen3.6 is switched by its chat template, which pre-fills an empty think block
when `enable_thinking` is false. Only a caller that renders the template itself
controls the reasoning -- switching it off, or giving it a real budget. LM
Studio renders the template for PWR and ignores the request, so runaway
reasoning confounded every measurement taken through it.

This process renders the template, generates with mlx-lm, and reuses the
prompt cache across turns. It does nothing else: tool calls come back as the
model wrote them, and PWR's family adapters read them, as they do for every
other backend. Policy, sandbox and audit stay in PWR.

Protocol. One JSON object per line in each direction.

  -> {"id": 1, "op": "load", "path": "/path/to/model"}
  <- {"id": 1, "event": "loaded", "model_type": "...", "weights_bytes": N, "vision": bool}

A message's content is a string, or a list of parts: {"type": "text", "text":
"..."} and {"type": "image", "path": "/abs/file.png"} or {"type": "image",
"data": "<base64>"}. Images are accepted only by a model loaded with its
vision encoder ("vision": true on load), which is one load of the weights
through mlx-vlm driven by the same generation loop as text (backlog C.25).

  -> {"id": 2, "op": "chat", "messages": [...], "tools": [...] | null,
      "thinking": true | false | null, "reasoning_budget": N | null,
      "reasoning_effort": "low" | "medium" | "high" | null,
      "max_tokens": N, "temperature": t, "top_p": p, "top_k": k, "seed": s}
  <- {"id": 2, "event": "delta", "channel": "reasoning" | "content", "text": "..."}
  <- {"id": 2, "event": "finalizing", "reasoning_tokens": N}   (budget reached)
  <- ...
  <- {"id": 2, "event": "done", "finish_reason": "stop" | "length" | "repetition" | "cancelled"
      | "reasoning_unfinished", "budget_forced": bool, "reasoning_tracked": bool,
      "usage": {...}, "timings": {...}}

`reasoning_budget` is enforced only when the chat template uses a thinking
delimiter pair the engine tracks (THINK_DELIMITERS): after that many tokens
inside the block the engine appends the closing delimiter, once, and lets the
answer follow within `max_tokens`. If the model reopens its reasoning or ends
without an answer, the request finishes `reasoning_unfinished`. A template
that reads `thinking_budget` is also told the budget; one that reads
`reasoning_effort` is given the level.

  -> {"id": 3, "op": "attention", "head_dim": 256}
  <- {"id": 3, "event": "attention", "fused": false, "scores_bytes": N}

  -> {"id": 4, "op": "cancel", "target": 2}   (while request 2 generates)
  <- {"id": 2, "event": "done", "finish_reason": "cancelled", ...}

  -> {"id": 5, "op": "shutdown"}

Any failure answers {"id": ..., "event": "error", "message": "..."}.
"""
from __future__ import annotations

import json
import os
import pathlib
import queue
import sys
import threading
import time
import traceback

import base64
import hashlib
import io
from collections import deque

# Models are local folders PWR has already checked. The Hub is never asked
# for anything from here: a path that is not there fails, it is not fetched.
os.environ.setdefault("HF_HUB_OFFLINE", "1")

import mlx.core as mx
import mlx.nn as nn
from mlx_lm import load, stream_generate
from mlx_lm.models.cache import can_trim_prompt_cache, make_prompt_cache, trim_prompt_cache
from mlx_lm.sample_utils import make_logits_processors, make_sampler
from mlx_lm.utils import load_tokenizer

# Larger steps, and the allocator cleared once rather than after every step:
# the engine spike found both choices slowed its in-process prefill well below
# LM Studio's on the same MLX build.
PREFILL_STEP = 8192
PREFILL_MIN_STEP = 256
# Attention scores are held in the activation dtype (bf16 or fp16).
SCORE_BYTES = 2
# A generation that keeps writing the same passage is stopped rather than left
# to run to its cap. Seen 2026-09-18: after a reasoning budget closed the think
# block, Qwen3.6 continued reasoning in the answer and wrote one code block
# sixty-three times. Output ending in this many back-to-back copies of one
# passage at least this long is a loop, not an answer; see `looping`.
REPEAT_SPAN = 200
REPEAT_LIMIT = 4
# The thinking-block delimiters the engine tracks, most specific first. The
# pair a model uses is read from its own chat template (`think_delimiters`);
# pwr-domain's `TemplateReasoning::KNOWN_DELIMITERS` carries the same list.
THINK_DELIMITERS = (("<seed:think>", "</seed:think>"), ("<think>", "</think>"))
# How a thinking phase that reached its budget is ended: the template's own
# closing delimiter, then a blank line. Nothing else is injected -- no
# model-family prose -- and only for a template that uses the delimiter.
CLOSE_SUFFIX = "\n\n"
# The engine closes a thinking phase at most this many times per request. If
# the answer does not follow, the request ends as `reasoning_unfinished`;
# whether to ask again is PWR's decision, bounded there.
FINALIZATION_ATTEMPTS = 1
# A backstop under PWR's own reserve: whatever budget a caller sends, this
# many tokens of the request's cap stay for the answer.
FINAL_RESERVE = 512
# A template with a native budget is told it in these steps, so a budget that
# moves by a few tokens as the context fills does not re-render the system
# prompt and miss the prompt cache. The engine still enforces the exact one.
NATIVE_BUDGET_STEP = 512


# A model folder's own Python (custom tokenizers, processors) is never run:
# said explicitly rather than left to a library default.
NO_REMOTE_CODE = {"trust_remote_code": False}

# When set, every chat's last messages and the model's raw output are appended
# here as JSON lines: the one place the model's own text exists before PWR
# reads calls out of it, which is what a misread call has to be checked against.
TRACE = os.environ.get("PWR_MLX_TRACE")


def vlm_available() -> bool:
    try:
        import mlx_vlm  # noqa: F401
        import PIL  # noqa: F401
        return True
    except ImportError:
        return False


def trace(record: dict) -> None:
    if TRACE:
        record = {"at_ms": int(time.time() * 1000), **record}
        with open(TRACE, "a") as handle:
            handle.write(json.dumps(record) + "\n")


def looping(text: str) -> bool:
    """Whether the end of `text` is one passage repeated back to back.

    A loop repeats itself contiguously, so its output turns periodic. The
    first guard only counted how often the last REPEAT_SPAN characters had
    appeared anywhere, and that stopped legitimate code: seen 2026-09-18, a
    model rewriting `idna/codec.py` whole, where one method's signature and
    guard clause genuinely appear four times in different classes, was cut
    mid-file in two runs out of two.
    """
    if len(text) < REPEAT_SPAN * REPEAT_LIMIT:
        return False
    tail = text[-REPEAT_SPAN:]
    previous = text.rfind(tail, 0, len(text) - REPEAT_SPAN)
    if previous < 0:
        return False
    period = len(text) - REPEAT_SPAN - previous
    if period * REPEAT_LIMIT > len(text):
        return False
    return text[-period * REPEAT_LIMIT:] == text[-period:] * REPEAT_LIMIT


class RepetitionSignals:
    """Small, content-free diagnostics for repeated generated token windows.

    This is observational: repeated code and tables must not become a new
    stop condition without a measured false-positive rate. Only aggregate
    counts leave the sidecar; token text and window hashes stay in memory.
    """

    WINDOW = 8

    def __init__(self):
        self.recent = {channel: deque(maxlen=self.WINDOW)
                       for channel in ("reasoning", "answer")}
        self.seen = {channel: set() for channel in self.recent}
        self.total = {channel: 0 for channel in self.recent}
        self.repeated = {channel: 0 for channel in self.recent}

    def feed(self, channel: str, token_text: str) -> None:
        if not token_text:
            return
        recent = self.recent[channel]
        recent.append(hashlib.blake2b(token_text.encode("utf-8"), digest_size=8).digest())
        if len(recent) < self.WINDOW:
            return
        window = tuple(recent)
        self.total[channel] += 1
        if window in self.seen[channel]:
            self.repeated[channel] += 1
        self.seen[channel].add(window)

    def summary(self) -> dict:
        return {channel: {
            "windows": self.total[channel],
            "repeated_windows": self.repeated[channel],
            "ratio_bps": 10_000 * self.repeated[channel] // self.total[channel]
                         if self.total[channel] else 0,
        } for channel in self.recent}


def think_delimiters(template: str):
    """The delimiter pair this chat template uses, or None."""
    for open_tag, close_tag in THINK_DELIMITERS:
        if open_tag in template and close_tag in template:
            return open_tag, close_tag
    return None


class ReasoningStream:
    """Splits generated text into reasoning and answer as it arrives.

    Pure, so it is tested without a model. Fed one generated token's text at a
    time; returns ("reasoning" | "content", text) pieces to stream. A token is
    counted as reasoning when it arrives inside a thinking block, so the count
    is in the model's own tokens, never characters.

    Text is held back in two places, and only there: the tail of a thinking
    block, long enough to catch a closing delimiter split across tokens; and
    the start of an answer, until it cannot be the opening delimiter -- a
    model may open its own block (Qwen3-14B does) rather than the template.
    """

    HOLD = 256

    def __init__(self, delimiters, starts_inside: bool) -> None:
        self.open_tag, self.close_tag = delimiters or (None, None)
        self.in_reasoning = bool(delimiters) and starts_inside
        self.pending = ""
        self.lead = ""
        self.answer_started = False
        self.reasoning_tokens = 0
        self.answer_chars = 0
        self.closed_by_budget = False
        self.reopened = False

    @property
    def answered(self) -> bool:
        return self.answer_chars > 0

    def feed(self, text: str) -> list:
        if self.in_reasoning:
            self.reasoning_tokens += 1
        events = []
        while text and not self.reopened:
            if self.in_reasoning:
                self.pending += text
                text = ""
                if self.close_tag in self.pending:
                    before, text = self.pending.split(self.close_tag, 1)
                    if before:
                        events.append(("reasoning", before))
                    self.pending = ""
                    self.in_reasoning = False
                elif len(self.pending) > self.HOLD:
                    keep = len(self.close_tag)
                    events.append(("reasoning", self.pending[:-keep]))
                    self.pending = self.pending[-keep:]
            elif self.open_tag and not self.answer_started:
                self.lead += text
                text = ""
                stripped = self.lead.lstrip()
                if stripped.startswith(self.open_tag):
                    self.lead = ""
                    if self.closed_by_budget:
                        # The phase PWR closed has been reopened: the
                        # transition failed, and generating on would only
                        # spend the answer's reserve on more reasoning.
                        self.reopened = True
                        break
                    self.in_reasoning = True
                    text = stripped[len(self.open_tag):]
                elif not self.open_tag.startswith(stripped):
                    self.answer_started = True
                    self.answer_chars += len(stripped)
                    events.append(("content", self.lead))
                    self.lead = ""
            else:
                self.answer_chars += len(text.strip())
                events.append(("content", text))
                text = ""
        return events

    def force_close(self) -> list:
        """Ends the thinking phase at its budget; returns what was held."""
        events = [("reasoning", self.pending)] if self.pending else []
        self.pending = ""
        self.in_reasoning = False
        self.closed_by_budget = True
        return events

    def flush(self) -> list:
        """What is still held when generation stops."""
        events = []
        if self.pending:
            events.append(("reasoning" if self.in_reasoning else "content", self.pending))
            self.pending = ""
        if self.lead.strip():
            self.answer_chars += len(self.lead.strip())
            events.append(("content", self.lead))
        self.lead = ""
        return events


def fused_attention(head_dim: int) -> bool:
    """Whether MLX's attention for a prefill step keeps its scores on chip.

    Measured on MLX 0.32 (2026-09-19): head dimensions 64, 80 and 128 run
    fused and need no score buffer; 96, 112, 160, 192 and 256 -- Qwen3.5/3.6/
    3.8's is 256 -- materialise every score. Asked of MLX here rather than
    listed, so an MLX that fuses more dimensions is used as it is.
    """
    heads, kv, queries, keys = 4, 1, 512, 8192
    q = mx.zeros((1, heads, queries, head_dim), dtype=mx.bfloat16)
    k = mx.zeros((1, kv, keys, head_dim), dtype=mx.bfloat16)
    mx.eval(q, k)
    mx.reset_peak_memory()
    base = mx.get_active_memory()
    mx.eval(mx.fast.scaled_dot_product_attention(q, k, k, scale=1.0, mask="causal"))
    scores = heads * queries * keys * SCORE_BYTES
    return mx.get_peak_memory() - base < scores // 4


# How long a sidecar whose input has closed may take to finish on its own.
EXIT_GRACE_SECS = 10.0


class Inbox:
    """Requests from PWR, read on a thread so a `cancel` is seen while a
    generation is running rather than after it. Before this, a reply PWR
    had abandoned ran to its cap -- the capability probe's count to 500, a
    runaway answer -- and the next request waited behind it."""

    def __init__(self, stream) -> None:
        self.lines: queue.Queue = queue.Queue()
        self.cancelled: set = set()
        self.held: list = []
        threading.Thread(target=self._read, args=(stream,), daemon=True).start()

    def _read(self, stream) -> None:
        for line in stream:
            self.lines.put(line)
        self.lines.put(None)
        # The input ended: PWR is gone. The main thread normally finishes
        # and returns, but one blocked inside an MLX evaluation never reads
        # this -- measured 2026-09-22, a sidecar stuck in `mlx::core::eval`
        # outlived the app and its core by minutes, orphaned, holding the
        # model's memory. So the process ends itself after a grace period
        # whatever the main thread is doing.
        threading.Timer(EXIT_GRACE_SECS, os._exit, args=(0,)).start()

    def _sort(self, line) -> None:
        # A cancel is recorded, never queued: it is about a request already
        # running, and answering it in turn would come after that request.
        try:
            request = json.loads(line) if line and line.strip() else None
        except ValueError:
            request = None
        if request and request.get("op") == "cancel":
            self.cancelled.add(request.get("target"))
        else:
            self.held.append(line)

    def is_cancelled(self, ident) -> bool:
        while True:
            try:
                line = self.lines.get_nowait()
            except queue.Empty:
                return ident in self.cancelled
            if line is None:
                # The input ended: nothing more will be asked, but what is
                # running was not cancelled -- finish it, then stop.
                self.held.append(None)
                return ident in self.cancelled
            self._sort(line)

    def next(self):
        while True:
            if self.held:
                return self.held.pop(0)
            line = self.lines.get()
            if line is None:
                return None
            self._sort(line)


def emit(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def common_prefix(a: list[int], b: list[int]) -> int:
    n = 0
    for x, y in zip(a, b):
        if x != y:
            break
        n += 1
    return n


def arrays_of(states) -> list:
    out = []

    def walk(x):
        if isinstance(x, mx.array):
            out.append(x)
        elif isinstance(x, (list, tuple)):
            for y in x:
                walk(y)

    walk(states)
    return out


def snapshot(cache):
    """Copies of the cache state. A KV cache writes into its buffer in place,
    and Qwen 3.5/3.6's linear-attention layers cannot be trimmed back, so the
    only way to reuse a prompt is to keep a copy taken where it ended."""

    def copy(x):
        if x is None:
            return None
        if isinstance(x, (list, tuple)):
            return type(x)(copy(y) for y in x)
        return x + 0 if isinstance(x, mx.array) else x

    states = [copy(c.state) for c in cache]
    mx.eval(arrays_of(states))
    return states


def restore(cache, states) -> None:
    def copy(x):
        if isinstance(x, (list, tuple)):
            return type(x)(copy(y) for y in x)
        return x + 0 if isinstance(x, mx.array) else x

    for c, state in zip(cache, states):
        c.state = copy(state)


class VisionText(nn.Module):
    """A model loaded by mlx-vlm, shaped as an mlx-lm model, so the one
    generation loop below -- prompt cache, checkpoints, reasoning budget --
    drives text and images alike, and the weights are loaded once (C.25).

    Qwen's vision models place tokens with three-axis positions, and an image
    takes fewer positions than tokens. Positions and image embeddings are
    held for the whole prompt and sliced by the cache offset on every call,
    so a prefill resumed from a checkpoint sees what one from the start would.
    Measured 2026-09-23 on Qwen3.6-35B-A3B: text output identical token for
    token to mlx-lm's own load, same speed, 0.9 GB more at peak."""

    def __init__(self, vlm):
        super().__init__()
        self.vlm = vlm
        self.language = vlm.language_model
        self.fa_idx = getattr(self.language.model, "fa_idx", 0)
        self.embeds = None      # (1, N, D) for a prompt with images
        self.positions = None   # (3, 1, N)
        self.delta = 0          # position minus index after the prompt
        self.features = {}      # image digest -> vision features, the last few

    @property
    def layers(self):
        return self.language.layers

    def make_cache(self):
        return self.language.make_cache()

    def set_prompt(self, tokens, images=None, digest=None) -> None:
        if not images:
            self.embeds, self.positions, self.delta = None, None, 0
            return
        store = self.features

        class Features:
            def get(self, key):
                return store.get(key)

            def put(self, key, value):
                while len(store) >= 4:
                    store.pop(next(iter(store)))
                store[key] = value

        features = self.vlm.get_input_embeddings(
            mx.array(tokens)[None], mx.array(images["pixel_values"]),
            image_grid_thw=mx.array(images["image_grid_thw"]),
            vision_cache=Features(), _image_key=digest,
        )
        self.embeds = features.inputs_embeds
        self.positions = features.position_ids
        self.delta = int(features.rope_deltas.reshape(-1)[0].item())
        mx.eval(self.embeds, self.positions)

    def __call__(self, inputs, cache=None, input_embeddings=None):
        offset = int(cache[self.fa_idx].offset) if cache is not None else 0
        n = inputs.shape[-1]
        embeds = input_embeddings
        if self.positions is not None and offset + n <= self.positions.shape[-1]:
            positions = self.positions[..., offset:offset + n]
            if embeds is None:
                embeds = self.embeds[:, offset:offset + n]
        else:
            start = offset + self.delta
            positions = mx.broadcast_to(
                mx.arange(start, start + n)[None, None], (3, inputs.shape[0], n)
            )
        out = self.language(inputs, inputs_embeds=embeds, cache=cache, position_ids=positions)
        return out.logits if hasattr(out, "logits") else out


def image_parts(messages, accept: bool = True):
    """The images in `messages`, in order, and the messages as the chat
    template takes them: each image part becomes `{"type": "image"}`."""
    images, shaped = [], []
    for message in messages:
        content = message.get("content")
        if not isinstance(content, list):
            shaped.append(message)
            continue
        parts = []
        for part in content:
            if part.get("type") == "image":
                if not accept:
                    raise RuntimeError(
                        "this model was not loaded with a vision encoder; it cannot read images"
                    )
                from PIL import Image  # only with mlx-vlm; a text engine never gets here
                if part.get("path"):
                    raw = pathlib.Path(part["path"]).read_bytes()
                else:
                    raw = base64.b64decode(part["data"])
                images.append((raw, Image.open(io.BytesIO(raw)).convert("RGB")))
                parts.append({"type": "image"})
            else:
                parts.append({"type": "text", "text": part.get("text", "")})
        shaped.append({**message, "content": parts})
    return images, shaped


class Engine:
    def __init__(self) -> None:
        self.model = None
        self.tokenizer = None
        self.processor = None   # set when the model was loaded with its vision encoder
        self.path = None
        self.cache = None
        # Tokens whose state the checkpoint holds: the last prompt, without its
        # generation prompt, so the next turn's history can start from it.
        self.checkpoint = None
        self.checkpoint_tokens: list[int] = []

    def load(self, request: dict) -> dict:
        path = pathlib.Path(request["path"]).expanduser()
        config = json.loads((path / "config.json").read_text())
        self.model = self.tokenizer = self.processor = None
        mx.clear_cache()
        if "vision_config" in config and vlm_available():
            from mlx_vlm import load as load_vision
            vlm, self.processor = load_vision(str(path))
            self.model = VisionText(vlm)
            self.tokenizer = load_tokenizer(path, NO_REMOTE_CODE)
        else:
            self.model, self.tokenizer = load(str(path), tokenizer_config=NO_REMOTE_CODE)
        self.path = path
        self.cache = None
        self.checkpoint = None
        self.checkpoint_tokens = []
        text = config.get("text_config", config)
        self.heads = int(text.get("num_attention_heads") or 64)
        head_dim = int(text.get("head_dim") or text["hidden_size"] // self.heads)
        limit = mx.device_info().get("max_buffer_length") or 8 * 1024**3
        self.score_budget = int(limit) // 4
        self.fused = fused_attention(head_dim)
        weights = sum(p.stat().st_size for p in path.glob("*.safetensors"))
        return {
            "event": "loaded",
            "model_type": config.get("model_type"),
            "weights_bytes": weights,
            "vision": self.processor is not None,
        }

    def render(self, messages, tools, thinking, generation_prompt: bool,
               budget=None, effort=None):
        """The prompt's tokens, and the processed images when it has any."""
        # Past assistant turns rendered the same way whatever follows them.
        # Qwen 3.6's template gives an assistant turn an empty think block
        # only while no user message comes after it, so every user message
        # (steering, a check-in, the checks' report) re-rendered the history
        # before it and the prompt cache missed: 100-130 s of prefill on
        # steps that added 2-3K tokens (D.E2E-21). `preserve_thinking` keeps
        # the block on every turn; a template without the variable ignores it.
        images, messages = image_parts(messages, accept=self.processor is not None)
        kwargs = {
            "add_generation_prompt": generation_prompt,
            "tokenize": not images,
            "preserve_thinking": True,
        }
        if tools:
            kwargs["tools"] = tools
        template = str(getattr(self.tokenizer, "chat_template", "") or "")
        if thinking is not False and budget is not None and "thinking_budget" in template:
            # The template's own budget (Seed-OSS), in steps; see NATIVE_BUDGET_STEP.
            kwargs["thinking_budget"] = int(budget) // NATIVE_BUDGET_STEP * NATIVE_BUDGET_STEP
        if thinking is not False and effort in ("low", "medium", "high"):
            # A template that takes a level (harmony's reasoning_effort).
            kwargs["reasoning_effort"] = effort
        if thinking is not None:
            kwargs["enable_thinking"] = bool(thinking)
            if not thinking:
                # Seed-OSS's template switches reasoning off with a zero
                # thinking budget (and pre-fills a closed think block, as
                # Qwen's does for enable_thinking). A template that does not
                # use the variable ignores it.
                kwargs["thinking_budget"] = 0
                # gpt-oss cannot switch reasoning off; its least is "low".
                kwargs["reasoning_effort"] = "low"
        rendered = self.tokenizer.apply_chat_template(messages, **kwargs)
        if not images:
            return list(rendered), None
        # The processor expands each image placeholder to the image's patch
        # tokens; the same image expands the same way every turn, so the
        # prompt cache still matches the history.
        processed = self.processor(text=[rendered], images=[image for _, image in images],
                                   return_tensors="np")
        tokens = [int(token) for token in processed["input_ids"][0]]
        digest = hashlib.sha256(b"".join(hashlib.sha256(raw).digest() for raw, _ in images))
        return tokens, {"pixel_values": processed["pixel_values"],
                        "image_grid_thw": processed["image_grid_thw"],
                        "digest": digest.hexdigest()}

    def prefill_step(self, offset: int) -> int:
        """Tokens per prefill step at this cache offset. A step materialises
        its attention scores, heads x step x keys, as one Metal buffer. Seen
        2026-09-18: at ~160k tokens of context a fixed 8,192-token step asked
        for 41.9 GB against a 41.7 GB buffer limit and the run died. The step
        shrinks as the context grows, keeping that buffer under a quarter of
        the limit -- only where attention materialises its scores at all
        (`fused_attention`). Seen 2026-09-19: shrinking the step for a model
        whose attention is fused took Seed-OSS-36B 2,418 s to prefill 60,112
        tokens."""
        if self.fused:
            return PREFILL_STEP
        keys = offset + PREFILL_STEP
        step = self.score_budget // (self.heads * SCORE_BYTES * keys)
        return max(PREFILL_MIN_STEP, min(PREFILL_STEP, step // 256 * 256))

    def prefill(self, tokens: list[int], offset: int, progress=lambda done, total: None) -> None:
        start = 0
        while start < len(tokens):
            progress(start, len(tokens))
            step = self.prefill_step(offset + start)
            self.model(mx.array(tokens[start:start + step])[None], cache=self.cache)
            mx.eval([c.state for c in self.cache])
            start += step
        progress(len(tokens), len(tokens))
        mx.clear_cache()

    def settle(self, prompt_length: int) -> None:
        """After a generation, a cache that can be cut back keeps only the
        prompt: what was generated is sent back as history, rendered by the
        template, and matched against it from there. One that cannot keeps
        its copy (`checkpoint`), taken before generating."""
        if self.checkpoint is not None or self.cache is None:
            return
        try:
            extra = self.cache[0].offset - prompt_length
            if extra > 0:
                trim_prompt_cache(self.cache, extra)
        except Exception:
            # Unsure what the cache holds: start the next prompt from nothing
            # rather than from a state that may not match it.
            self.cache = None
            self.checkpoint_tokens = []

    def resume(self, base: list[int], progress) -> int:
        """Brings the cache to the end of `base`, reusing what it can of the
        last prompt, and returns how many tokens were reused."""
        reused = 0
        trimmable = (self.cache is not None and not isinstance(self.model, VisionText)
                     and can_trim_prompt_cache(self.cache))
        if trimmable and self.checkpoint is None:
            # A cache that can be cut back is cut back to what the new prompt
            # shares with the last one, wherever they part -- no copy held
            # beside it. The copy doubled the memory a dense model's cache
            # takes, and a prompt that differed anywhere was prefilled whole.
            reused = common_prefix(self.checkpoint_tokens, base)
            if reused:
                trim_prompt_cache(self.cache, self.cache[0].offset - reused)
            else:
                self.cache = make_prompt_cache(self.model)
        elif self.checkpoint is not None and base[:len(self.checkpoint_tokens)] == self.checkpoint_tokens:
            restore(self.cache, self.checkpoint)
            reused = len(self.checkpoint_tokens)
        else:
            self.cache = make_prompt_cache(self.model)
            self.checkpoint = None
        try:
            self.prefill(base[reused:], reused, progress)
        except BaseException:
            # Half a prefill matches no prompt: the next one starts clean.
            self.cache = None
            self.checkpoint = None
            self.checkpoint_tokens = []
            raise
        # Qwen 3.5/3.6's linear-attention layers, and a model reading images,
        # cannot be cut back: for them the prompt's state is copied, to be
        # restored when the next prompt extends this one.
        if isinstance(self.model, VisionText) or not can_trim_prompt_cache(self.cache):
            self.checkpoint = snapshot(self.cache)
        else:
            self.checkpoint = None
        self.checkpoint_tokens = base
        return reused

    def chat(self, request: dict, reply, cancelled=lambda: False) -> dict:
        if self.model is None:
            raise RuntimeError("no model loaded")
        messages = request["messages"]
        tools = request.get("tools")
        thinking = request.get("thinking")
        budget = request.get("reasoning_budget")
        effort = request.get("reasoning_effort")
        max_tokens = int(request.get("max_tokens") or 8192)
        template = str(getattr(self.tokenizer, "chat_template", "") or "")
        delimiters = think_delimiters(template)
        if budget is not None:
            budget = max(0, min(int(budget), max(0, max_tokens - FINAL_RESERVE)))
        if request.get("seed") is not None:
            mx.random.seed(int(request["seed"]))
        sampler = make_sampler(
            temp=float(request.get("temperature") or 0.0),
            top_p=float(request.get("top_p") or 0.0),
            top_k=int(request.get("top_k") or 0),
            min_p=float(request.get("min_p") or 0.0),
        )
        logits_processors = make_logits_processors(
            presence_penalty=request.get("presence_penalty"),
            repetition_penalty=request.get("repetition_penalty"),
        )

        started = time.perf_counter()
        full, images = self.render(messages, tools, thinking, True, budget, effort)
        base, _ = self.render(messages, tools, thinking, False, budget, effort)
        if isinstance(self.model, VisionText):
            self.model.set_prompt(full, images, images and images["digest"])
        if full[:len(base)] != base:
            # The template does not render the history the same way with and
            # without a generation prompt; checkpoint just before the last token.
            base = full[:-1]
        reused = self.resume(base, lambda done, total: reply(
            {"event": "prefill", "processed": int(done), "total": int(total)}
        ))
        prefilled = time.perf_counter()

        # Where generation starts: inside a think block if the template opened
        # one and did not close it.
        opening = self.tokenizer.decode(full[len(base):])
        starts_inside = bool(delimiters) and delimiters[0] in opening and delimiters[1] not in opening
        tracker = ReasoningStream(delimiters, starts_inside)
        # A budget is enforced only where the phase can be seen and closed.
        enforce = budget is not None and delimiters is not None
        generated = 0
        finish = "length"
        finalizations = 0
        written = []          # everything generated, both channels, for the loop check
        written_len = 0
        repetition = RepetitionSignals()

        def stream(prompt_tokens, limit):
            nonlocal generated, finish, written_len
            for response in stream_generate(
                self.model, self.tokenizer, mx.array(prompt_tokens), max_tokens=limit,
                sampler=sampler, logits_processors=logits_processors,
                prompt_cache=self.cache,
                # A long prefill says it is alive, so PWR can tell one
                # from an engine stuck in an evaluation.
                prompt_progress_callback=lambda done, total: reply(
                    {"event": "prefill", "processed": int(done), "total": int(total)}
                ),
            ):
                generated += 1
                if cancelled():
                    finish = "cancelled"
                    return finish
                text = response.text
                written.append(text)
                written_len += len(text)
                if written_len >= REPEAT_SPAN * REPEAT_LIMIT and generated % 32 == 0:
                    if looping("".join(written)):
                        finish = "repetition"
                        return finish
                was_reasoning = tracker.in_reasoning
                pieces = tracker.feed(text)
                repetition.feed("reasoning" if was_reasoning and tracker.in_reasoning
                                else "answer", text)
                for channel, piece in pieces:
                    reply({"event": "delta", "channel": channel, "text": piece})
                if tracker.reopened:
                    return "reopened"
                if enforce and tracker.in_reasoning and tracker.reasoning_tokens >= budget:
                    return "budget"
                if response.finish_reason is not None:
                    finish = response.finish_reason
                    return finish
            return finish

        try:
            outcome = stream(full[len(base):], max_tokens)
            while outcome == "budget" and finalizations < FINALIZATION_ATTEMPTS:
                # Close the reasoning with the template's own delimiter and let
                # the answer follow, inside what is left of the cap.
                finalizations += 1
                for channel, piece in tracker.force_close():
                    reply({"event": "delta", "channel": channel, "text": piece})
                reply({"event": "finalizing", "reasoning_tokens": tracker.reasoning_tokens})
                forced = list(self.tokenizer.encode(delimiters[1] + CLOSE_SUFFIX,
                                                    add_special_tokens=False))
                finish = "length"
                outcome = stream(forced, max(1, max_tokens - generated))
        finally:
            self.settle(len(base))
        for channel, piece in tracker.flush():
            reply({"event": "delta", "channel": channel, "text": piece})
        if finalizations and finish not in ("cancelled", "repetition") and (
                outcome in ("reopened", "budget") or not tracker.answered):
            # A failed transition is a failure, never a successful reply that
            # holds only private reasoning or half a tool call.
            finish = "reasoning_unfinished"
        done = time.perf_counter()
        # What the prompt cache saved, per request: the measure of whether a
        # history stayed a prefix of the next (D.E2E-21).
        trace({"messages": messages[-2:], "thinking": thinking,
               "raw": "".join(written), "finish": finish,
               "prompt_tokens": len(full), "cached_tokens": reused,
               "prefilled_tokens": len(base) - reused,
               "prefill_s": round(prefilled - started, 3),
               "copied_cache": self.checkpoint is not None,
               "peak_memory_gb": round(mx.get_peak_memory() / 1e9, 2)})
        return {
            "event": "done",
            "finish_reason": finish if finish in (
                "stop", "repetition", "cancelled", "reasoning_unfinished") else "length",
            "budget_forced": finalizations > 0,
            "repetition_signals": repetition.summary(),
            # Whether the reasoning counts below are the engine's own: only
            # where the template's delimiters let it see the phase.
            "reasoning_tracked": delimiters is not None,
            "usage": {
                "prompt_tokens": len(full),
                "cached_tokens": reused,
                "prefilled_tokens": len(base) - reused,
                "completion_tokens": generated,
                "reasoning_tokens": tracker.reasoning_tokens if delimiters else None,
                "answer_tokens": generated - tracker.reasoning_tokens if delimiters else None,
            },
            "timings": {
                "prefill_secs": round(prefilled - started, 3),
                "generation_secs": round(done - prefilled, 3),
                "peak_memory_bytes": int(mx.get_peak_memory()),
            },
        }


def main() -> None:
    engine = Engine()
    inbox = Inbox(sys.stdin)
    while True:
        line = inbox.next()
        if line is None:
            return
        line = line.strip()
        if not line:
            continue
        request = {}
        try:
            request = json.loads(line)
            ident = request.get("id")
            op = request.get("op")
            if op == "shutdown":
                emit({"id": ident, "event": "bye"})
                return
            if op == "attention":
                # What a prefill step of a model with this head dimension
                # holds in scores, without loading anything: the window is
                # computed before a model is loaded.
                limit = mx.device_info().get("max_buffer_length") or 8 * 1024**3
                fused = fused_attention(int(request["head_dim"]))
                emit({"id": ident, "event": "attention", "fused": fused,
                      "scores_bytes": 0 if fused else int(limit) // 4})
            elif op == "load":
                emit({"id": ident, **engine.load(request)})
            elif op == "chat":
                result = engine.chat(request, lambda event: emit({"id": ident, **event}),
                                     lambda: inbox.is_cancelled(ident))
                emit({"id": ident, **result})
            else:
                emit({"id": ident, "event": "error", "message": f"unknown op {op!r}"})
        except Exception as error:  # the process answers every request, even a failed one
            emit({
                "id": request.get("id"),
                "event": "error",
                "message": f"{type(error).__name__}: {error}",
                "trace": traceback.format_exc()[-2000:],
            })


if __name__ == "__main__":
    main()
