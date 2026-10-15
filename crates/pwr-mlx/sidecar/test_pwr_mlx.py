"""Tests for the sidecar's pure parts. Run with the engine's Python:

    $PWR_MLX_PYTHON -m unittest discover -s crates/pwr-mlx/sidecar
"""
import json
import pathlib
import tempfile
import unittest

from pwr_mlx import (REPEAT_LIMIT, add_turn_ends, REPEAT_SPAN, ReasoningStream, RepetitionSignals, fused_attention, looping,
                        one_thought, think_delimiters)

HERE = pathlib.Path(__file__).resolve()
ROOT = HERE.parents[3]


class MetalContextTimeout(unittest.TestCase):
    def test_driver_workaround_is_set_before_mlx_import_and_respects_override(self):
        import os, subprocess, sys
        # Exercise module startup in a fresh interpreter: setting it after the
        # Metal device was created would not fix the failing context switch.
        script = """import builtins, os, sys
original = builtins.__import__
def importing(name, *args, **kwargs):
    if name == 'mlx.core':
        expected = os.environ.get('EXPECTED_AGX')
        assert os.environ.get('AGX_RELAX_CDM_CTXSTORE_TIMEOUT') == expected
    return original(name, *args, **kwargs)
builtins.__import__ = importing
import pwr_mlx
print(os.environ.get('AGX_RELAX_CDM_CTXSTORE_TIMEOUT'))
"""
        for explicit in (None, "0", "1"):
            with self.subTest(explicit=explicit):
                env = dict(os.environ, PYTHONPATH=str(HERE.parent))
                env.pop("AGX_RELAX_CDM_CTXSTORE_TIMEOUT", None)
                if explicit is not None:
                    env["AGX_RELAX_CDM_CTXSTORE_TIMEOUT"] = explicit
                expected = explicit or ("1" if sys.platform == "darwin" else "None")
                if expected != "None":
                    env["EXPECTED_AGX"] = expected
                else:
                    env.pop("EXPECTED_AGX", None)
                result = subprocess.run([sys.executable, "-c", script], env=env,
                                        capture_output=True, text=True, timeout=30, check=True)
                self.assertEqual(result.stdout.strip(), expected)

    def test_failed_device_diagnostics_use_the_pre_operation_sample(self):
        from unittest.mock import patch
        from pwr_mlx import Engine
        engine = Engine()
        engine.observe("prefill", offset=108000, chunk_tokens=32)
        before = engine.operation["active_memory_bytes"]
        with patch("pwr_mlx.mx.get_active_memory", side_effect=RuntimeError("broken GPU")):
            record = engine.diagnostics()
        self.assertEqual(record["phase"], "prefill")
        self.assertEqual(record["chunk_tokens"], 32)
        self.assertEqual(record["active_memory_bytes"], before)
        self.assertNotIn("started", record)
        self.assertGreaterEqual(record["elapsed_secs"], 0)

    def test_metal_fault_answers_with_diagnostics_on_the_json_protocol(self):
        import subprocess, sys
        script = """import pwr_mlx
pwr_mlx.EXIT_GRACE_SECS = 0.5
class Failed(pwr_mlx.Engine):
    def chat(self, *args):
        self.observe('prefill', offset=108000, chunk_tokens=32)
        raise RuntimeError('[METAL] Command buffer execution failed: Impacting Interactivity')
pwr_mlx.Engine = Failed
pwr_mlx.main()
"""
        result = subprocess.run([sys.executable, "-c", script], cwd=HERE.parent,
                                input='{"id": 44, "op": "chat"}\n',
                                capture_output=True, text=True, timeout=30, check=True)
        event = json.loads(result.stdout)
        self.assertEqual(event["id"], 44)
        self.assertEqual(event["event"], "error")
        self.assertEqual(event["diagnostics"]["phase"], "prefill")
        self.assertEqual(event["diagnostics"]["offset"], 108000)
        self.assertIn("Impacting Interactivity", event["message"])
        self.assertIn('"chunk_tokens": 32', event["message"])


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



class CopiedCache(unittest.TestCase):
    """A cache that cannot be cut back is copied just before the marker that
    closes the prompt's last message, so a next prompt that rewrites that
    marker still resumes from the copy."""

    def setUp(self):
        import pwr_mlx

        class Layer:
            def __init__(self):
                self.offset = 0

        class Tokenizer:
            eos_token_ids = {99}

            def convert_tokens_to_ids(self, token):
                return 99

        self.prefilled = []
        patches = {
            "make_prompt_cache": lambda model: [Layer()],
            "snapshot": lambda cache: [c.offset for c in cache],
            "restore": lambda cache, states: [setattr(c, "offset", o) for c, o in zip(cache, states)],
        }
        self.saved = {name: getattr(pwr_mlx, name) for name in patches}
        for name, value in patches.items():
            setattr(pwr_mlx, name, value)
        self.addCleanup(lambda: [setattr(pwr_mlx, n, v) for n, v in self.saved.items()])
        self.engine = pwr_mlx.Engine()
        self.engine.model = Images()
        self.engine.tokenizer = Tokenizer()

        def prefill(tokens, offset, progress, cancelled=lambda: False):
            self.prefilled.append(list(tokens))
            for layer in self.engine.cache:
                layer.offset = offset + len(tokens)

        self.engine.prefill = prefill

    def test_a_rewritten_closing_marker_still_resumes_from_the_copy(self):
        engine = self.engine
        # ... </tool_response> <|im_end|> \n
        first = [1, 2, 3, 4, 99, 10]
        self.assertEqual(engine.resume(first, lambda *_: None), 0)
        self.assertEqual(engine.checkpoint_tokens, [1, 2, 3, 4])
        self.assertEqual(self.prefilled, [[1, 2, 3, 4], [99, 10]])
        # A second tool result: the marker becomes \n <tool_response> ...
        second = [1, 2, 3, 4, 10, 7, 7, 99, 10]
        self.assertEqual(engine.resume(second, lambda *_: None), 4)
        self.assertEqual(self.prefilled[-2:], [[10, 7, 7], [99, 10]])
        # An ordinary next step, the assistant's turn after the marker.
        third = second + [5, 5, 99, 10]
        self.assertEqual(engine.resume(third, lambda *_: None), 7)

    def test_a_prompt_without_a_marker_is_copied_whole(self):
        self.assertEqual(self.engine.resume([1, 2, 3], lambda *_: None), 0)
        self.assertEqual(self.engine.checkpoint_tokens, [1, 2, 3])


