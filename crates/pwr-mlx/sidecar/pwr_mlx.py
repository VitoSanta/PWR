#!/usr/bin/env python3
"""PWR's MLX engine sidecar for Gemma 4: one model, driven over JSON lines on stdio.

PWR is built around the Gemma 4 family (12B, 26B-A4B, 31B; decision of
2026-10-14). This process loads a Gemma 4 pack through mlx-vlm -- weights,
vision encoder, Google's chat template -- renders the template itself, generates,
and reuses the prompt cache across turns. It does nothing else: tool calls come
back as the model wrote them (`<|tool_call>call:NAME{...}<tool_call|>`) and PWR's
Gemma adapter reads them. Policy, sandbox and audit stay in PWR.

Protocol. One JSON object per line in each direction.

  -> {"id": 1, "op": "load", "path": "/path/to/model"}
  <- {"id": 1, "event": "loaded", "model_type": "...", "weights_bytes": N, "vision": true,
      "rope_scaling": null}

A message's content is a string, or a list of parts: {"type": "text", "text":
"..."} and {"type": "image", "path": "/abs/file.png"} or {"type": "image",
"data": "<base64>"}.

  -> {"id": 2, "op": "chat", "messages": [...], "tools": [...] | null,
      "thinking": true | false | null, "reasoning_budget": N | null,
      "max_tokens": N, "temperature": t, "top_p": p, "top_k": k, "seed": s}
  <- {"id": 2, "event": "delta", "channel": "reasoning" | "content", "text": "..."}
  <- {"id": 2, "event": "finalizing", "reasoning_tokens": N}   (budget reached)
  <- ...
  <- {"id": 2, "event": "done", "finish_reason": "stop" | "length" | "repetition" | "cancelled"
      | "reasoning_unfinished", "budget_forced": bool, "reasoning_tracked": bool,
      "usage": {...}, "timings": {...}}

`thinking` true puts `<|think|>` in the system turn (the template's
`enable_thinking`). `reasoning_budget` is enforced on the thought channel:
after that many tokens inside it the engine writes `<channel|>`, once, and lets
the answer follow within `max_tokens`. If the model reopens its reasoning or
ends without an answer, the request finishes `reasoning_unfinished`.

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

# Metal can terminate a command with ImpactingInteractivity while saving GPU
# context, even when allocations are below the wired-memory limit. Apply the
# driver workaround before MLX creates its device (llama.cpp PR #22216).
# An explicit caller value, including "0", is authoritative.
if sys.platform == "darwin":
    os.environ.setdefault("AGX_RELAX_CDM_CTXSTORE_TIMEOUT", "1")

import mlx.core as mx
import mlx.nn as nn
from mlx_lm import stream_generate
from mlx_lm.models.cache import make_prompt_cache
from mlx_lm.sample_utils import make_logits_processors, make_sampler
from mlx_lm.utils import load_tokenizer

# Larger steps, and the allocator cleared once rather than after every step:
# the engine spike found both choices slowed its in-process prefill well below
# LM Studio's on the same MLX build.
PREFILL_STEP = 8192
PREFILL_MIN_STEP = 16
# A stop is seen between prefill chunks, so a chunk has to be short in *time*,
# not only in tokens: a dense 31B model takes minutes over one 8,192-token
# chunk, and Stop, a model switch and everything else waited behind it with
# nothing on screen (2026-10-01, measured in the app: Stop at 17:41:52, the
# engine free at about 17:44:40). The first chunk is small; after it the chunk
# is as many tokens as take about this long at the speed just measured. Smaller
# chunks cost nothing measurable (Gemma 4 12B, 4,976 tokens: 25.0 s at 1,024,
# 25-27 s at 8,192).
PREFILL_CHUNK_SECS = 3.0
PREFILL_FIRST_STEP = 512
# Non-fused attention materialises scores proportional to the cache length.
# Its old 256-token floor could take much longer than the time target.
PREFILL_UNFUSED_CHUNK_SECS = 1.0
# The most memory MLX keeps aside, freed but not returned, to reuse for its
# next buffers. Unbounded by default: seen 2026-10-07 (Nemotron 30B 4-bit,
# about 17 GB of weights, a long goal on a 64 GB machine) the engine stood at
# 50 GB, the system swapped, and Metal ended a generation with "Impacting
# Interactivity". What the model and its cache hold is not touched by this.
CACHE_LIMIT_BYTES = int(float(os.environ.get("PWR_MLX_CACHE_GB", "2")) * 1024**3)
# Attention scores are held in the activation dtype (bf16 or fp16).
SCORE_BYTES = 2
# How a cache that cannot be cut back whole is kept for the next prompt.
# "whole" copies every layer. "layers" copies only the layers that cannot be
# cut back and cuts the others back to where the copy was taken. Measured
# 2026-10-13 on Bonsai 27B: the 48 linear-attention layers, the ones that
# cannot be cut back, hold 154 MB whatever the context; the 16 that grow with
# it (65,536 bytes a token) are plain KV buffers. Copied whole, the context
# was held twice: 125 recorded runs of Qwen3.5 9B stood at 1.9 copies between
# generations. The next token's scores were the same either way, to the bit,
# on Bonsai 27B 1-bit and Qwen3.5 9B. For Gemma the sliding layers are the
# ones copied (1,024 tokens each) and the global layers are cut back:
# "layers" by default since 2026-10-14, so the 262,144-token context is held
# once; "whole" stays as the switch to compare against.
CHECKPOINT = os.environ.get("PWR_MLX_CHECKPOINT", "layers")
# The most a prefill step may hold in attention scores where attention is not
# fused, in MB; unset, 1 GiB (`pwr_domain::gemma::SCORES_BYTES`): Gemma's
# global layers have 512-wide heads, which MLX does not fuse, and a quarter of
# the largest Metal buffer (10.4 GB on a 64 GB machine) took the room the
# 262,144-token context needs on every tier. Measured 2026-10-13 on Qwen3.5 9B at 17k and 42k tokens and
# on Bonsai 27B at 17k: a step holds 2.7-2.9 bytes for every head, token of
# the step and token of context, over a floor of 0.26-0.37 GB that the step
# does not move; steps of 64 tokens and more read at the same speed, 16 and 32
# at half and three quarters of it. A budget asked for is kept with those
# measures: `BUDGET_SCORE_BYTES` a score, and `SCORES_FLOOR_BYTES` told to PWR
# beside it.
SCORES_MB = os.environ.get("PWR_MLX_SCORES_MB")
DEFAULT_SCORES_BYTES = 1024**3
BUDGET_SCORE_BYTES = 3
SCORES_FLOOR_BYTES = 512 * 1024**2
# Bits the global layers keep their keys and values at: 16 unless asked
# for 8 (`PWR_MLX_KV_BITS=8`), the one lever left where a tier's memory does not
# hold the full context at 16 (`pwr_domain::gemma::window`). Gemma's cache
# broke at 4 bits (2026-10-14, 26B-A4B: one line in another language over and
# over), so 4 is refused. The sliding layers stay at 16: they hold 1,024 tokens.
KV_BITS = 8 if os.environ.get("PWR_MLX_KV_BITS") == "8" else None
# Google's assistant for the pack (multi-token prediction): it drafts a few
# tokens and the model verifies them. Experimental: the 2026-10-14 short
# comparisons did not meet exact-text/speed gates. Explicit opt-in only until
# a pack passes those gates -- this variable, or the person's switch in the
# app sent with the load; loading a drafter alone is not evidence of them.
# `PWR_MLX_DRAFT_BLOCK` fixes the block, otherwise the assistant's own.
ASSISTANT = os.environ.get("PWR_MLX_ASSISTANT", "off") == "on"
DRAFT_BLOCK = int(os.environ.get("PWR_MLX_DRAFT_BLOCK") or 0) or None
KV_GROUP = 64
# Tokens an image takes in Gemma 4's prompt, one of the budgets Google gives
# (70, 140, 280, 560, 1120; the high ones for text and documents). Unset, the
# model's own (280: 273 image tokens for a 1280x800 screenshot, 1,092 at 1120).
IMAGE_TOKENS = int(os.environ["PWR_MLX_IMAGE_TOKENS"]) if os.environ.get("PWR_MLX_IMAGE_TOKENS") in (
    "70", "140", "280", "560", "1120") else None
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
THINK_DELIMITERS = (("<|channel>thought", "<channel|>"),)
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


# A model folder's own Python (custom tokenizers, processors) is never run:
# said explicitly rather than left to a library default.
NO_REMOTE_CODE = {"trust_remote_code": False}

# When set, every chat's last messages and the model's raw output are appended
# here as JSON lines: the one place the model's own text exists before PWR
# reads calls out of it, which is what a misread call has to be checked against.
TRACE = os.environ.get("PWR_MLX_TRACE")
# When set, a folder: every chat request is written there whole -- messages,
# tools, sampling -- with the exact tokens the model is given, one file a
# request, so a reply can be replayed outside PWR. Holds the person's text,
# like the trace.
PROMPT_DUMP = os.environ.get("PWR_MLX_PROMPT_DUMP")


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


def dump_prompt(request: dict, tokens: list[int], tokenizer) -> None:
    folder = pathlib.Path(PROMPT_DUMP)
    folder.mkdir(parents=True, exist_ok=True)
    number = len(list(folder.glob("prompt-*.json")))
    record = {"at_ms": int(time.time() * 1000), "request": request, "tokens": tokens,
              "text": tokenizer.decode(tokens)}
    (folder / f"prompt-{number:04d}.json").write_text(json.dumps(record, ensure_ascii=False))


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


class OneThought:
    """A logits processor that lets a reply open one thought channel at most.

    Gemma 4 writes its reasoning as `<|channel>thought ... <channel|>`, one
    block before the answer. Measured 2026-10-02 (goal-ledger, gemma-4-12B):
    after a tool result, where the template adds no generation prompt, the
    model wrote `<|channel>thought\n<channel|>` and opened the channel again,
    hundreds of times, until the loop guard cut the reply -- three replies
    of that and the task was given up at its second action. Once the
    channel has been opened or closed in this reply (or the prompt closed it:
    the template's empty block at a new turn, or `empty_thought` after a tool
    result), it cannot be opened again.

    It no longer forbids the channel when reasoning is off. Google's own
    template has the model open an empty block before every answer, and the
    model does so after a tool result too: measured 2026-10-14 outside PWR,
    Gemma 4 26B wrote `<|channel>thought\n<channel|>` and a well-formed call
    six times out of six, where with the channel forbidden it gave an empty
    reply or repeated lines in another language."""

    def __init__(self, open_id: int, close_id: int, allow_thought=True):
        self.open_id, self.close_id = open_id, close_id
        self.allow_thought = allow_thought
        self.seen = None
        self.shut = False
        self.closed = False

    def __call__(self, tokens, logits):
        if self.seen is None:
            # Ignore markers from older replies. A boundary marker belongs to
            # this reply (thinking-off templates may pre-close the channel).
            self.seen = tokens.size
            boundary = int(tokens[-1].item()) if tokens.size else None
            self.shut = not self.allow_thought or boundary in (self.open_id, self.close_id)
            self.closed = not self.allow_thought or boundary == self.close_id
        else:
            if tokens.size < self.seen:
                self.seen = 0
            fresh = tokens[self.seen:].tolist() if tokens.size > self.seen else []
            self.seen = tokens.size
            self.shut = self.shut or self.open_id in fresh or self.close_id in fresh
            self.closed = self.closed or self.close_id in fresh
        if self.shut:
            logits[..., self.open_id] = -float("inf")
        if self.closed:
            # Blocking re-opening alone still allowed <channel|> hundreds
            # of times (Gemma short ledger, 2026-10-03). A single thought
            # block also has exactly one closing delimiter.
            logits[..., self.close_id] = -float("inf")
        return logits


def one_thought(tokenizer, template: str):
    """The processor for templates with Gemma 4's thought channel, when both
    of its markers are single tokens; otherwise None."""
    if "<|channel>" not in template:
        return None
    convert = getattr(tokenizer, "convert_tokens_to_ids", None)
    if convert is None:
        return None
    ids = []
    for marker in ("<|channel>", "<channel|>"):
        try:
            token = convert(marker)
        except Exception:
            return None
        if not isinstance(token, int) or token < 0 or token == getattr(tokenizer, "unk_token_id", None):
            return None
        ids.append(token)
    return OneThought(*ids)


EMPTY_THOUGHT = "<|channel>thought\n<channel|>"


def empty_thought(tokenizer, template: str, thinking) -> list:
    """The tokens of Gemma 4's empty thought block, for a reply that goes on
    with a model turn after a tool result while reasoning is not on.

    Gemma's template opens no new model turn after a tool result: the model
    continues its own, and with reasoning off the template's empty block --
    which it writes at the start of every other turn -- is missing there.
    Google advises adding it for 26B and 31B "to stabilize behavior"
    (prompt formatting for Gemma 4). Nothing for any other template."""
    has_empty_block = EMPTY_THOUGHT in template or EMPTY_THOUGHT.replace("\n", "\\n") in template
    if thinking is True or not has_empty_block or "<|tool_call>" not in template:
        return []
    return list(tokenizer.encode(EMPTY_THOUGHT, add_special_tokens=False))


# Where Gemma 4 hands control back (Google's prompt formatting): the end of
# its turn, and `<|tool_response>`, written after its calls for the caller
# to answer. The pack's generation_config lists them; mlx-lm's tokenizer
# keeps only `<eos>`. `<tool_call|>` is not one: a reply may hold several calls.
TURN_ENDS = ("<turn|>", "<|tool_response>")


def add_turn_ends(tokenizer) -> None:
    """Adds each of Gemma's stops the vocabulary has as a single special
    token to the tokens that stop generation. A marker the vocabulary lacks is
    skipped: converting it would give the unknown token, and stopping on that
    would end generation at any unknown word."""
    add = getattr(tokenizer, "add_eos_token", None)
    if add is None:
        return
    unknown = getattr(tokenizer, "unk_token_id", None)
    for marker in TURN_ENDS:
        try:
            token = tokenizer.convert_tokens_to_ids(marker)
        except Exception:
            continue
        if isinstance(token, int) and token != unknown and token >= 0:
            add(marker)


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


PROTOCOL_STDOUT = sys.stdout


def emit(obj: dict) -> None:
    PROTOCOL_STDOUT.write(json.dumps(obj) + "\n")
    PROTOCOL_STDOUT.flush()


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


def score_budget() -> int:
    """What a prefill step may hold in attention scores, in bytes."""
    limit = mx.device_info().get("max_buffer_length") or 8 * 1024**3
    asked = float(SCORES_MB) * 1024**2 if SCORES_MB else DEFAULT_SCORES_BYTES
    return min(int(limit) // 4, int(asked))


class Offset(int):
    """Where a layer that is cut back, not copied, stood when a copy was taken."""


def quantized(cache, bits):
    """The cache with its global layers -- the plain KV caches that grow with
    the context -- held at `bits`; the sliding layers as they are."""
    if not bits:
        return cache
    return [c.to_quantized(group_size=KV_GROUP, bits=bits)
            if type(c).__name__ == "KVCache" and callable(getattr(c, "to_quantized", None)) else c
            for c in cache]


def appends_only(layer) -> bool:
    """Whether a layer's cache only ever writes past its offset, so cutting
    the offset back to where it stood gives back what was there: mlx-vlm's
    `KVCache`, Gemma's global layers, and its quantized kind. Judged by kind, and narrowly: a
    sliding-window cache rewrites its ring, and is copied."""
    return (type(layer).__name__ in ("KVCache", "QuantizedKVCache")
            and isinstance(getattr(layer, "offset", None), int)
            and callable(getattr(layer, "trim", None)))


def snapshot(cache):
    """Copies of the cache state. Gemma's sliding-window layers rewrite their
    ring in place and cannot be cut back once full, so the only way to reuse a
    prompt is to keep a copy taken where it ended.

    With `CHECKPOINT` "layers" a layer that only appends keeps its offset in
    place of a copy; see `appends_only`."""

    def copy(x):
        if x is None:
            return None
        if isinstance(x, (list, tuple)):
            return type(x)(copy(y) for y in x)
        return x + 0 if isinstance(x, mx.array) else x

    by_layer = CHECKPOINT == "layers"
    # mlx-lm 0.32 folds offsets and ring metadata into state. Older caches
    # expose meta_state separately: preserve it where present, without dropping
    # metadata from either representation.
    states = [Offset(c.offset) if by_layer and appends_only(c)
              else (copy(c.state), copy(getattr(c, "meta_state", None))) for c in cache]
    mx.eval(arrays_of(states))
    return states


def restore(cache, states) -> None:
    def copy(x):
        if isinstance(x, (list, tuple)):
            return type(x)(copy(y) for y in x)
        return x + 0 if isinstance(x, mx.array) else x

    # Checked before anything is changed: a cache half put back matches no prompt.
    for c, kept in zip(cache, states):
        if isinstance(kept, Offset) and c.offset < kept:
            raise ValueError("the cache is shorter than its copy")
    for c, kept in zip(cache, states):
        if isinstance(kept, Offset):
            c.trim(c.offset - kept)
        else:
            state, metadata = kept
            c.state = copy(state)
            if hasattr(c, "meta_state"):
                c.meta_state = copy(metadata)


class VisionText(nn.Module):
    """A Gemma 4 pack loaded by mlx-vlm, shaped as an mlx-lm model, so the one
    generation loop below -- prompt cache, checkpoints, reasoning budget --
    drives text and images alike, and the weights are loaded once (C.25).

    A prompt's image embeddings are held for the whole prompt and sliced by
    the cache offset on every call, so a prefill resumed from a checkpoint
    sees what one from the start would. Gemma reads an image's tokens with
    attention both ways: each sees the whole image. mlx-vlm does that only for
    a prompt read whole from an empty cache; here every step that holds an
    image gets it (`image_masks`), and no step cuts an image in two
    (`whole_images`)."""

    def __init__(self, vlm):
        super().__init__()
        self.vlm = vlm
        self.language = vlm.language_model
        self.embeds = None      # (1, N, D) for a prompt with images
        self.features = {}      # image digest -> vision features, the last few
        self.per_layer = None   # per-layer inputs, where the model has them
        self.blocks = None      # image number of each prompt token, -1 for text
        self.step_blocks = None  # the same for the tokens of the step being read
        model = self.language.model
        plain = model._make_masks
        model._make_masks = lambda h, cache, types=None: image_masks(
            model, plain(h, cache, None), h, cache, self.step_blocks)

    @property
    def layers(self):
        return self.language.layers

    def make_cache(self):
        return self.language.make_cache()

    def set_prompt(self, tokens, images=None, digest=None) -> None:
        self.embeds = self.per_layer = self.blocks = None
        if not images:
            return
        store = self.features

        class Features:
            def get(self, key):
                return store.get(key)

            def put(self, key, value):
                while len(store) >= 4:
                    store.pop(next(iter(store)))
                store[key] = value

        arrays = images["arrays"]
        features = self.vlm.get_input_embeddings(
            mx.array(tokens)[None], mx.array(arrays["pixel_values"]),
            **{k: mx.array(arrays[k]) for k in ("image_position_ids",) if k in arrays},
            vision_cache=Features(), _image_key=digest,
        )
        self.embeds, self.per_layer = features.inputs_embeds, features.per_layer_inputs
        self.blocks = image_blocks(tokens, self.vlm.config.image_token_id)
        mx.eval(self.embeds)

    def whole_images(self, start: int, end: int) -> int:
        """Where a step of the prompt from `start` to `end` ends without
        cutting an image: at the start of the image `end` falls in, or at its
        end when the image starts the step."""
        blocks = self.blocks
        if blocks is None or not 0 < end < len(blocks) or blocks[end] < 0 or blocks[end] != blocks[end - 1]:
            return end
        first = end
        while first > 0 and blocks[first - 1] == blocks[end]:
            first -= 1
        if first > start:
            return first
        last = end
        while last < len(blocks) and blocks[last] == blocks[end]:
            last += 1
        return last

    def __call__(self, inputs, cache=None, input_embeddings=None, **outputs):
        """The logits of a step; with `outputs` (mlx-vlm's `return_hidden`,
        `return_shared_kv`), the language model's whole output."""
        offset = int(cache[0].offset) if cache is not None else 0
        n = inputs.shape[-1]
        embeds = per_layer = None
        if self.embeds is not None and offset + n <= self.embeds.shape[1]:
            embeds = self.embeds[:, offset:offset + n]
            if self.per_layer is not None:
                per_layer = self.per_layer[:, offset:offset + n]
            step = self.blocks[offset:offset + n]
            if n > 1 and any(b >= 0 for b in step):
                self.step_blocks = step
        try:
            out = self.language(inputs, inputs_embeds=embeds, cache=cache, per_layer_inputs=per_layer, **outputs)
        finally:
            self.step_blocks = None
        if outputs:
            return out
        return out.logits if hasattr(out, "logits") else out


