"""Tests for the sidecar's pure parts. Run with the engine's Python:

    $PWR_MLX_PYTHON -m unittest discover -s crates/pwr-mlx/sidecar
"""
import json
import pathlib
import tempfile
import unittest

from pwr_mlx import (REPEAT_LIMIT, REPEAT_SPAN, ReasoningStream, RepetitionSignals, fused_attention, looping,
                        think_delimiters)

HERE = pathlib.Path(__file__).resolve()
ROOT = HERE.parents[3]


class Trace(unittest.TestCase):
    def test_record_has_a_timestamp_for_chat_scoped_export(self):
        import pwr_mlx
        original = pwr_mlx.TRACE
        try:
            with tempfile.TemporaryDirectory() as directory:
                path = pathlib.Path(directory) / "trace.jsonl"
                pwr_mlx.TRACE = str(path)
                pwr_mlx.trace({"raw": "model output"})
                record = json.loads(path.read_text().strip())
                self.assertIsInstance(record["at_ms"], int)
                self.assertEqual(record["raw"], "model output")
        finally:
            pwr_mlx.TRACE = original


class Looping(unittest.TestCase):
    def test_a_block_repeated_back_to_back_is_a_loop(self):
        block = "def f(x):\n    return x + 1\n\n" * 12  # longer than REPEAT_SPAN
        self.assertGreater(len(block), REPEAT_SPAN)
        self.assertTrue(looping("Here is the fix:\n" + block * REPEAT_LIMIT))

    def test_fewer_copies_than_the_limit_are_not(self):
        block = "x" * 50 + "".join(chr(97 + i % 26) for i in range(300))
        self.assertFalse(looping("preface\n" + block * (REPEAT_LIMIT - 1)))

    def test_code_that_repeats_a_passage_among_different_bodies_is_not(self):
        signature = (
            "    def _buffer_decode(self, data: Any, errors: str, final: bool) -> tuple[str, int]:\n"
            "        if errors != \"strict\":\n"
            "            raise IDNAError(f'Unsupported error handling \"{errors}\"')\n\n"
            "        if not data:\n            return"
        )
        text = "".join(
            f"class C{i}:\n{signature} \"\", {i}\n        body_{i} = compute({i})\n\n"
            for i in range(REPEAT_LIMIT)
        ) + signature
        self.assertGreaterEqual(text.count(text[-REPEAT_SPAN:]), REPEAT_LIMIT)
        self.assertFalse(looping(text))

    def test_the_two_replies_the_first_guard_cut_are_not_loops(self):
        traces = {
            "experiments/part-e-budget-mlx-20260918/reports/model-trace.jsonl": 223,
            "experiments/part-e-orient64-mlx-20260918/reports/model-trace.jsonl": 196,
        }
        for path, line in traces.items():
            file = ROOT / path
            if not file.exists():
                self.skipTest(f"{path} is not in this checkout")
            raw = json.loads(file.read_text().splitlines()[line])["raw"]
            self.assertFalse(looping(raw), path)

    def test_repeated_reasoning_windows_are_observed_without_stopping_output(self):
        signal = RepetitionSignals()
        for token in (["step", "one", "two", "three", "four", "five", "six", "seven"] * 4):
            signal.feed("reasoning", token)
        for token in (f"answer-{index}" for index in range(24)):
            signal.feed("answer", token)
        summary = signal.summary()
        self.assertGreater(summary["reasoning"]["ratio_bps"], 5000)
        self.assertEqual(summary["answer"]["ratio_bps"], 0)
        self.assertNotIn("step", json.dumps(summary))


class FusedAttention(unittest.TestCase):
    def test_head_dims_measured_on_mlx_0_32(self):
        # 128 (Seed-OSS) and 64 (gpt-oss) fuse; 256 (Qwen3.6/3.8) does not.
        self.assertTrue(fused_attention(128))
        self.assertTrue(fused_attention(64))
        self.assertFalse(fused_attention(256))


