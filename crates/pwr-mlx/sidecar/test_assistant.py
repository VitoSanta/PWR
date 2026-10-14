"""Regressions for target-only constraints and speculative cache alignment."""
import types
import unittest
from unittest.mock import patch
import mlx.core as mx
import pwr_mlx as p


class AssistantConstraints(unittest.TestCase):
    def test_target_masks_both_markers_within_one_verified_block(self):
        sample = p.ThoughtSampler(lambda scores: mx.argmax(scores, axis=-1), p.OneThought(1, 2), [9])
        # open, close, then an attempted second close/open in the SAME block.
        logits = mx.array([[0., 9., 0., 1.], [0., 8., 9., 1.], [0., 8., 9., 1.]])
        self.assertEqual(sample.sample_target(logits, row_ids=[0]*3, positions=[0, 1, 2]).tolist(), [1, 2, 3])

    def test_previous_turn_markers_do_not_close_this_turn_and_drafts_are_stateless(self):
        sample = p.ThoughtSampler(lambda scores: mx.argmax(scores, axis=-1), p.OneThought(1, 2), [1, 2, 9])
        scores = mx.array([[0., 9., 0., 1.]])
        sample(scores)  # A proposal is not an accepted target token.
        self.assertEqual(sample.states, {0: (False, False)})
        self.assertEqual(sample.sample_target(scores, row_ids=[0], positions=[0]).item(), 1)

    def test_preclosed_prompt_and_rejected_suffix(self):
        sample = p.ThoughtSampler(lambda scores: mx.argmax(scores, axis=-1), p.OneThought(1, 2), [2])
        self.assertEqual(sample.sample_target(mx.array([[0., 9., 8., 1.]]), row_ids=[0], positions=[0]).item(), 3)
        # Revisit an uncommitted position; its prefix state, not a later state, wins.
        sample = p.ThoughtSampler(lambda scores: mx.argmax(scores, axis=-1), p.OneThought(1, 2), [9])
        sample.sample_target(mx.array([[0., 9., 0., 1.], [0., 0., 9., 1.]]), row_ids=[0, 0], positions=[0, 1])
        self.assertEqual(sample.sample_target(mx.array([[0., 9., 0., 10.]]), row_ids=[0], positions=[0]).item(), 3)
        self.assertEqual(sample.states[1], (False, False))

    def test_fused_argmax_cannot_bypass_constraints(self):
        model = types.SimpleNamespace(model=object(), speculative_argmax_from_hidden=lambda _: 2, speculative_verify_logits=lambda *_: 2, config='config')
        target = p.ConstrainedTarget(model)
        self.assertFalse(hasattr(target, 'speculative_argmax_from_hidden'))
        self.assertFalse(hasattr(target, 'speculative_verify_logits'))
        self.assertIs(target.model, model.model)
        self.assertEqual(target.config, 'config')

    def test_forced_reasoning_close_survives_a_new_speculative_stream(self):
        thought = p.OneThought(1, 2)
        thought.shut = thought.closed = True
        sample = p.ThoughtSampler(lambda scores: mx.argmax(scores, axis=-1), thought, [9])
        self.assertEqual(sample.sample_target(mx.array([[0.,9.,8.,1.]]), row_ids=[0], positions=[0]).item(), 3)

    def test_cache_settles_partial_round_and_pending_last_token(self):
        class Layer:
            def __init__(self, offset): self.offset = offset
            def trim(self, count): self.offset -= count
        engine = p.Engine();engine.cache = [Layer(20), Layer(20)]
        engine.settle(10, [7, 8])
        self.assertEqual([c.offset for c in engine.cache], [12, 12])
        calls=[]
        def model(tokens, cache):
            calls.append(tokens.tolist())
            for layer in cache:layer.offset += tokens.shape[-1]
            return mx.array([0])
        engine.model=model;engine.cache=[Layer(11)]
        engine.settle(10,[7,8])
        self.assertEqual(calls, [[[8]]]);self.assertEqual(engine.cache[0].offset,12)

    def test_quantized_cache_and_off_skip_assistant_loading(self):
        with patch.object(p, 'ASSISTANT', True), patch.object(p, 'KV_BITS', 8):
            draft, reason=p.load_assistant(None, '/missing')
            self.assertIsNone(draft);self.assertIn('8 bits',reason)
        with patch.object(p, 'ASSISTANT', False):
            draft, reason=p.load_assistant(None, '/missing')
            self.assertIsNone(draft);self.assertIn('off',reason)