class LayerCheckpoint(unittest.TestCase):
    """Asked for, the copy of a cache that cannot be cut back holds only the
    layers that cannot: one that only appends is cut back to where it stood."""

    def setUp(self):
        import mlx.core as mx
        import pwr_mlx

        class KVCache:
            def __init__(self):
                self.offset = 0
                self.state = [mx.zeros((1,))]
                self.meta_state = ""

            def trim(self, n):
                n = min(self.offset, n)
                self.offset -= n
                return n

        class Fixed:
            def __init__(self):
                self.state = [mx.array([0.0])]
                self.meta_state = ""

        self.mx, self.module = mx, pwr_mlx
        self.cache = [Fixed(), KVCache(), Fixed()]
        self.was = pwr_mlx.CHECKPOINT
        self.addCleanup(lambda: setattr(pwr_mlx, "CHECKPOINT", self.was))

    def advance(self, tokens, value):
        for layer in self.cache:
            if hasattr(layer, "offset"):
                layer.offset += tokens
            else:
                layer.state = [self.mx.array([value])]

    def test_only_the_layers_that_cannot_be_cut_back_are_copied(self):
        self.module.CHECKPOINT = "layers"
        self.advance(100, 1.0)
        kept = self.module.snapshot(self.cache)
        self.assertEqual([type(k).__name__ for k in kept], ["tuple", "Offset", "tuple"])
        self.assertEqual(kept[1], 100)
        self.advance(40, 2.0)
        self.module.restore(self.cache, kept)
        self.assertEqual(self.cache[1].offset, 100)
        self.assertEqual(self.cache[0].state[0].item(), 1.0)
        self.assertEqual(self.cache[2].state[0].item(), 1.0)
        # The copy is still what it was: a second prompt can start from it.
        self.advance(7, 3.0)
        self.module.restore(self.cache, kept)
        self.assertEqual(self.cache[1].offset, 100)
        self.assertEqual(self.cache[0].state[0].item(), 1.0)

    def test_unasked_only_the_layers_that_cannot_be_cut_back_are_copied(self):
        self.assertEqual(self.module.CHECKPOINT, "layers")

    def test_asked_whole_every_layer_is_copied(self):
        self.module.CHECKPOINT = "whole"
        kept = self.module.snapshot(self.cache)
        self.assertEqual([type(k).__name__ for k in kept], ["tuple", "tuple", "tuple"])

    def test_a_sliding_window_is_copied_whatever_was_asked(self):
        self.module.CHECKPOINT = "layers"

        class RotatingKVCache:
            offset = 5
            state = [self.mx.zeros((1,))]
            meta_state = ""

            def trim(self, n):
                return n

        kept = self.module.snapshot([RotatingKVCache()])
        self.assertIsInstance(kept[0], tuple)

    def test_a_cache_shorter_than_its_copy_is_left_as_it_was(self):
        self.module.CHECKPOINT = "layers"
        self.advance(100, 1.0)
        kept = self.module.snapshot(self.cache)
        self.advance(0, 2.0)
        self.cache[1].offset = 60
        with self.assertRaises(ValueError):
            self.module.restore(self.cache, kept)
        self.assertEqual(self.cache[0].state[0].item(), 2.0)
        self.assertEqual(self.cache[1].offset, 60)

    def test_a_copy_that_cannot_be_put_back_starts_the_prompt_from_nothing(self):
        module = self.module
        fresh = [object()]
        saved = (module.make_prompt_cache, module.restore, module.snapshot)
        self.addCleanup(lambda: [setattr(module, n, v) for n, v in zip(
            ("make_prompt_cache", "restore", "snapshot"), saved)])
        module.make_prompt_cache = lambda model: fresh

        def refuse(cache, states):
            raise ValueError("the cache is shorter than its copy")

        module.restore = refuse
        module.snapshot = lambda cache: ["copy"]
        engine = module.Engine()
        engine.model = Images()

        class Tokenizer:
            eos_token_ids = set()

            def convert_tokens_to_ids(self, token):
                return None

        engine.tokenizer = Tokenizer()
        engine.cache, engine.checkpoint, engine.checkpoint_tokens = self.cache, ["old"], [1, 2]
        prefilled = []
        engine.prefill = lambda tokens, offset, progress, cancelled=lambda: False: prefilled.append((list(tokens), offset))
        self.assertEqual(engine.resume([1, 2, 3], lambda *_: None), 0)
        self.assertIs(engine.cache, fresh)
        self.assertEqual(prefilled[0], ([1, 2, 3], 0))