def image_blocks(tokens, image_token: int) -> list[int]:
    """For each token, the number of the image it belongs to (images are runs
    of the image token), -1 for text."""
    blocks, number, previous = [], -1, False
    for token in tokens:
        inside = token == image_token
        if inside and not previous:
            number += 1
        blocks.append(number if inside else -1)
        previous = inside
    return blocks


def image_masks(model, masks, h, cache, blocks):
    """The model's own masks for a step, with the step's image tokens seeing
    each other both ways, as Gemma 4 reads an image. The step's keys end
    with its own tokens, in the global and the sliding layers alike (a
    rotating cache appends the new keys after the ones it keeps), so the
    image block is laid on the mask's last columns."""
    if blocks is None:
        return masks
    from mlx_vlm.models.base import create_attention_mask
    ids = mx.array(blocks)
    same = (ids[:, None] == ids[None, :]) & (ids[:, None] >= 0)
    n = len(blocks)
    window = getattr(model, "window_size", None)
    built, out = {}, []
    for layer, c, mask in zip(model.layers, cache or [None] * len(masks), masks):
        kind = getattr(layer, "layer_type", "full_attention")
        if kind not in built:
            if not isinstance(mask, mx.array):
                mask = create_attention_mask(
                    h, c, window_size=window if kind == "sliding_attention" else None,
                    return_array=True)
            keys = mask.shape[-1]
            block = same if keys == n else mx.concatenate(
                [mx.zeros((n, keys - n), dtype=mx.bool_), same], axis=1)
            built[kind] = mask | block if mask.dtype == mx.bool_ else mx.where(block, mx.zeros_like(mask), mask)
        out.append(built[kind])
    return out


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