class ParentGone(unittest.TestCase):
    """A sidecar whose main thread is stuck -- as one was inside an MLX
    evaluation on 2026-09-22 -- still exits once PWR closes its input."""

    def test_exits_after_input_closes_even_when_the_main_thread_is_blocked(self):
        import subprocess, sys, time
        script = (
            "import sys, time, pwr_mlx\n"
            "pwr_mlx.EXIT_GRACE_SECS = 0.5\n"
            "pwr_mlx.Inbox(sys.stdin)\n"
            "time.sleep(60)\n"
        )
        started = time.monotonic()
        child = subprocess.Popen(
            [sys.executable, "-c", script],
            cwd=pathlib.Path(__file__).parent,
            stdin=subprocess.PIPE,
        )
        child.stdin.close()
        child.wait(timeout=30)
        self.assertLess(time.monotonic() - started, 30)



class TrimmedCache(unittest.TestCase):
    """A cache that can be cut back is reused up to where two prompts part,
    with no copy beside it; what a generation added is cut off after it."""

    def setUp(self):
        import pwr_mlx

        class Layer:
            def __init__(self):
                self.offset = 0

        self.prefilled = []
        patches = {
            "can_trim_prompt_cache": lambda cache: True,
            "trim_prompt_cache": lambda cache, n: [setattr(c, "offset", c.offset - n) for c in cache],
            "make_prompt_cache": lambda model: [Layer(), Layer()],
        }
        self.saved = {name: getattr(pwr_mlx, name) for name in patches}
        for name, value in patches.items():
            setattr(pwr_mlx, name, value)
        self.addCleanup(lambda: [setattr(pwr_mlx, n, v) for n, v in self.saved.items()])
        self.engine = pwr_mlx.Engine()

        def prefill(tokens, offset, progress):
            self.prefilled.append(len(tokens))
            for layer in self.engine.cache:
                layer.offset += len(tokens)

        self.engine.prefill = prefill

    def generate(self, prompt, count):
        for layer in self.engine.cache:
            layer.offset += count
        self.engine.settle(len(prompt))

    def test_the_cache_is_reused_up_to_where_prompts_part(self):
        engine = self.engine
        first = [1, 2, 3, 4]
        self.assertEqual(engine.resume(first, lambda *_: None), 0)
        self.assertIsNone(engine.checkpoint)
        self.generate(first, 5)
        self.assertEqual(engine.cache[0].offset, 4)
        # A step that extends the prompt prefills only what it adds.
        second = first + [9, 9]
        self.assertEqual(engine.resume(second, lambda *_: None), 4)
        self.assertEqual(self.prefilled[-1], 2)
        self.generate(second, 3)
        # A history rewritten part way (a new message after reasoning was
        # dropped, a compaction) reuses what comes before the change.
        third = [1, 2, 7, 7, 7]
        self.assertEqual(engine.resume(third, lambda *_: None), 2)
        self.assertEqual(self.prefilled[-1], 3)
        self.assertEqual(engine.cache[0].offset, 5)

    def test_a_failed_prefill_leaves_nothing_to_resume_from(self):
        engine = self.engine
        engine.resume([1, 2, 3], lambda *_: None)

        def fail(tokens, offset, progress):
            raise MemoryError("out of memory")

        engine.prefill = fail
        with self.assertRaises(MemoryError):
            engine.resume([1, 2, 3, 4], lambda *_: None)
        self.assertIsNone(engine.cache)
        self.assertEqual(engine.checkpoint_tokens, [])