class ScoreBudget(unittest.TestCase):
    def test_a_budget_asked_for_is_the_most_a_step_may_hold(self):
        import pwr_mlx
        was = pwr_mlx.SCORES_MB
        self.addCleanup(lambda: setattr(pwr_mlx, "SCORES_MB", was))
        pwr_mlx.SCORES_MB = None
        # Unasked, 1 GiB: room for the full context on every tier.
        self.assertEqual(pwr_mlx.score_budget(), 1024**3)
        pwr_mlx.SCORES_MB = "512"
        self.assertEqual(pwr_mlx.score_budget(), 512 * 1024**2)
        # Never more than a quarter of the largest Metal buffer.
        pwr_mlx.SCORES_MB = str(10**9)
        limit = pwr_mlx.mx.device_info().get("max_buffer_length") or 8 * 1024**3
        self.assertEqual(pwr_mlx.score_budget(), int(limit) // 4)

    def test_a_small_budget_shortens_the_step_as_the_context_grows(self):
        import pwr_mlx
        engine = pwr_mlx.Engine()
        was = pwr_mlx.SCORES_MB
        self.addCleanup(lambda: setattr(pwr_mlx, "SCORES_MB", was))
        engine.fused, engine.heads, engine.score_budget = False, 16, 1024**3
        # Unasked, a score is counted as the two bytes it is stored in.
        pwr_mlx.SCORES_MB = None
        self.assertEqual(engine.prefill_step(200_000), 160)
        # Asked for, as the three a step was measured to hold.
        pwr_mlx.SCORES_MB = "1024"
        self.assertEqual(engine.prefill_step(0), 2720)
        self.assertEqual(engine.prefill_step(200_000), 96)


# Any pair: the tracker reads the delimiters it is given.
PAIR = ("<think>", "</think>")


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
        self.assertEqual(think_delimiters("x <|channel>thought y <channel|>"), ("<|channel>thought", "<channel|>"))
        self.assertIsNone(think_delimiters("x <think> y </think>"))
        # An opening tag alone is not a pair the engine can close.
        self.assertIsNone(think_delimiters("<|channel>thought only"))


class Stream(unittest.TestCase):
    def test_gemma_thought_is_separate_from_native_tool_call(self):
        delimiters = think_delimiters("<|channel>thought\n<channel|>")
        self.assertEqual(delimiters, ("<|channel>thought", "<channel|>"))
        tracker = ReasoningStream(delimiters, starts_inside=False)
        thought, content = run(tracker, ["<|chan", "nel>thought", "plan", "<channel", "|>", "<|tool_call>call:read_file{}<tool_call|>"])
        self.assertEqual(thought, "plan")
        self.assertEqual(content, "<|tool_call>call:read_file{}<tool_call|>")

    def test_reasoning_opened_by_the_template_is_counted_in_tokens(self):
        tracker = ReasoningStream(PAIR, starts_inside=True)
        reasoning, content = run(tracker, ["The", " user", " wants", " 5", "</th", "ink>", "\n\n5"])
        self.assertEqual(reasoning, "The user wants 5")
        self.assertEqual(content.strip(), "5")
        # One count per generated token inside the block, not per character.
        self.assertEqual(tracker.reasoning_tokens, 6)
        self.assertTrue(tracker.answered)

    def test_a_block_the_model_opens_itself_is_reasoning_too(self):
        tracker = ReasoningStream(PAIR, starts_inside=False)
        reasoning, content = run(tracker, ["\n", "<th", "ink>", "hmm", "</think>", "Hello"])
        self.assertEqual(reasoning, "hmm")
        self.assertEqual(content, "Hello")

    def test_an_answer_that_merely_starts_with_a_bracket_is_not_held(self):
        tracker = ReasoningStream(PAIR, starts_inside=False)
        reasoning, content = run(tracker, ["<", "div>", " ok"])
        self.assertEqual(reasoning, "")
        self.assertEqual(content, "<div> ok")

    def test_a_model_without_delimiters_streams_everything_as_content(self):
        tracker = ReasoningStream(None, starts_inside=False)
        reasoning, content = run(tracker, ["<|channel|>analysis", " x"])
        self.assertEqual((reasoning, content), ("", "<|channel|>analysis x"))
        self.assertEqual(tracker.reasoning_tokens, 0)

    def test_reopening_after_a_forced_close_is_detected(self):
        tracker = ReasoningStream(PAIR, starts_inside=True)
        run(tracker, ["a", "b"])
        tracker.force_close()
        tracker.feed("\n")
        tracker.feed("<think>")
        self.assertTrue(tracker.reopened)
        self.assertFalse(tracker.answered)

    def test_a_tool_call_after_the_close_is_an_answer_and_is_not_split(self):
        tracker = ReasoningStream(PAIR, starts_inside=True)
        call = '<tool_call>{"name": "read_file", "arguments": {"path": "a"}}</tool_call>'
        reasoning, content = run(tracker, ["think", "</think>", "\n"] + list(call))
        self.assertEqual(content.strip(), call)


class FakeResponse:
    def __init__(self, text, finish=None):
        self.text = text
        self.finish_reason = finish


class FakeTokenizer:
    """Tokens are characters; the template opens a thought channel."""

    chat_template = "{{ enable_thinking }} <|channel>thought <channel|>"

    def apply_chat_template(self, messages, add_generation_prompt=False, **kwargs):
        self.kwargs = kwargs
        prompt = "".join(m["content"] for m in messages)
        if add_generation_prompt:
            prompt += "|<|channel>thought" if kwargs.get("enable_thinking", True) else "|<|channel>thought<channel|>"
        return [ord(c) for c in prompt]

    def decode(self, tokens):
        return "".join(chr(t) for t in tokens)

    def encode(self, text, add_special_tokens=False):
        return [ord(c) for c in text]


class Images:
    """A model with no image in its prompt."""

    def set_prompt(self, tokens, images=None, digest=None):
        pass

    def whole_images(self, start, end):
        return end


def scripted(first, after_close=None):
    """A stream_generate stand-in: `first` for the prompt, `after_close` once
    the engine has fed the closing delimiter."""
    calls = []

    def generate(model, tokenizer, prompt, max_tokens, **kwargs):
        prompt_text = "".join(chr(int(t)) for t in prompt.tolist())
        calls.append((prompt_text, max_tokens))
        script = after_close if prompt_text.startswith("<channel|>") else first
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
        engine.model = Images()
        engine.tokenizer = FakeTokenizer()
        engine.prefill = lambda tokens, offset, progress=None, cancelled=lambda: False: None
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

    def test_actual_rendered_context_is_refused_before_prefill(self):
        from unittest.mock import Mock
        self.engine.resume = Mock(side_effect=AssertionError("must not prefill"))
        self.engine.render = lambda *args: (list(range(20)), None)
        generate = scripted(list("ok"))
        done, _, _, _ = self.chat(generate, context_tokens=20)
        self.assertEqual(done["event"], "error")
        self.assertEqual(done["code"], "context_limit")
        self.assertEqual(done["prompt_tokens"], 20)
        self.assertEqual(generate.calls, [])
        self.engine.resume.assert_not_called()

    def test_schema_and_template_tokens_bound_generation_space(self):
        generate = scripted(list("long response"))
        full = self.engine.render([{"role": "user", "content": "q"}], None, False, True)[0]
        done, _, _, _ = self.chat(generate, thinking=False,
                                 context_tokens=len(full) + 2, max_tokens=8192)
        self.assertEqual(generate.calls[0][1], 2)
        self.assertLessEqual(done["usage"]["prompt_tokens"] + done["usage"]["completion_tokens"], len(full) + 2)

    def test_an_aside_leaves_the_conversations_cache_where_it_was(self):
        cache, copy = object(), object()
        self.engine.cache, self.engine.checkpoint = cache, copy
        self.engine.checkpoint_tokens = [1, 2, 3]
        conversation = (cache, copy, [1, 2, 3])
        self.module.stream_generate = scripted(list("summary"))
        self.engine.chat({"messages": [{"role": "user", "content": "describe"}],
                          "thinking": False, "aside": True}, lambda event: None)
        self.assertIs(self.engine.cache, conversation[0])
        self.assertIs(self.engine.checkpoint, conversation[1])
        self.assertEqual(self.engine.checkpoint_tokens, conversation[2])

    def test_an_aside_that_fails_still_gives_the_conversation_its_cache_back(self):
        self.engine.cache, self.engine.checkpoint = object(), object()
        self.engine.checkpoint_tokens = kept = [1, 2, 3]

        def broken(*args, **kwargs):
            raise RuntimeError("engine fault")
            yield
        self.module.stream_generate = broken
        with self.assertRaises(RuntimeError):
            self.engine.chat({"messages": [{"role": "user", "content": "describe"}],
                              "thinking": False, "aside": True}, lambda event: None)
        self.assertEqual(self.engine.checkpoint_tokens, kept)

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
        # mlx-lm's own presence window is 20 tokens unless the caller widens it.
        self.assertEqual(seen["processors"], {
            "presence_penalty": 0.2, "presence_context_size": 20,
            "repetition_penalty": 1.05,
        })
        self.assertIs(seen["stream"]["sampler"], seen["sampler"])
        self.assertIs(seen["stream"]["logits_processors"], seen["processors"])

    def test_a_wider_presence_window_reaches_the_processors(self):
        seen = {}
        self.module.make_logits_processors = lambda **kwargs: seen.setdefault("processors", kwargs)

        def generate(model, tokenizer, prompt, max_tokens, **kwargs):
            yield FakeResponse("ok", "stop")

        self.chat(generate, presence_penalty=1.0, presence_context_size=1024)
        self.assertEqual(seen["processors"]["presence_context_size"], 1024)

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
        self.assertEqual(generate.calls[1][1], 1000 - 4 - len(generate.calls[1][0]))
        self.assertEqual(done["usage"]["forced_tokens"], len(generate.calls[1][0]))

    def test_gemma_budget_closes_thought_and_preserves_tool_call(self):
        class GemmaTokenizer(FakeTokenizer):
            chat_template = "{{ enable_thinking }}<|channel>thought<channel|><|tool_call>"
            def apply_chat_template(self, messages, add_generation_prompt=False, **kwargs):
                prompt = "".join(m["content"] for m in messages)
                if add_generation_prompt:
                    prompt += "<|channel>thought"
                return [ord(c) for c in prompt]
        self.engine.tokenizer = GemmaTokenizer()
        calls = []
        call = "<|tool_call>call:read_file{path:a}<tool_call|>"
        def generate(model, tokenizer, prompt, max_tokens, **kwargs):
            text = tokenizer.decode(prompt.tolist())
            calls.append(text)
            pieces = [call] if text.startswith("<channel|>") else list("abcdefghij")
            for at, piece in enumerate(pieces):
                yield FakeResponse(piece, "stop" if at == len(pieces)-1 else None)
        done, thought, content, _ = self.chat(generate, reasoning_budget=4, max_tokens=1000)
        self.assertTrue(done["budget_forced"])
        self.assertEqual(done["usage"]["reasoning_tokens"], 4)
        self.assertEqual(thought, "abcd")
        self.assertEqual(content, call)
        self.assertTrue(calls[1].startswith("<channel|>"))

    def test_a_reopened_block_after_the_close_fails_once_and_does_not_loop(self):
        generate = scripted(list("abcdefghij"), ["<|channel>thought"] + list("more"))
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
        generate = scripted(list("abc") + ["<channel|>"] + list("ok"))
        done, reasoning, content, _ = self.chat(generate, max_tokens=1000)
        self.assertFalse(done["budget_forced"])
        self.assertEqual((reasoning, content), ("abc", "ok"))
        self.assertEqual(len(generate.calls), 1)

    def test_cached_model_template_cannot_disable_gemmas_reasoning_budget(self):
        self.engine.tokenizer.chat_template = "{{ messages }}"
        generate = scripted(list("<|channel>thoughtabcdefgh"), list("answer"))
        done, _, _, _ = self.chat(generate, reasoning_budget=2, max_tokens=1000)
        self.assertTrue(done["budget_forced"])
        self.assertTrue(done["reasoning_tracked"])
        self.assertIsNotNone(done["usage"]["reasoning_tokens"])

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


class CacheCheckpointMetadata(unittest.TestCase):
    def test_rotating_cache_restores_position_and_next_append(self):
        import mlx.core as mx
        from mlx_lm.models.cache import RotatingKVCache
        from pwr_mlx import snapshot, restore
        cache = RotatingKVCache(max_size=4, keep=0)
        first = mx.arange(6).reshape(1, 1, 3, 2)
        cache.update_and_fetch(first, first)
        saved = snapshot([cache])
        expected_meta = (cache.offset, cache.keep, cache.max_size, cache._idx)
        # Cross the window while generating, then roll back to the prompt.
        cache.update_and_fetch(mx.ones((1, 1, 4, 2)), mx.ones((1, 1, 4, 2)))
        restore([cache], saved)
        self.assertEqual((cache.offset, cache.keep, cache.max_size, cache._idx), expected_meta)
        reference = RotatingKVCache(max_size=4, keep=0)
        reference.update_and_fetch(first, first)
        next_tokens = mx.full((1, 1, 2, 2), 7)
        actual = cache.update_and_fetch(next_tokens, next_tokens)
        expected = reference.update_and_fetch(next_tokens, next_tokens)
        self.assertEqual((cache.offset, cache.keep, cache.max_size, cache._idx),
                         (reference.offset, reference.keep, reference.max_size, reference._idx))
        for a, b in zip(actual, expected):
            self.assertTrue(mx.array_equal(a, b).item())

    def test_kv_cache_checkpoint_is_reusable_after_generation(self):
        import mlx.core as mx
        from mlx_lm.models.cache import KVCache
        from pwr_mlx import snapshot, restore
        cache = KVCache()
        first = mx.arange(6).reshape(1, 1, 3, 2)
        cache.update_and_fetch(first, first)
        saved = snapshot([cache])
        cache.update_and_fetch(mx.ones((1, 1, 2, 2)), mx.ones((1, 1, 2, 2)))
        restore([cache], saved)
        self.assertEqual(cache.offset, 3)
        for tensor in cache.state[:2]:
            self.assertTrue(mx.array_equal(tensor[..., :cache.offset, :], first).item())
        cache.update_and_fetch(mx.ones((1, 1, 1, 2)), mx.ones((1, 1, 1, 2)))
        restore([cache], saved)
        self.assertEqual(cache.offset, 3)
        for tensor in cache.state[:2]:
            self.assertTrue(mx.array_equal(tensor[..., :cache.offset, :], first).item())


class OneThoughtPerReply(unittest.TestCase):
    """Gemma 4 reopened an empty thought channel until the loop guard (2026-10-02)."""

    class Tokenizer:
        unk_token_id = 3
        vocabulary = {"<|channel>": 100, "<channel|>": 101}

        def convert_tokens_to_ids(self, token):
            return self.vocabulary.get(token, self.unk_token_id)

    def step(self, processor, tokens):
        import mlx.core as mx
        logits = processor(mx.array(tokens), mx.zeros((1, 128)))
        return logits[0, 100].item()

    def test_the_channel_opens_once_and_never_again(self):
        processor = one_thought(self.Tokenizer(), "... <|channel>thought ...")
        self.assertEqual(self.step(processor, [7, 8]), 0.0)
        self.assertEqual(self.step(processor, [7, 8, 100]), float("-inf"))
        self.assertEqual(self.step(processor, [7, 8, 100, 9, 101, 10]), float("-inf"))

    def test_a_prompt_that_closed_it_already_shuts_it(self):
        processor = one_thought(self.Tokenizer(), "<|channel>")
        self.assertEqual(self.step(processor, [100, 9, 101]), float("-inf"))

    def test_a_previous_turn_does_not_block_this_turn_thinking(self):
        processor = one_thought(self.Tokenizer(), "<|channel>")
        prompt = [7, 100, 9, 101, 8, 10]
        self.assertEqual(self.step(processor, prompt), 0.0)
        self.assertEqual(self.step(processor, prompt + [100]), float("-inf"))

    def close_score(self, processor, tokens):
        import mlx.core as mx
        return processor(mx.array(tokens), mx.zeros((1, 128)))[0, 101].item()

    def test_first_close_is_allowed_but_repeated_close_is_blocked(self):
        processor = one_thought(self.Tokenizer(), "<|channel>")
        self.assertEqual(self.close_score(processor, [7, 8]), 0.0)
        self.assertEqual(self.close_score(processor, [7, 8, 100, 9]), 0.0)
        self.assertEqual(self.close_score(processor, [7, 8, 100, 9, 101]), float("-inf"))
        self.assertEqual(self.close_score(processor, [7, 8, 100, 9, 101, 10]), float("-inf"))

    def test_prefilled_close_cannot_close_again(self):
        processor = one_thought(self.Tokenizer(), "<|channel>")
        self.assertEqual(self.close_score(processor, [100, 9, 101]), float("-inf"))

    def test_historical_close_does_not_block_new_reply_close(self):
        processor = one_thought(self.Tokenizer(), "<|channel>")
        prompt = [100, 9, 101, 7, 8]
        self.assertEqual(self.close_score(processor, prompt), 0.0)
        self.assertEqual(self.close_score(processor, prompt + [101]), float("-inf"))

    def test_after_a_tool_result_the_empty_block_shuts_the_channel(self):
        # Reasoning off: the engine ends the prompt with the empty block
        # (`empty_thought`), so the reply cannot open another.
        processor = one_thought(self.Tokenizer(), "<|channel>")
        prompt = [7, 8, 100, 9, 101]
        self.assertEqual(self.step(processor, prompt), float("-inf"))
        self.assertEqual(self.close_score(processor, prompt), float("-inf"))

    def test_without_a_prefilled_block_the_reply_may_open_one(self):
        # What Gemma does after a tool result when nothing closed the channel:
        # an empty block, then the answer. Forbidding it gave empty replies.
        processor = one_thought(self.Tokenizer(), "<|channel>")
        self.assertEqual(self.step(processor, [7, 8]), 0.0)
        self.assertEqual(self.step(processor, [7, 8, 100, 101]), float("-inf"))

    def test_templates_without_the_channel_get_nothing(self):
        self.assertIsNone(one_thought(self.Tokenizer(), "<think></think>"))

    def test_a_vocabulary_without_the_markers_gets_nothing(self):
        tokenizer = self.Tokenizer()
        tokenizer.vocabulary = {}
        self.assertIsNone(one_thought(tokenizer, "<|channel>"))


class GemmaEmptyThought(unittest.TestCase):
    """After a tool result, a Gemma 4 reply with reasoning off starts after
    the empty thought block, as every other turn of its template does."""

    class Tokenizer:
        def encode(self, text, add_special_tokens=False):
            return [100, 5, 101] if text == "<|channel>thought\n<channel|>" else [0]

    def test_gemma_with_reasoning_off_gets_the_block(self):
        import pwr_mlx
        gemma = "<|tool_call> ... <|channel>"
        self.assertEqual(pwr_mlx.empty_thought(self.Tokenizer(), gemma, False), [100, 5, 101])
        self.assertEqual(pwr_mlx.empty_thought(self.Tokenizer(), gemma, None), [100, 5, 101])

    def test_reasoning_on_or_another_template_gets_nothing(self):
        import pwr_mlx
        self.assertEqual(pwr_mlx.empty_thought(self.Tokenizer(), "<|tool_call> <|channel>", True), [])
        self.assertEqual(pwr_mlx.empty_thought(self.Tokenizer(), "<tool_call> <think>", False), [])



class GemmaImages(unittest.TestCase):
    """Gemma 4 reads an image's tokens with attention both ways. PWR reads a
    prompt in steps and from checkpoints, so every step holding an image gets
    that mask, and no step cuts an image in two."""

    IMAGE = 9

    class Layer:
        def __init__(self, kind):
            self.layer_type = kind

    class Cache:
        def __init__(self):
            self.offset = 0
            self.state = None

    def vision(self, tokens):
        import mlx.core as mx
        import pwr_mlx
        test = self

        class Inner:
            layers = [test.Layer("sliding_attention"), test.Layer("full_attention")]
            window_size = 2

            def _make_masks(self, h, cache, types=None):
                return ["causal", "causal"]

        class Language:
            model = Inner()

            def __call__(self, inputs, inputs_embeds=None, cache=None, per_layer_inputs=None):
                n = inputs.shape[-1]
                test.steps.append((n, vision.step_blocks, inputs_embeds is not None))
                for c in cache:
                    c.offset += n
                return mx.zeros((1, n, 4))

        class Config:
            model_type = "gemma4"
            image_token_id = self.IMAGE

        class Vlm:
            language_model = Language()
            config = Config()

        self.steps = []
        vision = pwr_mlx.VisionText(Vlm())
        vision.blocks = pwr_mlx.image_blocks(tokens, self.IMAGE)
        vision.embeds = mx.zeros((1, len(tokens), 4))
        return vision

    def test_images_are_runs_of_the_image_token(self):
        import pwr_mlx
        self.assertEqual(pwr_mlx.image_blocks([1, 9, 9, 2, 9, 9, 9, 3], 9), [-1, 0, 0, -1, 1, 1, 1, -1])

    def test_a_step_ends_before_an_image_or_after_one_it_starts_with(self):
        vision = self.vision([1, 2, 9, 9, 9, 9, 3, 4])
        self.assertEqual(vision.whole_images(0, 4), 2)   # back to the image's start
        self.assertEqual(vision.whole_images(2, 4), 6)   # the step starts with it: to its end
        self.assertEqual(vision.whole_images(0, 2), 2)   # already before it
        self.assertEqual(vision.whole_images(0, 6), 6)   # already after it
        self.assertEqual(vision.whole_images(0, 9), 9)   # past the prompt

    def test_prefill_steps_never_cut_an_image(self):
        import pwr_mlx
        tokens = [1, 9, 9, 9, 9, 2, 3, 4]
        engine = pwr_mlx.Engine()
        engine.model = self.vision(tokens)
        engine.cache = [self.Cache(), self.Cache()]
        engine.prefill_step = lambda offset: 2
        engine.prefill(tokens, 0)
        self.assertEqual([n for n, _, _ in self.steps], [1, 4, 2, 1])
        self.assertEqual(self.steps[1][1], [0, 0, 0, 0])   # the image's step has its mask
        self.assertTrue(all(blocks is None for n, blocks, _ in self.steps if n != 4))
        self.assertTrue(all(embedded for _, _, embedded in self.steps))

    def test_the_image_sees_itself_both_ways_on_the_steps_new_keys(self):
        import mlx.core as mx
        import pwr_mlx

        class Model:
            layers = [self.Layer("sliding_attention"), self.Layer("full_attention")]
            window_size = 2

        # A step of four tokens after three in the cache: the keys are the
        # three kept and the four new ones.
        causal = mx.array([[j <= 3 + i for j in range(7)] for i in range(4)])
        blocks = [-1, 0, 0, -1]
        masks = pwr_mlx.image_masks(Model(), [causal, causal], mx.zeros((1, 4, 8)), [None, None], blocks)
        for mask in masks:
            self.assertTrue(mask[1, 5].item())       # image token 1 sees image token 2
            self.assertFalse(mask[0, 4].item())      # text does not see ahead
            self.assertFalse(mask[2, 6].item())      # nor does the image see the text after it
            self.assertTrue(mask[3, 0].item())       # what causal allowed stays
        # From an empty cache the model gave strings; a mask is built for them.
        fresh = pwr_mlx.image_masks(Model(), ["causal", "causal"], mx.zeros((1, 4, 8)), [None, None], blocks)
        self.assertEqual(fresh[1].shape, (4, 4))
        self.assertTrue(fresh[1][1, 2].item())
        self.assertFalse(fresh[0][0, 3].item())     # the window still holds outside the image

    def test_no_image_in_the_step_keeps_the_models_masks(self):
        import pwr_mlx
        self.assertEqual(pwr_mlx.image_masks(None, ["causal"], None, None, None), ["causal"])

    def test_render_passes_every_array_and_the_image_budget(self):
        import base64, io
        import numpy as np
        import pwr_mlx
        from PIL import Image
        raw = io.BytesIO(); Image.new("RGB", (4, 4)).save(raw, format="PNG")
        messages = [{"role": "user", "content": [{"type": "image", "data": base64.b64encode(raw.getvalue()).decode()},
                                                 {"type": "text", "text": "what?"}]}]
        seen = {}

        class Tokenizer:
            chat_template = ""

            def apply_chat_template(self, messages, **kwargs):
                return "<|image|>what?"

        def processor(text, images, return_tensors, **kwargs):
            seen.update(kwargs)
            return {"input_ids": np.array([[9, 9, 1]]), "attention_mask": np.ones((1, 3)),
                    "pixel_values": np.zeros((1, 2, 3)), "image_position_ids": np.zeros((1, 2, 2)),
                    "mm_token_type_ids": np.array([[1, 1, 0]])}

        was = pwr_mlx.IMAGE_TOKENS
        self.addCleanup(lambda: setattr(pwr_mlx, "IMAGE_TOKENS", was))
        pwr_mlx.IMAGE_TOKENS = 1120
        engine = pwr_mlx.Engine()
        engine.tokenizer, engine.processor = Tokenizer(), processor
        tokens, images = engine.render(messages, None, False, True)
        self.assertEqual(tokens, [9, 9, 1])
        self.assertEqual(sorted(images["arrays"]), ["image_position_ids", "mm_token_type_ids", "pixel_values"])
        self.assertEqual(seen, {"max_soft_tokens": 1120})


class GemmaHistory(unittest.TestCase):
    """Gemma 4's template, given PWR's history, renders it as the model
    writes it: text before the calls, and a turn ended on tool results closed
    before a user message."""

    PACK = pathlib.Path.home() / ".pwr/models/mlx-community/gemma-4-26b-a4b-it-4bit"

    def render(self, messages, patched=True):
        import pwr_mlx
        from mlx_lm.utils import load_tokenizer
        if not (self.PACK / "chat_template.jinja").exists():
            self.skipTest("no Gemma 4 pack on this machine")
        tokenizer = load_tokenizer(self.PACK)
        if patched:
            # The engine's own rendering, as a chat gets it.
            engine = pwr_mlx.Engine()
            engine.tokenizer = tokenizer
            tokens, _ = engine.render(messages, None, False, True)
            return tokenizer.decode(tokens)
        return tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=True,
                                             enable_thinking=False, chat_template=pwr_mlx.GEMMA_TEMPLATE)

    CALL = {"role": "assistant", "content": "Reading the parser.",
            "tool_calls": [{"id": "c1", "type": "function",
                            "function": {"name": "read_file", "arguments": {"path": "src/a.py"}}}]}
    RESULT = {"role": "tool", "tool_call_id": "c1", "content": "def a(): pass"}

    def test_the_text_comes_before_the_call_and_the_turn_stays_open_on_results(self):
        text = self.render([{"role": "user", "content": "Fix a."}, self.CALL, self.RESULT])
        self.assertIn("<|turn>model\nReading the parser.<|tool_call>call:read_file{path:", text)
        self.assertTrue(text.endswith("<tool_response|>"), text[-80:])

    def test_a_user_message_after_results_closes_the_model_turn(self):
        text = self.render([{"role": "user", "content": "Fix a."}, self.CALL, self.RESULT,
                            {"role": "user", "content": "[PWR] checks: 2 failed"}])
        self.assertIn("<tool_response|><turn|>\n<|turn>user\n[PWR] checks: 2 failed<turn|>\n", text)
        self.assertTrue(text.endswith("<|turn>model\n<|channel>thought\n<channel|>"), text[-80:])

    def test_unpatched_template_shows_what_was_wrong(self):
        text = self.render([{"role": "user", "content": "Fix a."}, self.CALL, self.RESULT,
                            {"role": "user", "content": "next"}], patched=False)
        self.assertIn("<tool_response|>Reading the parser.<turn|>", text)

    def test_results_reach_gemma_as_mappings_with_the_file_text_unescaped(self):
        result = {"role": "tool", "tool_call_id": "c1", "content":
                  '{"result":{"content":"<content> below","path":"src/a.py","first_line":1,"redacted":false,"note":null}}'
                  '\n\n<content>\n\ndef a():\n    return "x"\n</content>'}
        text = self.render([{"role": "user", "content": "Fix a."}, self.CALL, result,
                            {"role": "user", "content": "next"}])
        self.assertIn('<|tool_response>response:read_file{content:<|"|>\ndef a():\n    return "x"<|"|>,'
                      'first_line:1,path:<|"|>src/a.py<|"|>,redacted:false}<tool_response|><turn|>\n', text)

    def test_a_user_message_after_results_keeps_the_copy_a_prefix(self):
        """D.E2E-21: a message after a tool result -- a notice, the checks'
        report -- must not re-render the history before it, or the next
        prompt cannot resume from the copy."""
        import pwr_mlx
        from mlx_lm.utils import load_tokenizer
        if not (self.PACK / "chat_template.jinja").exists():
            self.skipTest("no Gemma 4 pack on this machine")
        engine = pwr_mlx.Engine()
        engine.tokenizer = load_tokenizer(self.PACK)
        pwr_mlx.add_turn_ends(engine.tokenizer)
        history = [{"role": "system", "content": "sys"}, {"role": "user", "content": "Fix a."}, self.CALL, self.RESULT]
        for thinking in (True, False):
            before = engine.render(history, None, thinking, False)[0]
            after = engine.render(history + [{"role": "user", "content": "[PWR] checks: 2 failed"}], None, thinking, False)[0]
            copy = before[:engine.checkpoint_point(before)]
            self.assertEqual(after[:len(copy)], copy, thinking)
            self.assertEqual(after[:len(before)], before, thinking)

    def test_gemmas_stops_end_generation(self):
        import pwr_mlx
        from mlx_lm.utils import load_tokenizer
        if not (self.PACK / "chat_template.jinja").exists():
            self.skipTest("no Gemma 4 pack on this machine")
        tokenizer = load_tokenizer(self.PACK)
        pwr_mlx.add_turn_ends(tokenizer)
        stops = {tokenizer.convert_tokens_to_ids(m) for m in ("<turn|>", "<|tool_response>")}
        self.assertLessEqual(stops, set(tokenizer.eos_token_ids))
        self.assertNotIn(tokenizer.convert_tokens_to_ids("<tool_call|>"), tokenizer.eos_token_ids)

    def test_the_template_is_googles_of_july(self):
        import hashlib
        import pwr_mlx
        self.assertEqual(hashlib.sha256(pwr_mlx.GEMMA_TEMPLATE.encode()).hexdigest(),
                         "ae53464bf3be25802b3a5b37def7fd89667067d7577049b3b2d74c4d8de4c6d4")

    def test_with_thinking_on_the_model_thinks_after_results(self):
        """Google's July template: a model turn going on after tool results
        opens the thought channel when thinking is on."""
        import pwr_mlx
        from mlx_lm.utils import load_tokenizer
        if not (self.PACK / "chat_template.jinja").exists():
            self.skipTest("no Gemma 4 pack on this machine")
        engine = pwr_mlx.Engine()
        engine.tokenizer = load_tokenizer(self.PACK)
        tokens, _ = engine.render([{"role": "user", "content": "Fix a."}, self.CALL, self.RESULT], None, True, True)
        self.assertTrue(engine.tokenizer.decode(tokens).endswith("<tool_response|><|channel>thought\n"))

    def test_another_template_is_left_alone(self):
        import pwr_mlx
        self.assertEqual(pwr_mlx.gemma_history_template("{{ messages }}"), "{{ messages }}")