# Google's Gemma 4 chat template, published 2026-07-09 ("fixed tool-calling
# loops, turn closures, and thinking content-ordering"): one for every size
# (`google/gemma-4-*-it`, SHA-256 ae53464b...c4d6). The mlx-community packs
# still carry the one before it, so PWR renders with this one whatever the
# pack holds; the packs' files stay as their SHA-256 says.
GEMMA_TEMPLATE = r"""{#
  Template: Google Gemma 4 Canonical Chat Template
  Author: Google Gemma Engineering Team
  Published: 2026-07-09
  Context: Fixed tool-calling loops, turn closures, and thinking content-ordering.
#}
{%- macro format_parameters(properties, required, filter_keys=false) -%}
    {%- set standard_keys = ['description', 'type', 'properties', 'required', 'nullable'] -%}
    {%- set ns = namespace(found_first=false) -%}
    {%- for key, value in properties | dictsort -%}
        {%- set add_comma = false -%}
        {%- if not filter_keys or key not in standard_keys -%}
            {%- if ns.found_first %},{% endif -%}
            {%- set ns.found_first = true -%}
            {{ key }}:{
            {%- if value['description'] -%}
                description:<|"|>{{ value['description'] }}<|"|>
                {%- set add_comma = true -%}
            {%- endif -%}
            {%- if value['type'] | upper == 'STRING' -%}
                {%- if value['enum'] -%}
                    {%- if add_comma %},{%- else -%} {%- set add_comma = true -%} {% endif -%}
                    enum:{{ format_argument(value['enum']) }}
                {%- endif -%}
            {%- elif value['type'] | upper == 'ARRAY' -%}
                {%- if value['items'] is mapping and value['items'] -%}
                    {%- if add_comma %},{%- else -%} {%- set add_comma = true -%} {% endif -%}
                    items:{
                    {%- set ns_items = namespace(found_first=false) -%}
                    {%- for item_key, item_value in value['items'] | dictsort -%}
                        {%- if item_value is not none -%}
                            {%- if ns_items.found_first %},{% endif -%}
                            {%- set ns_items.found_first = true -%}
                            {%- if item_key == 'properties' -%}
                                properties:{
                                {%- if item_value is mapping -%}
                                    {{- format_parameters(item_value, value['items']['required'] | default([])) -}}
                                {%- endif -%}
                                }
                            {%- elif item_key == 'required' -%}
                                required:[
                                {%- for req_item in item_value -%}
                                    <|"|>{{- req_item -}}<|"|>
                                    {%- if not loop.last %},{% endif -%}
                                {%- endfor -%}
                                ]
                            {%- elif item_key == 'type' -%}
                                {%- if item_value is string -%}
                                    type:{{ format_argument(item_value | upper) }}
                                {%- else -%}
                                    type:{{ format_argument(item_value | map('upper') | list) }}
                                {%- endif -%}
                            {%- else -%}
                                {{ item_key }}:{{ format_argument(item_value) }}
                            {%- endif -%}
                        {%- endif -%}
                    {%- endfor -%}
                    }
                {%- endif -%}
            {%- endif -%}
            {%- if value['nullable'] %}
                {%- if add_comma %},{%- else -%} {%- set add_comma = true -%} {% endif -%}
                nullable:true
            {%- endif -%}
            {%- if value['type'] | upper == 'OBJECT' -%}
                {%- if value['properties'] is defined and value['properties'] is mapping -%}
                    {%- if add_comma %},{%- else -%} {%- set add_comma = true -%} {% endif -%}
                    properties:{
                    {{- format_parameters(value['properties'], value['required'] | default([])) -}}
                    }
                {%- elif value is mapping -%}
                    {%- if add_comma %},{%- else -%} {%- set add_comma = true -%} {% endif -%}
                    properties:{
                    {{- format_parameters(value, value['required'] | default([]), filter_keys=true) -}}
                    }
                {%- endif -%}
                {%- if value['required'] -%}
                    {%- if add_comma %},{%- else -%} {%- set add_comma = true -%} {% endif -%}
                    required:[
                    {%- for item in value['required'] | default([]) -%}
                        <|"|>{{- item -}}<|"|>
                        {%- if not loop.last %},{% endif -%}
                    {%- endfor -%}
                    ]
                {%- endif -%}
            {%- endif -%}
            {%- if add_comma %},{%- else -%} {%- set add_comma = true -%} {% endif -%}
            type:<|"|>{{ value['type'] | upper }}<|"|>}
        {%- endif -%}
    {%- endfor -%}
{%- endmacro -%}
{%- macro format_function_declaration(tool_data) -%}
    declaration:{{- tool_data['function']['name'] -}}{description:<|"|>{{- tool_data['function']['description'] -}}<|"|>
    {%- set params = tool_data['function']['parameters'] -%}
    {%- if params -%}
        ,parameters:{
        {%- if params['properties'] -%}
            properties:{ {{- format_parameters(params['properties'], params['required']) -}} },
        {%- endif -%}
        {%- if params['required'] -%}
            required:[
            {%- for item in params['required'] -%}
                <|"|>{{- item -}}<|"|>
                {{- ',' if not loop.last -}}
            {%- endfor -%}
            ],
        {%- endif -%}
        {%- if params['type'] -%}
            type:<|"|>{{- params['type'] | upper -}}<|"|>}
        {%- endif -%}
    {%- endif -%}
    {%- if 'response' in tool_data['function'] -%}
        {%- set response_declaration = tool_data['function']['response'] -%}
        ,response:{
        {%- if response_declaration['description'] -%}
            description:<|"|>{{- response_declaration['description'] -}}<|"|>,
        {%- endif -%}
        {%- if response_declaration['type'] | upper == 'OBJECT' -%}
            type:<|"|>{{- response_declaration['type'] | upper -}}<|"|>}
        {%- endif -%}
    {%- endif -%}
    }
{%- endmacro -%}
{%- macro format_argument(argument, escape_keys=True) -%}
    {%- if argument is none -%}
        {{- 'null' -}}
    {%- elif argument is string -%}
        {{- '<|"|>' + argument + '<|"|>' -}}
    {%- elif argument is boolean -%}
        {{- 'true' if argument else 'false' -}}
    {%- elif argument is mapping -%}
        {{- '{' -}}
        {%- set ns = namespace(found_first=false) -%}
        {%- for key, value in argument | dictsort -%}
            {%- if ns.found_first %},{% endif -%}
            {%- set ns.found_first = true -%}
            {%- if escape_keys -%}
                {{- '<|"|>' + key + '<|"|>' -}}
            {%- else -%}
                {{- key -}}
            {%- endif -%}
            :{{- format_argument(value, escape_keys=escape_keys) -}}
        {%- endfor -%}
        {{- '}' -}}
    {%- elif argument is sequence -%}
        {{- '[' -}}
        {%- for item in argument -%}
            {{- format_argument(item, escape_keys=escape_keys) -}}
            {%- if not loop.last %},{% endif -%}
        {%- endfor -%}
        {{- ']' -}}
    {%- else -%}
        {{- argument -}}
    {%- endif -%}
{%- endmacro -%}
{%- macro strip_thinking(text) -%}
    {%- set ns = namespace(result='') -%}
    {%- for part in text.split('<channel|>') -%}
        {%- if '<|channel>' in part -%}
            {%- set ns.result = ns.result + part.split('<|channel>')[0] -%}
        {%- else -%}
            {%- set ns.result = ns.result + part -%}
        {%- endif -%}
    {%- endfor -%}
    {{- ns.result | trim -}}
{%- endmacro -%}

{%- macro format_tool_response_block(tool_name, response) -%}
    {{- '<|tool_response>' -}}
    {%- if response is mapping -%}
        {{- 'response:' + tool_name + '{' -}}
        {%- for key, value in response | dictsort -%}
            {{- key -}}:{{- format_argument(value, escape_keys=False) -}}
            {%- if not loop.last %},{% endif -%}
        {%- endfor -%}
        {{- '}' -}}
    {%- else -%}
        {{- 'response:' + tool_name + '{value:' + format_argument(response, escape_keys=False) + '}' -}}
    {%- endif -%}
    {{- '<tool_response|>' -}}
{%- endmacro -%}

{#- ===== SETUP ===== -#}
{%- set ns = namespace(prev_message_type=None, prev_non_tool_role=None) -%}
{%- set loop_messages = messages -%}
{%- set enable_thinking = enable_thinking | default(false) -%}
{%- set preserve_thinking = preserve_thinking | default(false) -%}
{{- bos_token -}}
{#- Handle System/Tool Definitions Block -#}
{%- if enable_thinking or tools or (messages and messages[0]['role'] in ['system', 'developer']) -%}
    {{- '<|turn>system\n' -}}
    {#- Inject Thinking token at the very top of the FIRST system turn -#}
    {%- if enable_thinking -%}
        {{- '<|think|>\n' -}}
        {%- set ns.prev_message_type = 'think' -%}
    {%- endif -%}
    {%- if messages and messages[0]['role'] in ['system', 'developer'] -%}
        {%- if messages[0]['content'] is string -%}
            {{- messages[0]['content'] | trim -}}
        {%- elif messages[0]['content'] is sequence -%}
            {%- for item in messages[0]['content'] -%}
                {{- item['text'] | trim + ' '-}}
            {%- endfor -%}
        {%- endif -%}
        {%- set loop_messages = messages[1:] -%}
    {%- endif -%}
    {%- if tools -%}
        {%- for tool in tools %}
            {{- '<|tool>' -}}
            {{- format_function_declaration(tool) | trim -}}
            {{- '<tool|>' -}}
        {%- endfor %}
        {%- set ns.prev_message_type = 'tool' -%}
    {%- endif -%}
    {{- '<turn|>\n' -}}
{%- endif %}

{#- Pre-scan: find last user message index for reasoning guard -#}
{%- set ns_turn = namespace(last_user_idx=-1) -%}
{%- for i in range(loop_messages | length) -%}
    {%- if loop_messages[i]['role'] == 'user' -%}
        {%- set ns_turn.last_user_idx = i -%}
    {%- endif -%}
{%- endfor -%}

{#- Loop through messages -#}
{%- for message in loop_messages -%}
    {%- if message['role'] != 'tool' -%}
    {%- set ns.prev_message_type = None -%}
    {%- set role = 'model' if message['role'] == 'assistant' else message['role'] -%}
    {#- Detect continuation using tracked state — O(1) instead of O(n) backward scan -#}
    {%- set continue_same_model_turn = (role == 'model' and ns.prev_non_tool_role == 'assistant') -%}
    {%- if not continue_same_model_turn -%}
        {{- '<|turn>' + role + '\n' }}

    {%- endif -%}

    {#- Render reasoning/reasoning_content as thinking channel -#}
    {%- set thinking_text = message.get('reasoning') or message.get('reasoning_content') -%}
    {%- set thinking_gate = (loop.index0 > ns_turn.last_user_idx) or (preserve_thinking and message.get('tool_calls')) -%}
    {%- if thinking_text and thinking_gate -%}
        {{- '<|channel>thought\n' + thinking_text + '\n<channel|>' -}}
    {%- endif -%}

            {%- if message.get('tool_calls') -%}
                {%- for tool_call in message.get('tool_calls') -%}
                    {%- set function = tool_call['function'] -%}
                    {{- '<|tool_call>call:' + function['name'] + '{' -}}
                    {%- if function['arguments'] is mapping -%}
                        {%- set ns_args = namespace(found_first=false) -%}
                        {%- for key, value in function['arguments'] | dictsort -%}
                            {%- if ns_args.found_first %},{% endif -%}
                            {%- set ns_args.found_first = true -%}
                            {{- key -}}:{{- format_argument(value, escape_keys=False) -}}
                        {%- endfor -%}
                    {%- elif function['arguments'] is none -%}
                    {%- else -%}
                        {{- raise_exception(
                            "chat_template: tool_calls[].function.arguments must be a "
                            "JSON object (mapping), not a string. Deserialize arguments "
                            "before passing to the template."
                        ) -}}
                    {%- endif -%}
                    {{- '}<tool_call|>' -}}
                {%- endfor -%}
                {%- set ns.prev_message_type = 'tool_call' -%}
            {%- endif -%}

            {%- set ns_tr_out = namespace(flag=false) -%}
            {%- if message.get('tool_responses') -%}
                {#- Legacy: tool_responses embedded on the assistant message (Google/Gemma native) -#}
                {%- for tool_response in message.get('tool_responses') -%}
                    {{- format_tool_response_block(tool_response['name'] | default('unknown', true), tool_response['response']) -}}
                    {%- set ns_tr_out.flag = true -%}
                    {%- set ns.prev_message_type = 'tool_response' -%}
                {%- endfor -%}
            {%- elif message.get('tool_calls') -%}
                {#- OpenAI Chat Completions: forward-scan consecutive role:tool messages -#}
                {%- set ns_tool_scan = namespace(stopped=false) -%}
                {%- for k in range(loop.index0 + 1, loop_messages | length) -%}
                    {%- if ns_tool_scan.stopped -%}
                    {%- elif loop_messages[k]['role'] != 'tool' -%}
                        {%- set ns_tool_scan.stopped = true -%}
                    {%- else -%}
                        {%- set follow = loop_messages[k] -%}
                        {#- Resolve tool_call_id to function name -#}
                        {%- set ns_tname = namespace(name=follow.get('name') or 'unknown') -%}
                        {%- for tc in message.get('tool_calls') -%}
                            {%- if tc.get('id') == follow.get('tool_call_id') -%}
                                {%- set ns_tname.name = tc['function']['name'] -%}
                            {%- endif -%}
                        {%- endfor -%}
                        {#- Handle content as string or content-parts array -#}
                        {%- set tool_body = follow.get('content') -%}
                        {%- if tool_body is string -%}
                            {{- format_tool_response_block(ns_tname.name, tool_body) -}}
                        {%- elif tool_body is sequence and tool_body is not string -%}
                            {%- set ns_txt = namespace(s='') -%}
                            {%- for part in tool_body -%}
                                {%- if part.get('type') == 'text' -%}
                                    {%- set ns_txt.s = ns_txt.s + (part.get('text') | default('')) -%}
                                {%- endif -%}
                            {%- endfor -%}
                            {{- format_tool_response_block(ns_tname.name, ns_txt.s) -}}
                            {%- for part in tool_body -%}
                                {%- if part.get('type') in ['image', 'image_url'] -%}
                                    {{- '<|image|>' -}}
                                {%- elif part.get('type') in ['audio', 'input_audio'] -%}
                                    {{- '<|audio|>' -}}
                                {%- elif part.get('type') == 'video' -%}
                                    {{- '<|video|>' -}}
                                {%- endif -%}
                            {%- endfor -%}
                        {%- else -%}
                            {{- format_tool_response_block(ns_tname.name, tool_body) -}}
                        {%- endif -%}
                        {%- set ns_tr_out.flag = true -%}
                        {%- set ns.prev_message_type = 'tool_response' -%}
                    {%- endif -%}
                {%- endfor -%}
            {%- endif -%}

            {%- set captured_content -%}
            {%- if message.get('content') is string -%}
                {%- if role == 'model' -%}
                    {{- strip_thinking(message['content']) -}}
                {%- else -%}
                    {{- message['content'] | trim -}}
                {%- endif -%}
            {%- elif message.get('content') is sequence -%}
                {%- for item in message['content'] -%}
                    {%- if item.get('type') == 'text' -%}
                        {%- if role == 'model' -%}
                            {{- strip_thinking(item['text']) -}}
                        {%- else -%}
                            {{- item['text'] | trim -}}
                        {%- endif -%}
                    {%- elif item.get('type') in ['image', 'image_url'] -%}
                        {{- '<|image|>' -}}
                    {%- elif item.get('type') in ['audio', 'input_audio'] -%}
                        {{- '<|audio|>' -}}
                    {%- elif item.get('type') == 'video' -%}
                        {{- '<|video|>' -}}
                    {%- endif -%}
                {%- endfor -%}
            {%- endif -%}
            {%- endset -%}

            {{- captured_content -}}
            {%- set has_content = captured_content | trim | length > 0 -%}

        {#- Forward-scan: find next non-tool message role for continuation detection -#}
        {%- set next_nt = namespace(role=None, found=false) -%}
        {%- for j in range(loop.index0 + 1, loop_messages | length) -%}
            {%- if not next_nt.found -%}
                {%- if loop_messages[j]['role'] != 'tool' -%}
                    {%- set next_nt.role = loop_messages[j]['role'] -%}
                    {%- set next_nt.found = true -%}
                {%- endif -%}
            {%- endif -%}
        {%- endfor -%}

        {%- set continues_into_next = (
            role == 'model'
            and next_nt.role == 'assistant'
            and (not message.get('tool_calls') or ns_tr_out.flag)
        ) -%}

        {%- if ns.prev_message_type == 'tool_call' and not ns_tr_out.flag -%}
            {{- '<|tool_response>' -}}
        {%- elif continues_into_next -%}
        {%- elif not (ns_tr_out.flag and not has_content and not next_nt.found) -%}
            {{- '<turn|>\n' -}}
        {%- endif -%}

    {#- Track previous non-tool role for next iteration (avoids O(n) backward scan) -#}
    {%- set ns.prev_non_tool_role = message['role'] -%}
    {%- endif -%}
{%- endfor -%}

{%- if add_generation_prompt -%}
    {%- if ns.prev_message_type != 'tool_response' and ns.prev_message_type != 'tool_call' -%}
        {{- '<|turn>model\n' -}}
        {%- if not enable_thinking -%}
            {{- '<|channel>thought\n<channel|>' -}}
        {%- endif -%}
    {%- elif ns.prev_message_type == 'tool_response' and enable_thinking -%}
        {{- '<|channel>thought\n' -}}
    {%- endif -%}
{%- endif -%}
"""