class StableHistory(unittest.TestCase):
    """D.E2E-21: a user message must not re-render the history before it."""

    def test_a_user_message_keeps_the_rendered_history_as_a_prefix(self):
        import pathlib
        import os
        root = pathlib.Path(os.environ.get("PWR_MLX_MODELS", pathlib.Path.home() / ".pwr/models"))
        path = root / "lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit"
        if not (path / "chat_template.jinja").exists():
            self.skipTest("Qwen3.6 is not on this machine")
        import pwr_mlx
        from mlx_lm.tokenizer_utils import load as load_tokenizer

        engine = pwr_mlx.Engine()
        engine.tokenizer = load_tokenizer(path)
        call = {"role": "assistant", "content": "I will read.", "tool_calls": [
            {"id": "c1", "type": "function",
             "function": {"name": "read_file", "arguments": {"path": "a.txt"}}}]}
        history = [{"role": "system", "content": "sys"}, {"role": "user", "content": "do it"},
                   call, {"role": "tool", "tool_call_id": "c1", "content": "a"}]
        steered = history + [{"role": "user", "content": "checks failed"}]
        for thinking in (True, False):
            before = engine.tokenizer.decode(engine.render(history, None, thinking, False)[0])
            after = engine.tokenizer.decode(engine.render(steered, None, thinking, False)[0])
            self.assertTrue(after.startswith(before), thinking)


QWEN = ("<think>", "</think>")


def run(tracker, pieces):
    reasoning, content = "", ""
    for piece in pieces:
        for channel, text in tracker.feed(piece):
            if channel == "reasoning":
                reasoning += text
            else:
                content += text
    for channel, text in tracker.flush():
        if channel == "reasoning":
            reasoning += text
        else:
            content += text
    return reasoning, content


class Delimiters(unittest.TestCase):
    def test_the_pair_comes_from_the_template(self):
        self.assertEqual(think_delimiters("x <think> y </think>"), QWEN)
        self.assertEqual(think_delimiters("<seed:think></seed:think>"),
                         ("<seed:think>", "</seed:think>"))
        self.assertIsNone(think_delimiters("<|channel|>analysis<|message|>"))
        # An opening tag alone is not a pair the engine can close.
        self.assertIsNone(think_delimiters("<think> only"))


class Stream(unittest.TestCase):
    def test_reasoning_opened_by_the_template_is_counted_in_tokens(self):
        tracker = ReasoningStream(QWEN, starts_inside=True)
        reasoning, content = run(tracker, ["The", " user", " wants", " 5", "</th", "ink>", "\n\n5"])
        self.assertEqual(reasoning, "The user wants 5")
        self.assertEqual(content.strip(), "5")
        # One count per generated token inside the block, not per character.
        self.assertEqual(tracker.reasoning_tokens, 6)
        self.assertTrue(tracker.answered)

    def test_a_block_the_model_opens_itself_is_reasoning_too(self):
        tracker = ReasoningStream(QWEN, starts_inside=False)
        reasoning, content = run(tracker, ["\n", "<th", "ink>", "hmm", "</think>", "Hello"])
        self.assertEqual(reasoning, "hmm")
        self.assertEqual(content, "Hello")

    def test_an_answer_that_merely_starts_with_a_bracket_is_not_held(self):
        tracker = ReasoningStream(QWEN, starts_inside=False)
        reasoning, content = run(tracker, ["<", "div>", " ok"])
        self.assertEqual(reasoning, "")
        self.assertEqual(content, "<div> ok")

    def test_a_model_without_delimiters_streams_everything_as_content(self):
        tracker = ReasoningStream(None, starts_inside=False)
        reasoning, content = run(tracker, ["<|channel|>analysis", " x"])
        self.assertEqual((reasoning, content), ("", "<|channel|>analysis x"))
        self.assertEqual(tracker.reasoning_tokens, 0)

    def test_reopening_after_a_forced_close_is_detected(self):
        tracker = ReasoningStream(QWEN, starts_inside=True)
        run(tracker, ["a", "b"])
        tracker.force_close()
        tracker.feed("\n")
        tracker.feed("<think>")
        self.assertTrue(tracker.reopened)
        self.assertFalse(tracker.answered)

    def test_a_tool_call_after_the_close_is_an_answer_and_is_not_split(self):
        tracker = ReasoningStream(QWEN, starts_inside=True)
        call = '<tool_call>{"name": "read_file", "arguments": {"path": "a"}}</tool_call>'
        reasoning, content = run(tracker, ["think", "</think>", "\n"] + list(call))
        self.assertEqual(content.strip(), call)