class ToolResultMapping(unittest.TestCase):
    def test_command_streams_are_restored_to_their_own_fields(self):
        import pwr_mlx
        for fields in ({"stdout": "\n città \\\"x\\\"\n", "stderr": "warning\n"},
                       {"stdout": "embedded <stderr>\nx\n</stderr>\n", "stderr": "real error"},
                       {"stdout": "out"}, {"stderr": "err"}, {"stdout": ""}):
            with self.subTest(fields=fields):
                meta = {"exit_code": 1, "stdout_truncated": True,
                        **{key: f"<{key}> below" for key in fields}}
                text = json.dumps({"result": meta}) + "".join(
                    f"\n\n<{key}>\n{value}\n</{key}>" for key, value in fields.items())
                self.assertEqual(pwr_mlx.tool_result_mapping(text), {**meta, **fields})

    def test_status_keeps_streams_inside_the_result(self):
        import pwr_mlx
        text = '{"result":{"stdout":"<stdout> below"},"status":{"actions":4}}\n\n<stdout>\nhello\n</stdout>'
        self.assertEqual(pwr_mlx.tool_result_mapping(text),
                         {"result": {"stdout": "hello"}, "status": {"actions": 4}})

    def test_unfinished_or_unexpected_blocks_do_not_replace_observed_fields(self):
        import pwr_mlx
        for suffix in ("<stdout>\npartial", "<other>\nx\n</other>"):
            self.assertEqual(pwr_mlx.tool_result_mapping(
                '{"result":{"stdout":"<stdout> below"}}\n\n' + suffix),
                {"stdout": "<stdout> below", "text": suffix})
        literal = {"stdout": "literal </stdout> in JSON"}
        self.assertEqual(pwr_mlx.tool_result_mapping(json.dumps({"result": literal})), literal)
        # A complete first stream does not justify accepting a partial second.
        partial = '<stdout>\ncomplete\n</stdout>\n\n<stderr>\npartial'
        metadata = {"stdout": "<stdout> below", "stderr": "<stderr> below"}
        self.assertEqual(pwr_mlx.tool_result_mapping(json.dumps({"result": metadata}) + '\n\n' + partial),
                         {**metadata, "text": partial})
        extra = '<stdout>\nunexpected\n</stdout>'
        self.assertEqual(pwr_mlx.tool_result_mapping(json.dumps({"result": literal}) + '\n\n' + extra),
                         {**literal, "text": extra})

    def test_json_with_file_text(self):
        import pwr_mlx
        mapping = pwr_mlx.tool_result_mapping('{"result":{"content":"<content> below","n":2}}\n\n<content>\n\nx\n</content>')
        self.assertEqual(mapping, {"content": "\nx", "n": 2})

    def test_json_alone_and_errors_keep_their_shape(self):
        import pwr_mlx
        self.assertEqual(pwr_mlx.tool_result_mapping('{"result":{"ok":true}}'), {"ok": True})
        self.assertEqual(pwr_mlx.tool_result_mapping('{"error":{"kind":"x"}}'), {"error": {"kind": "x"}})
        self.assertEqual(pwr_mlx.tool_result_mapping('{"result":{"a":1}} trailing'), {"a": 1, "text": "trailing"})

    def test_plain_text_is_not_a_mapping(self):
        import pwr_mlx
        self.assertIsNone(pwr_mlx.tool_result_mapping("command not found"))
        self.assertIsNone(pwr_mlx.tool_result_mapping("[1, 2]"))

    def test_results_move_to_the_reply_only_when_all_are_mappings(self):
        import pwr_mlx
        call = {"role": "assistant", "content": "", "tool_calls": [
            {"id": "a", "function": {"name": "read_file", "arguments": {}}},
            {"id": "b", "function": {"name": "search", "arguments": {}}}]}
        ok = [call, {"role": "tool", "tool_call_id": "a", "content": '{"result":{"x":1}}'},
              {"role": "tool", "tool_call_id": "b", "content": '{"result":{"y":2}}'}, {"role": "user", "content": "go"}]
        shaped = pwr_mlx.gemma_tool_results(ok)
        self.assertEqual([m["role"] for m in shaped], ["assistant", "user"])
        self.assertEqual(shaped[0]["tool_responses"], [{"name": "read_file", "response": {"x": 1}},
                                                        {"name": "search", "response": {"y": 2}}])
        mixed = [call, {"role": "tool", "tool_call_id": "a", "content": '{"result":{"x":1}}'},
                 {"role": "tool", "tool_call_id": "b", "content": [{"type": "image"}]}]
        self.assertEqual(pwr_mlx.gemma_tool_results(mixed), mixed)