# Where the template puts what a reply wrote beside its calls.
GEMMA_CALLS = "{%- if message.get('tool_calls') -%}"
GEMMA_CONTENT = "{%- if message.get('content') is string -%}"
GEMMA_PARTS = "{%- elif message.get('content') is sequence -%}"


def gemma_history_template(template: str) -> str:
    """Gemma 4's template, rendering a history the way the model writes one.

    The text a reply has beside its calls is what the model wrote before
    them; the template puts it after the calls' results, where Gemma writes
    the answer that follows them. Here the text goes before the calls. (The
    turn left open on results when a user message follows, which PWR closed
    here too, Google's July template closes itself.) A template without these
    passages is left as it is."""
    if not all(template.count(part) == 1 for part in (GEMMA_CALLS, GEMMA_CONTENT, GEMMA_PARTS)):
        return template
    before = (
        "{%- if role == 'model' and message.get('tool_calls') and message.get('content') is string -%}"
        "{{- strip_thinking(message['content']) -}}"
        "{%- endif -%}\n            " + GEMMA_CALLS)
    content = "{%- if message.get('content') is string and not (role == 'model' and message.get('tool_calls')) -%}"
    # The text moved before the calls is still a string here: not parts.
    parts = "{%- elif message.get('content') is sequence and message.get('content') is not string -%}"
    return (template.replace(GEMMA_CALLS, before).replace(GEMMA_CONTENT, content)
            .replace(GEMMA_PARTS, parts))