class FakeResponse:
    def __init__(self, text, finish=None):
        self.text = text
        self.finish_reason = finish


class FakeTokenizer:
    """Tokens are characters; the template opens a think block."""

    chat_template = "{{ enable_thinking }} <think> </think>"

    def apply_chat_template(self, messages, add_generation_prompt=False, **kwargs):
        self.kwargs = kwargs
        prompt = "".join(m["content"] for m in messages)
        if add_generation_prompt:
            prompt += "|<think>" if kwargs.get("enable_thinking", True) else "|<think></think>"
        return [ord(c) for c in prompt]

    def decode(self, tokens):
        return "".join(chr(t) for t in tokens)

    def encode(self, text, add_special_tokens=False):
        return [ord(c) for c in text]


def scripted(first, after_close=None):
    """A stream_generate stand-in: `first` for the prompt, `after_close` once
    the engine has fed the closing delimiter."""
    calls = []

    def generate(model, tokenizer, prompt, max_tokens, **kwargs):
        prompt_text = "".join(chr(int(t)) for t in prompt.tolist())
        calls.append((prompt_text, max_tokens))
        script = after_close if prompt_text.startswith("</think>") else first
        for index, text in enumerate(script[:max_tokens]):
            last = index == len(script) - 1
            yield FakeResponse(text, "stop" if last else None)
    generate.calls = calls
    return generate