class PromptDump(unittest.TestCase):
    def test_every_request_is_written_whole_with_its_tokens(self):
        import pwr_mlx

        class Tokenizer:
            def decode(self, tokens):
                return "|".join(map(str, tokens))

        was = pwr_mlx.PROMPT_DUMP
        self.addCleanup(lambda: setattr(pwr_mlx, "PROMPT_DUMP", was))
        with tempfile.TemporaryDirectory() as folder:
            pwr_mlx.PROMPT_DUMP = folder
            pwr_mlx.dump_prompt({"messages": [{"role": "user", "content": "hi"}], "temperature": 1.0}, [1, 2], Tokenizer())
            pwr_mlx.dump_prompt({"messages": []}, [3], Tokenizer())
            first = json.loads((pathlib.Path(folder) / "prompt-0000.json").read_text())
            self.assertEqual(first["tokens"], [1, 2])
            self.assertEqual(first["text"], "1|2")
            self.assertEqual(first["request"]["temperature"], 1.0)
            self.assertTrue((pathlib.Path(folder) / "prompt-0001.json").exists())


if __name__ == "__main__":
    unittest.main()



class Stepped:
    """A model stand-in for prefill: a function over each step, no images."""

    def __init__(self, step):
        self.step = step

    def __call__(self, tokens, cache):
        return self.step(tokens, cache)

    def whole_images(self, start, end):
        return end