def tool_result_mapping(text: str):
    """A PWR tool result as the mapping it is: the JSON it begins with -- the
    `result` object itself when that is all it holds -- with verbatim file
    text and command streams restored to the fields their placeholders name.
    Unknown or incomplete suffixes stay text, not invented field values.
    None when the text is not one."""
    try:
        head, end = json.JSONDecoder().raw_decode(text)
    except ValueError:
        return None
    if not isinstance(head, dict):
        return None
    mapping = head["result"] if list(head) == ["result"] and isinstance(head["result"], dict) else head
    mapping = {k: v for k, v in mapping.items() if v is not None}
    rest = text[end:].strip()
    fields = mapping.get("result") if isinstance(mapping.get("result"), dict) else mapping
    remaining, restored = rest, {}
    while remaining:
        tag = next((key for key in ("content", "stdout", "stderr")
                    if fields.get(key) == f"<{key}> below"
                    and key not in restored and remaining.startswith(f"<{key}>\n")), None)
        if tag is None:
            break
        opening, closing = f"<{tag}>\n", f"\n</{tag}>"
        at = remaining.find(closing, len(opening))
        if at < 0:
            # A malformed envelope is kept whole, even if an earlier section
            # looked complete. Do not turn partial text into observed output.
            remaining, restored = rest, {}
            break
        restored[tag] = remaining[len(opening):at]
        remaining = remaining[at + len(closing):].strip()
    fields.update(restored)
    if remaining:
        mapping["text"] = remaining
    return mapping