class Chat(unittest.TestCase):
    """The whole request path around the model, with the model scripted."""

    def setUp(self):
        import pwr_mlx
        self.module = pwr_mlx
        self.saved = (pwr_mlx.stream_generate, pwr_mlx.make_prompt_cache,
                      pwr_mlx.snapshot, pwr_mlx.restore,
                      pwr_mlx.make_sampler, pwr_mlx.make_logits_processors)
        pwr_mlx.make_prompt_cache = lambda model: []
        pwr_mlx.snapshot = lambda cache: None
        pwr_mlx.restore = lambda cache, states: None
        engine = pwr_mlx.Engine()
        engine.model = object()
        engine.tokenizer = FakeTokenizer()
        engine.prefill = lambda tokens, offset, progress=None: None
        self.engine = engine

    def tearDown(self):
        m = self.module
        (m.stream_generate, m.make_prompt_cache, m.snapshot, m.restore,
         m.make_sampler, m.make_logits_processors) = self.saved

    def chat(self, generate, cancelled=lambda: False, **request):
        self.module.stream_generate = generate
        events = []
        done = self.engine.chat({"messages": [{"role": "user", "content": "q"}], **request},
                                events.append, cancelled)
        text = lambda channel: "".join(e["text"] for e in events
                                       if e.get("event") == "delta" and e["channel"] == channel)
        return done, text("reasoning"), text("content"), events

    def test_sampling_values_reach_the_stream(self):
        seen = {}
        self.module.make_sampler = lambda **kwargs: seen.setdefault("sampler", kwargs)
        self.module.make_logits_processors = lambda **kwargs: seen.setdefault("processors", kwargs)

        def generate(model, tokenizer, prompt, max_tokens, **kwargs):
            seen["stream"] = kwargs
            yield FakeResponse("ok", "stop")

        done, _, _, _ = self.chat(
            generate, temperature=1.0, top_p=0.95, top_k=20,
            min_p=0.1, presence_penalty=0.2, repetition_penalty=1.05,
        )
        self.assertEqual(done["finish_reason"], "stop")
        self.assertEqual(seen["sampler"], {
            "temp": 1.0, "top_p": 0.95, "top_k": 20, "min_p": 0.1,
        })
        self.assertEqual(seen["processors"], {
            "presence_penalty": 0.2, "repetition_penalty": 1.05,
        })
        self.assertIs(seen["stream"]["sampler"], seen["sampler"])
        self.assertIs(seen["stream"]["logits_processors"], seen["processors"])

    def test_a_budget_closes_the_reasoning_and_the_answer_follows(self):
        generate = scripted(list("abcdefghij"), list("\n42"))
        done, reasoning, content, events = self.chat(
            generate, reasoning_budget=4, max_tokens=1000)
        self.assertEqual(done["finish_reason"], "stop")
        self.assertTrue(done["budget_forced"])
        self.assertEqual(done["usage"]["reasoning_tokens"], 4)
        self.assertEqual(reasoning, "abcd")
        self.assertEqual(content.strip(), "42")
        self.assertTrue(any(e.get("event") == "finalizing" for e in events))
        # The continuation got the rest of the cap, not a fixed sliver of it.
        self.assertEqual(generate.calls[1][1], 1000 - 4)

    def test_a_reopened_block_after_the_close_fails_once_and_does_not_loop(self):
        generate = scripted(list("abcdefghij"), ["<think>"] + list("more"))
        done, _, _, _ = self.chat(generate, reasoning_budget=4, max_tokens=1000)
        self.assertEqual(done["finish_reason"], "reasoning_unfinished")
        self.assertEqual(len(generate.calls), 2)

    def test_stopping_with_no_answer_after_the_close_is_unfinished(self):
        generate = scripted(list("abcdefghij"), ["\n"])
        done, _, _, _ = self.chat(generate, reasoning_budget=4, max_tokens=1000)
        self.assertEqual(done["finish_reason"], "reasoning_unfinished")

    def test_the_budget_never_takes_the_final_reserve(self):
        generate = scripted(list("x" * 2000), list("ok"))
        done, _, _, _ = self.chat(generate, reasoning_budget=5000, max_tokens=1024)
        self.assertEqual(done["usage"]["reasoning_tokens"], 1024 - self.module.FINAL_RESERVE)

    def test_no_budget_means_no_close(self):
        generate = scripted(list("abc") + ["</think>"] + list("ok"))
        done, reasoning, content, _ = self.chat(generate, max_tokens=1000)
        self.assertFalse(done["budget_forced"])
        self.assertEqual((reasoning, content), ("abc", "ok"))
        self.assertEqual(len(generate.calls), 1)

    def test_a_template_without_delimiters_is_not_budgeted_or_counted(self):
        self.engine.tokenizer.chat_template = "{{ messages }}"
        generate = scripted(list("<think>abcdefgh"))
        done, _, _, _ = self.chat(generate, reasoning_budget=2, max_tokens=1000)
        self.assertFalse(done["budget_forced"])
        self.assertFalse(done["reasoning_tracked"])
        self.assertIsNone(done["usage"]["reasoning_tokens"])

    def test_cancelling_stops_inside_the_reasoning_without_waiting_for_the_budget(self):
        seen = []
        cancelled = lambda: len(seen) >= 3
        def generate(model, tokenizer, prompt, max_tokens, **kwargs):
            for _ in range(max_tokens):
                seen.append(1)
                yield FakeResponse("r")
        done, _, _, _ = self.chat(generate, cancelled, reasoning_budget=5000, max_tokens=8000)
        self.assertEqual(done["finish_reason"], "cancelled")
        self.assertLessEqual(len(seen), 3)

    def test_the_level_and_the_native_budget_reach_the_template(self):
        self.engine.tokenizer.chat_template = "<think></think> thinking_budget reasoning_effort"
        self.chat(scripted(list("a</think>ok")), reasoning_budget=1300, reasoning_effort="high",
                  max_tokens=4000)
        kwargs = self.engine.tokenizer.kwargs
        self.assertEqual(kwargs["thinking_budget"], 1024)
        self.assertEqual(kwargs["reasoning_effort"], "high")


if __name__ == "__main__":
    unittest.main()