class PrefillCancellation(unittest.TestCase):
    def test_long_unfused_prefill_can_shrink_below_the_old_256_token_floor(self):
        import mlx.core as mx
        from unittest.mock import patch
        from types import SimpleNamespace
        from pwr_mlx import Engine, PREFILL_FIRST_STEP, PREFILL_UNFUSED_CHUNK_SECS
        engine = Engine()
        engine.cache = [SimpleNamespace(state=mx.array([0]))]
        engine.fused = False
        engine.prefill_step = lambda offset: 8192
        clock, sizes = [0.0], []

        def slow(tokens, cache):
            sizes.append(tokens.shape[1])
            clock[0] += tokens.shape[1] / 50  # long context: 50 tokens/sec

        engine.model = Stepped(slow)
        with patch("pwr_mlx.time.perf_counter", side_effect=lambda: clock[0]):
            engine.prefill(list(range(1000)), 108000)
        self.assertEqual(sum(sizes), 1000)
        self.assertEqual(sizes[0], PREFILL_FIRST_STEP)
        self.assertTrue(all(n <= 50 * PREFILL_UNFUSED_CHUNK_SECS for n in sizes[1:]), sizes)

    def test_a_slow_model_gets_short_chunks_so_a_stop_is_seen_soon(self):
        import time
        import mlx.core as mx
        import pwr_mlx
        from pwr_mlx import Engine
        from types import SimpleNamespace
        engine = Engine.__new__(Engine)
        engine.cache = [SimpleNamespace(state=mx.array([0]))]
        engine.prefill_step = lambda offset: pwr_mlx.PREFILL_STEP
        sizes = []

        def slow(tokens, cache):
            sizes.append(tokens.shape[1])
            time.sleep(tokens.shape[1] * 0.002)  # 500 tokens a second
        engine.model = Stepped(slow)
        original = pwr_mlx.PREFILL_CHUNK_SECS
        pwr_mlx.PREFILL_CHUNK_SECS = 0.3  # 150 tokens' worth, rounded down
        try:
            engine.prefill(list(range(2000)), 0)
        finally:
            pwr_mlx.PREFILL_CHUNK_SECS = original
        self.assertEqual(sum(sizes), 2000)
        self.assertEqual(sizes[0], pwr_mlx.PREFILL_FIRST_STEP)
        self.assertTrue(all(size <= 150 for size in sizes[1:-1]), sizes)

    def test_a_fast_model_is_let_run_in_big_chunks(self):
        import mlx.core as mx
        import pwr_mlx
        from pwr_mlx import Engine
        from types import SimpleNamespace
        engine = Engine.__new__(Engine)
        engine.cache = [SimpleNamespace(state=mx.array([0]))]
        engine.prefill_step = lambda offset: pwr_mlx.PREFILL_STEP
        sizes = []
        engine.model = Stepped(lambda tokens, cache: sizes.append(tokens.shape[1]))
        engine.prefill(list(range(20000)), 0)
        self.assertEqual(sum(sizes), 20000)
        self.assertEqual(sizes[0], pwr_mlx.PREFILL_FIRST_STEP)
        self.assertGreater(max(sizes), pwr_mlx.PREFILL_FIRST_STEP)

    def test_stop_is_observed_between_chunks(self):
        import mlx.core as mx
        from pwr_mlx import Engine, PrefillCancelled
        from types import SimpleNamespace
        engine = Engine.__new__(Engine)
        engine.cache = [SimpleNamespace(state=mx.array([0]))]
        engine.prefill_step = lambda offset: 2
        calls = []
        engine.model = Stepped(lambda tokens, cache: calls.append(tokens))
        with self.assertRaises(PrefillCancelled):
            engine.prefill([1, 2, 3, 4, 5, 6], 0, cancelled=lambda: bool(calls))
        self.assertEqual(len(calls), 1)