def gemma_tool_results(messages):
    """The history with each call's results given to Gemma 4 as mappings.

    Gemma reads a result as `response:NAME{key:value,...}`. PWR's results are
    JSON with the file text after it; written as one string, the model reads
    them as `response:NAME{value:"{\\"result\\":...}"}`, quotes escaped. The
    template takes a mapping only as `tool_responses` on the reply holding the
    calls, so the results move there; a reply whose results are not all such
    text (an image) keeps them as they are."""
    out, index = [], 0
    while index < len(messages):
        message = messages[index]
        index += 1
        if message.get("role") != "assistant" or not message.get("tool_calls"):
            out.append(message)
            continue
        follow = []
        while index + len(follow) < len(messages) and messages[index + len(follow)].get("role") == "tool":
            follow.append(messages[index + len(follow)])
        mappings = [tool_result_mapping(f["content"]) if isinstance(f.get("content"), str) else None
                    for f in follow]
        if not follow or any(m is None for m in mappings):
            out.append(message)
            continue
        names = {call.get("id"): call.get("function", {}).get("name") for call in message["tool_calls"]}
        responses = [{"name": names.get(f.get("tool_call_id")) or f.get("name") or "unknown", "response": m}
                     for f, m in zip(follow, mappings)]
        out.append({**message, "tool_responses": responses})
        index += len(follow)
    return out


class PrefillCancelled(Exception):
    """An interrupted prefill has no reusable cache state."""


def load_assistant(vlm, path, on=False):
    """The pack's assistant ready to draft for `vlm`, or None and why not.

    `on` is the person's switch in the app, sent with the load."""
    if not (ASSISTANT or on):
        return None, "off (PWR_MLX_ASSISTANT)"
    if KV_BITS:
        # It reads the global layers' keys and values as they are kept.
        return None, f"the global layers' cache is held at {KV_BITS} bits"
    try:
        from mlx_vlm.speculative.drafters import load_drafter, validate_drafter_compatibility
        drafter, kind = load_drafter(str(path), kind="mtp")
        validate_drafter_compatibility(vlm, drafter, kind)
        return drafter, None
    except Exception as error:
        return None, f"{type(error).__name__}: {error}"[:300]


class Response:
    """One step of a speculative reply, shaped as mlx-lm's."""

    def __init__(self, text: str, finish_reason=None):
        self.text, self.finish_reason = text, finish_reason


class ThoughtSampler:
    """Apply the thought constraint to target positions, never to draft state.

    MTP may sample a whole verified block before yielding its first token.
    Updating a mask when tokens are yielded is too late for that block. A
    rejected suffix is overwritten when verification resumes at its position.
    Markers in earlier conversation turns do not belong to this reply.
    """

    def __init__(self, sampler, thought, prompt_tokens):
        self.sampler, self.thought = sampler, thought
        boundary = prompt_tokens[-1] if prompt_tokens else None
        self.states = {0: (thought.shut or not thought.allow_thought or boundary in (thought.open_id, thought.close_id),
                           thought.closed or not thought.allow_thought or boundary == thought.close_id)} if thought else {0: (False, False)}

    def __call__(self, logits):
        # Proposals cannot modify the target's history. Verification applies
        # the constraint and rejects any proposal that violates it.
        return self.sampler(logits)

    def sample_target(self, logits, *, row_ids, positions):
        if any(row != 0 for row in row_ids):
            raise ValueError("PWR speculative sampling serves one conversation")
        sampled = []
        for row, position in enumerate(positions):
            shut, closed = self.states[position]
            scores = logits[row:row + 1]
            if self.thought is not None:
                ids = mx.arange(scores.shape[-1])
                forbidden = ((ids == self.thought.open_id) & shut) | ((ids == self.thought.close_id) & closed)
                scores = mx.where(forbidden, -mx.inf, scores)
            token = self.sampler(scores)
            value = int(token.item())
            if self.thought is not None:
                shut = shut or value in (self.thought.open_id, self.thought.close_id)
                closed = closed or value == self.thought.close_id
            self.states[position + 1] = (shut, closed)
            sampled.append(token.reshape(-1))
        return mx.concatenate(sampled)


class ConstrainedTarget:
    """Keep MTP on its sampler-aware verifier, including at temperature zero.

    The fused argmax shortcut bypasses logits processors. Everything else,
    including the model's exact cache transaction, stays with mlx-vlm.
    """

    def __init__(self, model):
        self._target = model

    def __getattr__(self, name):
        if name in ("speculative_argmax_from_hidden", "speculative_verify_logits"):
            raise AttributeError(name)
        return getattr(self._target, name)


def unbuffered(cache) -> list:
    """The cache with its sliding layers back to the kind it was made with.

    The speculative rounds hold a sliding layer as mlx-vlm's buffered kind,
    which keeps a few rejected tokens' room. The copy of a prompt was taken
    of the plain kind and the next turn puts it back there: the layer is
    given back empty, to be restored from the copy or made anew."""
    from mlx_vlm.models.cache import BufferedRotatingKVCache, RotatingKVCache
    return [RotatingKVCache(max_size=c.max_size, keep=c.keep) if isinstance(c, BufferedRotatingKVCache) else c
            for c in cache]


def model_template(path: pathlib.Path, config: dict) -> str:
    """Edge checkpoints have a native prefix without the large-model empty thought."""
    text = config.get("text_config", config)
    if text.get("max_position_embeddings") == 131_072:
        return (path / "chat_template.jinja").read_text()
    return GEMMA_TEMPLATE


class Engine:
    def __init__(self) -> None:
        self.model = None
        self.template = GEMMA_TEMPLATE
        self.tokenizer = None
        self.processor = None   # set when the model was loaded with its vision encoder
        self.path = None
        self.cache = None
        # Tokens whose state the checkpoint holds: the last prompt, without its
        # generation prompt, so the next turn's history can start from it.
        self.checkpoint = None
        self.checkpoint_tokens: list[int] = []
        self.operation = {}
        self.drafter = None

    def observe(self, phase: str, **fields) -> None:
        # Sample before GPU work: after a Metal failure querying/evaluating the
        # damaged device can itself fail. No prompt text enters this record.
        self.operation = {"phase": phase, "started": time.perf_counter(), **fields,
                          "active_memory_bytes": int(mx.get_active_memory()),
                          "cache_memory_bytes": int(mx.get_cache_memory()),
                          "peak_memory_bytes": int(mx.get_peak_memory())}
        trace({"event": "operation_begin", **self.diagnostics()})

    def diagnostics(self) -> dict:
        operation = dict(getattr(self, "operation", {}))
        began = operation.pop("started", None)
        if began is not None:
            operation["elapsed_secs"] = round(time.perf_counter() - began, 3)
        operation["agx_relax_context_timeout"] = os.environ.get("AGX_RELAX_CDM_CTXSTORE_TIMEOUT")
        return operation

    def load(self, request: dict) -> dict:
        self.observe("load")
        path = pathlib.Path(request["path"]).expanduser()
        config = json.loads((path / "config.json").read_text())
        model_type = str(config.get("model_type", ""))
        if not model_type.startswith("gemma4") or "vision_config" not in config:
            raise RuntimeError(f"PWR runs Gemma 4 only; {path.name} is {model_type or 'of no known type'}")
        if not vlm_available():
            raise RuntimeError("mlx-vlm and Pillow are needed to load Gemma 4")
        self.model = self.tokenizer = self.processor = self.drafter = None
        self.chunk_limit = PREFILL_FIRST_STEP
        mx.clear_cache()
        from mlx_vlm import load as load_vision
        vlm, self.processor = load_vision(str(path))
        self.model = VisionText(vlm)
        self.tokenizer = load_tokenizer(path, NO_REMOTE_CODE)
        self.template = model_template(path, config)
        self.tokenizer.chat_template = self.template
        add_turn_ends(self.tokenizer)
        self.path = path
        self.cache = None
        self.checkpoint = None
        self.checkpoint_tokens = []
        text = config.get("text_config", config)
        self.heads = int(text.get("num_attention_heads") or 64)
        head_dim = int(text.get("head_dim") or text["hidden_size"] // self.heads)
        self.score_budget = score_budget()
        self.fused = fused_attention(head_dim)
        weights = sum(p.stat().st_size for p in path.glob("*.safetensors"))
        assistant_error = None
        if request.get("assistant"):
            self.drafter, assistant_error = load_assistant(
                vlm, request["assistant"], bool(request.get("assistant_on")))
        return {
            "event": "loaded",
            "model_type": model_type,
            "weights_bytes": weights,
            "vision": True,
            "rope_scaling": None,
            "assistant": self.drafter is not None,
            "assistant_error": assistant_error,
        }

    def render(self, messages, tools, thinking, generation_prompt: bool):
        """The prompt's tokens, and the processed images when it has any."""
        template = self.template
        images, messages = image_parts(messages, accept=self.processor is not None)
        messages = gemma_tool_results(messages)
        kwargs = {
            "add_generation_prompt": generation_prompt,
            "tokenize": not images,
            "chat_template": gemma_history_template(template),
        }
        if tools:
            kwargs["tools"] = tools
        if thinking is not None:
            kwargs["enable_thinking"] = bool(thinking)
        rendered = self.tokenizer.apply_chat_template(messages, **kwargs)
        if not images:
            return list(rendered), None
        # The processor expands each image placeholder to the image's soft
        # tokens; the same image expands the same way every turn, so the
        # prompt cache still matches the history.
        budget_kwargs = {}
        if IMAGE_TOKENS:
            budget_kwargs["max_soft_tokens"] = IMAGE_TOKENS
        processed = self.processor(text=[rendered], images=[image for _, image in images],
                                   return_tensors="np", **budget_kwargs)
        tokens = [int(token) for token in processed["input_ids"][0]]
        digest = hashlib.sha256(b"".join(hashlib.sha256(raw).digest() for raw, _ in images))
        if budget_kwargs:
            digest.update(str(IMAGE_TOKENS).encode())
        arrays = {k: v for k, v in processed.items() if k not in ("input_ids", "attention_mask")}
        return tokens, {"arrays": arrays, "digest": digest.hexdigest()}

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
        step = self.score_budget // (self.heads * (BUDGET_SCORE_BYTES if SCORES_MB else SCORE_BYTES) * keys)
        return max(PREFILL_MIN_STEP, min(PREFILL_STEP, step // PREFILL_MIN_STEP * PREFILL_MIN_STEP))

    def prefill(self, tokens: list[int], offset: int, progress=lambda done, total: None, cancelled=lambda: False) -> None:
        start = 0
        while start < len(tokens):
            if cancelled():
                raise PrefillCancelled()
            progress(start, len(tokens))
            step = min(self.prefill_step(offset + start),
                       getattr(self, "chunk_limit", PREFILL_FIRST_STEP))
            step = self.model.whole_images(offset + start, offset + start + step) - offset - start
            chunk = tokens[start:start + step]
            self.observe("prefill", offset=offset + start, chunk_tokens=len(chunk))
            began = time.perf_counter()
            self.model(mx.array(chunk)[None], cache=self.cache)
            mx.eval([c.state for c in self.cache])
            took = time.perf_counter() - began
            if took > 0:
                target = (PREFILL_CHUNK_SECS if getattr(self, "fused", True)
                          else PREFILL_UNFUSED_CHUNK_SECS)
                fitting = int(len(chunk) / took * target) // PREFILL_MIN_STEP * PREFILL_MIN_STEP
                self.chunk_limit = max(PREFILL_MIN_STEP, min(PREFILL_STEP, step * 2, fitting))
            trace({"event": "prefill_chunk", **self.diagnostics(),
                   "duration_secs": round(took, 3),
                   "active_memory_after_bytes": int(mx.get_active_memory()),
                   "cache_memory_after_bytes": int(mx.get_cache_memory())})
            start += len(chunk)
        progress(len(tokens), len(tokens))
        mx.clear_cache()

    def fresh_cache(self):
        return quantized(make_prompt_cache(self.model), KV_BITS)

    def resume(self, base: list[int], progress, cancelled=lambda: False) -> int:
        """Brings the cache to the end of `base`, reusing what it can of the
        last prompt, and returns how many tokens were reused."""
        reused = 0
        if self.checkpoint is not None and base[:len(self.checkpoint_tokens)] == self.checkpoint_tokens:
            self.observe("restore", prompt_tokens=len(base))
            try:
                restore(self.cache, self.checkpoint)
                reused = len(self.checkpoint_tokens)
            except ValueError:
                # A copy the cache cannot be put back to: start from nothing.
                self.cache = self.fresh_cache()
                self.checkpoint = None
        else:
            if TRACE and self.checkpoint_tokens:
                # Where the new prompt stopped being the old one, in text: the
                # one question a cache miss raises, answered only when tracing.
                at = common_prefix(self.checkpoint_tokens, base)
                decode = self.tokenizer.decode
                trace({"cache_miss_at": at, "checkpoint_tokens": len(self.checkpoint_tokens),
                       "prompt_tokens": len(base),
                       "was": decode(self.checkpoint_tokens[max(0, at - 40):at + 80]),
                       "now": decode(base[max(0, at - 40):at + 80])})
            self.cache = self.fresh_cache()
            self.checkpoint = None
        # Gemma's sliding-window layers cannot be cut back once full: the
        # prompt's state is copied, to be restored when the next prompt
        # extends this one.
        cut = max(reused, self.checkpoint_point(base))
        cut = max(reused, self.model.whole_images(reused, cut))
        try:
            self.prefill(base[reused:cut], reused, progress, cancelled)
            self.observe("checkpoint", prompt_tokens=cut)
            self.checkpoint = snapshot(self.cache)
            if cut < len(base):
                self.prefill(base[cut:], cut, progress, cancelled)
        except BaseException:
            # Half a prefill matches no prompt: the next one starts clean.
            self.cache = None
            self.checkpoint = None
            self.checkpoint_tokens = []
            raise
        self.checkpoint_tokens = base[:cut]
        return reused

    def checkpoint_point(self, tokens: list[int]) -> int:
        """Where the copy of a prompt's state is taken: just before the
        end-of-turn marker that closes its last message.

        The copy is only of use to a next prompt that begins with it, and the
        marker closing the last message is not always there in the next one:
        a model turn ended on tool results is closed only when a user message
        follows it (`gemma_history_template`). Measured 2026-09-26 on another
        template, a copy taken after such a marker stopped being a prefix and
        the whole conversation was prefilled again, eight times in one
        ninety-minute run. Taken before it, it costs the marker's tokens."""
        marks = set(getattr(self.tokenizer, "eos_token_ids", None) or [])
        for index in range(len(tokens) - 1, max(-1, len(tokens) - 64), -1):
            if tokens[index] in marks:
                return index
        return len(tokens)

    def speculate(self, prompt_tokens, max_tokens, sampler, greedy: bool, thought):
        """A reply written with the assistant, yielded as `stream_generate`
        yields one: mlx-vlm's speculative rounds on this engine's cache, after
        the last prompt tokens are read with the hidden state and keys the
        assistant drafts from. The rounds apply no logits processors, so the
        thought channel is held to one opening (`OneThought`) by the sampler."""
        from mlx_vlm.speculative.utils import run_speculative_rounds
        tokens = [int(t) for t in prompt_tokens]
        sample = ThoughtSampler(sampler, thought, tokens)

        inputs = mx.array(tokens)[None]
        start = int(self.cache[0].offset)
        out = self.model(inputs, cache=self.cache, return_hidden=True, return_shared_kv=True)
        logits = out.logits[:, -1, :]
        logprobs = logits - mx.logsumexp(logits, axis=-1, keepdims=True)
        first = sample.sample_target(logprobs, row_ids=[0], positions=[0])
        detokenizer = self.tokenizer.detokenizer
        detokenizer.reset()
        ends = set(self.tokenizer.eos_token_ids)
        written = []
        try:
            for token, _ in run_speculative_rounds(
                    ConstrainedTarget(self.model.language), self.drafter, self.cache, inputs, first, logprobs, out,
                    draft_kind="mtp", max_tokens=max_tokens, sampler=sample,
                    draft_block_size=DRAFT_BLOCK, sampler_is_greedy=greedy):
                token = int(token)
                if token in ends:
                    detokenizer.finalize()
                    yield Response(detokenizer.last_segment, "stop")
                    return
                written.append(token)
                detokenizer.add_token(token)
                if len(written) >= max_tokens:
                    detokenizer.finalize()
                    yield Response(detokenizer.last_segment, "length")
                    return
                yield Response(detokenizer.last_segment)
        finally:
            if thought is not None:
                thought.shut, thought.closed = sample.states[len(written)]
            self.settle(start + len(tokens), written)

    def settle(self, prompt_end: int, written: list) -> None:
        """The cache holding the prompt and exactly the tokens written, as one
        written a token at a time holds them, so a reply forced on (the
        reasoning budget) goes on from what was written. A round commits its
        accepted tokens before they are given out, and the last token given
        out is not yet read: a reply stopped inside a round holds a few more,
        one stopped after it one fewer."""
        target = prompt_end + len(written)
        held = int(self.cache[0].offset)
        if held > target:
            for layer in self.cache:
                layer.trim(held - target)
        elif held == target - 1 and written:
            mx.eval(self.model(mx.array([[written[-1]]]), cache=self.cache))

    def drafted(self, before):
        """What the assistant drafted in this reply, or None without it."""
        if before is None:
            return None
        now = {name: getattr(self.drafter, f"speculative_total_{name}", 0) - before[name]
               for name in ("rounds", "accepted", "drafted")}
        return {"rounds": int(now["rounds"]), "drafted": int(now["drafted"]), "accepted": round(float(now["accepted"]), 1)}

    def chat(self, request: dict, reply, cancelled=lambda: False) -> dict:
        """One request. An `aside` -- a wiki summary written while the person
        reads the last answer -- runs on a cache of its own and leaves the
        conversation's where it was. Served on the conversation's, it replaced
        the one copy there is: measured 2026-09-26, the person's next message
        after each summary prefilled a 66k- and a 75k-token conversation from
        nothing, 205 s and 271 s, where the goal's own next turn resumed in 8 s."""
        if not request.get("aside"):
            return self.converse(request, reply, cancelled)
        kept = (self.cache, self.checkpoint, self.checkpoint_tokens)
        self.cache, self.checkpoint, self.checkpoint_tokens = None, None, []
        try:
            return self.converse(request, reply, cancelled)
        finally:
            self.cache, self.checkpoint, self.checkpoint_tokens = kept

    def converse(self, request: dict, reply, cancelled=lambda: False) -> dict:
        if self.model is None:
            raise RuntimeError("no model loaded")
        messages = request["messages"]
        tools = request.get("tools")
        thinking = request.get("thinking")
        budget = request.get("reasoning_budget")
        max_tokens = int(request.get("max_tokens") or 8192)
        template = self.template
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
        # mlx-lm penalises only the last 20 tokens by default. A vendor's
        # presence penalty (Qwen's 1.5) is defined over everything generated
        # so far, and a loop of a passage longer than 20 tokens is invisible to
        # a 20-token window: `presence_context_size` widens it where the
        # caller asks (PWR does after a reply has looped).
        logits_processors = make_logits_processors(
            presence_penalty=request.get("presence_penalty"),
            presence_context_size=int(request.get("presence_context_size") or 20),
            repetition_penalty=request.get("repetition_penalty"),
        )
        thought = one_thought(self.tokenizer, template)
        # With the assistant unless the request asks for penalties, which the
        # speculative rounds cannot apply.
        speculative = (self.drafter is not None and not request.get("presence_penalty")
                       and not request.get("repetition_penalty"))
        greedy = not float(request.get("temperature") or 0.0)
        drafted_before = {name: getattr(self.drafter, f"speculative_total_{name}", 0)
                          for name in ("rounds", "accepted", "drafted")} if speculative else None
        if thought is not None:
            logits_processors = [*(logits_processors or []), thought]

        started = time.perf_counter()
        self.observe("render")
        full, images = self.render(messages, tools, thinking, True)
        context_tokens = request.get("context_tokens")
        if context_tokens is not None:
            context_tokens = int(context_tokens)
            # Count the actual template, schema and image-expanded token vector
            # before touching the model or its KV cache.
            if context_tokens <= 0 or len(full) >= context_tokens:
                return {"event": "error", "code": "context_limit",
                        "message": "rendered prompt leaves no generation space",
                        "prompt_tokens": len(full), "context_tokens": context_tokens}
            max_tokens = min(max_tokens, context_tokens - len(full))
            if budget is not None:
                budget = max(0, min(budget, max_tokens - FINAL_RESERVE))
        base, _ = self.render(messages, tools, thinking, False)
        if full == base:
            # A model turn going on after a tool result (Gemma 4).
            full = full + empty_thought(self.tokenizer, template, thinking)
        if PROMPT_DUMP:
            dump_prompt(request, full, self.tokenizer)
        self.observe("vision", prompt_tokens=len(full))
        self.model.set_prompt(full, images, images and images["digest"])
        if len(full) == len(base) or full[:len(base)] != base:
            # The template adds no generation token after a tool result: the
            # model goes on with its own turn. Generation still needs one
            # prompt token.
            base = full[:-1]
        try:
            reused = self.resume(base, lambda done, total: reply(
                {"event": "prefill", "processed": int(done), "total": int(total)}
            ), cancelled)
        except PrefillCancelled:
            return {"event": "done", "finish_reason": "cancelled",
                    "usage": {"prompt_tokens": len(full), "completion_tokens": 0}}
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
        forced_tokens = 0
        written = []          # everything generated, both channels, for the loop check
        written_len = 0
        repetition = RepetitionSignals()

        def stream(prompt_tokens, limit):
            nonlocal generated, finish, written_len
            self.observe("generation", prompt_tokens=len(full), generated_tokens=generated)
            responses = self.speculate(prompt_tokens, limit, sampler, greedy, thought) if speculative else stream_generate(
                self.model, self.tokenizer, mx.array(prompt_tokens), max_tokens=limit,
                sampler=sampler, logits_processors=logits_processors,
                prompt_cache=self.cache,
                # A long prefill says it is alive, so PWR can tell one
                # from an engine stuck in an evaluation.
                prompt_progress_callback=lambda done, total: reply(
                    {"event": "prefill", "processed": int(done), "total": int(total)}
                ),
            )
            try:
                return read(responses)
            finally:
                if speculative:
                    responses.close()

        def read(responses):
            nonlocal generated, finish, written_len
            for response in responses:
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
                # Gemma's stops are among the tokenizer's (`add_turn_ends`);
                # should one come through as text all the same, the reply
                # ends there.
                if any(marker in "".join(written[-12:]) for marker in TURN_ENDS):
                    finish = "stop"
                    return finish
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
                forced = list(self.tokenizer.encode(delimiters[1] + CLOSE_SUFFIX,
                                                    add_special_tokens=False))
                remaining = max_tokens - generated - forced_tokens - len(forced)
                if remaining <= 0:
                    break
                forced_tokens += len(forced)
                # Close the reasoning with the template's own delimiter and let
                # the answer follow, inside what is left of the cap.
                finalizations += 1
                if thought is not None:
                    thought.shut = thought.closed = True
                for channel, piece in tracker.force_close():
                    reply({"event": "delta", "channel": channel, "text": piece})
                reply({"event": "finalizing", "reasoning_tokens": tracker.reasoning_tokens})
                finish = "length"
                outcome = stream(forced, remaining)
        finally:
            if speculative and self.cache is not None:
                self.cache = unbuffered(self.cache)
        for channel, piece in tracker.flush():
            reply({"event": "delta", "channel": channel, "text": piece})
        if (finalizations or outcome == "budget") and finish not in ("cancelled", "repetition") and (
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
               "assistant": self.drafted(drafted_before),
               "peak_memory_gb": round(mx.get_peak_memory() / 1e9, 2),
               "active_memory_gb": round(mx.get_active_memory() / 1e9, 2),
               "cache_memory_gb": round(mx.get_cache_memory() / 1e9, 2)})
        # A generation's buffers are not the next one's: given back now.
        mx.clear_cache()
        return {
            "event": "done",
            "finish_reason": finish if finish in (
                "stop", "repetition", "cancelled", "reasoning_unfinished") else "length",
            "budget_forced": finalizations > 0,
            "assistant": self.drafted(drafted_before),
            "repetition_signals": repetition.summary(),
            # Whether the reasoning counts below are the engine's own: only
            # where the template's delimiters let it see the phase.
            "reasoning_tracked": delimiters is not None,
            "usage": {
                "prompt_tokens": len(full),
                "cached_tokens": reused,
                "prefilled_tokens": len(base) - reused,
                "completion_tokens": generated + forced_tokens,
                "forced_tokens": forced_tokens,
                "reasoning_tokens": tracker.reasoning_tokens if delimiters else None,
                "answer_tokens": generated - tracker.reasoning_tokens if delimiters else None,
            },
            "timings": {
                "prefill_secs": round(prefilled - started, 3),
                "generation_secs": round(done - prefilled, 3),
                "peak_memory_bytes": int(mx.get_peak_memory()),
                "active_memory_bytes": int(mx.get_active_memory()),
            },
        }


def main() -> None:
    # mlx-lm prints memory warnings during generation. stdout is our JSON-lines
    # transport, so send library diagnostics to stderr before any model work.
    sys.stdout = sys.stderr
    mx.set_cache_limit(CACHE_LIMIT_BYTES)
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
        engine.operation = {}
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
                fused = fused_attention(int(request["head_dim"]))
                held = score_budget() + (SCORES_FLOOR_BYTES if SCORES_MB else 0)
                emit({"id": ident, "event": "attention", "fused": fused,
                      "scores_bytes": 0 if fused else held,
                      # How the copy of a cache that cannot be cut back is
                      # kept, for the window's account of memory. Gemma's
                      # cache is held at the activations' width.
                      "checkpoint": CHECKPOINT,
                      "cache_per_mille": 531 if KV_BITS == 8 else None})
            elif op == "load":
                emit({"id": ident, **engine.load(request)})
            elif op == "chat":
                result = engine.chat(request, lambda event: emit({"id": ident, **event}),
                                     lambda: inbox.is_cancelled(ident))
                emit({"id": ident, **result})
            else:
                emit({"id": ident, "event": "error", "message": f"unknown op {op!r}"})
        except Exception as error:  # the process answers every request, even a failed one
            diagnostics = engine.diagnostics()
            trace({"event": "engine_error", "error_type": type(error).__name__, **diagnostics})
            emit({
                "id": request.get("id"),
                "event": "error",
                "message": f"{type(error).__name__}: {error}; engine diagnostics: {json.dumps(diagnostics)}",
                "diagnostics": diagnostics,
                "trace": traceback.format_exc()[-2000:],
            })


if __name__ == "__main__":
    main()
