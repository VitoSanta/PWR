"""The paired analysis against values worked out by hand."""

import json
import math
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import analyze  # noqa: E402


class Statistics(unittest.TestCase):
    def test_wilson_interval(self):
        low, high = analyze.wilson(5, 10)
        self.assertAlmostEqual(low, 0.2366, places=4)
        self.assertAlmostEqual(high, 0.7634, places=4)
        self.assertTrue(all(math.isnan(x) for x in analyze.wilson(0, 0)))

    def test_exact_mcnemar(self):
        # 12 discordant pairs split 10/2: 2 * (C(12,0)+C(12,1)+C(12,2)) / 2^12.
        self.assertAlmostEqual(analyze.mcnemar_exact(10, 2), 2 * 79 / 4096)
        self.assertEqual(analyze.mcnemar_exact(0, 0), 1.0)
        self.assertEqual(analyze.mcnemar_exact(3, 3), 1.0)

    def test_holm_adjustment(self):
        adjusted = analyze.holm([0.01, 0.04, 0.03])
        for got, want in zip(adjusted, [0.03, 0.06, 0.06]):
            self.assertAlmostEqual(got, want)

    def test_sign_flip_is_exact_for_few_tasks(self):
        # Three tasks all favouring A: 2 of the 8 sign patterns are as extreme.
        self.assertAlmostEqual(analyze.sign_flip_p({"a": 1, "b": 1, "c": 1}, {"a": 0, "b": 0, "c": 0}), 0.25)

    def test_no_effect_has_power_at_most_alpha(self):
        self.assertLessEqual(analyze.mcnemar_power(50, 0.1, 0.1), 0.05)

    def test_power_agrees_with_the_normal_approximation(self):
        # Connor (1987): n = (z_a/2 sqrt(pd) + z_b sqrt(pd - delta^2))^2 / delta^2.
        for p10, p01 in [(0.25, 0.05), (0.12, 0.04), (0.3, 0.1)]:
            pd, delta = p10 + p01, p10 - p01
            approx = (1.959964 * math.sqrt(pd) + 0.841621 * math.sqrt(pd - delta ** 2)) ** 2 / delta ** 2
            exact = analyze.tasks_for_power(p10, p01)
            # The exact test is conservative: a few more tasks, not fewer.
            self.assertGreaterEqual(exact, math.ceil(approx) - 1, (p10, p01))
            self.assertLessEqual(exact, approx * 1.15, (p10, p01))


def write_result(root, run, task, attempt, turns, window=32768, split="dev"):
    folder = root / run / (task if attempt == 1 else f"{task}~{attempt}")
    folder.mkdir(parents=True)
    result = {"task": task, "attempt": attempt, "split": split, "passed": turns[-1], "minutes": 10.0,
              "turns": [{"turn": i + 1, "passed": p} for i, p in enumerate(turns)],
              "provenance": {"window": {"granted_tokens": window}}}
    (folder / "result.json").write_text(json.dumps(result))


class Compare(unittest.TestCase):
    def test_first_cycle_and_nudged_outcomes_are_compared_apart(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            # A: t1 first cycle, t2 after a nudge, t3 fails, t4 first cycle.
            for task, turns in {"t1": [True], "t2": [False, True], "t3": [False, False], "t4": [True]}.items():
                write_result(root, "a", task, 1, turns)
            # B: only t1 passes, first cycle; B has no t4 at all.
            for task, turns in {"t1": [True], "t2": [False, False], "t3": [False, False]}.items():
                write_result(root, "b", task, 1, turns)
            report = analyze.compare([("A", root / "a"), ("B", root / "b")])
        arm_a = report["arms"][0]
        self.assertEqual(arm_a["first_cycle"]["passed"], 2)
        self.assertEqual(arm_a["final"]["passed"], 3)
        self.assertEqual(arm_a["nudges"], 2)
        first = next(c for c in report["comparisons"] if c["outcome"] == "first_cycle")
        final = next(c for c in report["comparisons"] if c["outcome"] == "final")
        # Only the three shared tasks are paired.
        self.assertEqual(first["pairs"], 3)
        self.assertEqual((first["a_only"], first["b_only"]), (0, 0))
        self.assertEqual((final["a_only"], final["b_only"]), (1, 0))
        self.assertEqual(final["test"], "exact McNemar")
        self.assertAlmostEqual(final["p"], 1.0)

    def test_an_unequal_condition_between_arms_is_named(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            write_result(root, "a", "t1", 1, [True], window=32768)
            write_result(root, "b", "t1", 1, [True], window=65536)
            report = analyze.compare([("A", root / "a"), ("B", root / "b")])
        self.assertIn("window", report["comparisons"][0]["differing_conditions"])

    def test_repeated_trials_are_tested_over_tasks_not_trials(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            for attempt in (1, 2):
                write_result(root, "a", "t1", attempt, [True])
                write_result(root, "b", "t1", attempt, [attempt == 1])
            report = analyze.compare([("A", root / "a"), ("B", root / "b")])
        comparison = report["comparisons"][0]
        self.assertEqual(comparison["test"], "paired sign-flip over tasks")
        self.assertAlmostEqual(comparison["difference"], 0.5)
        self.assertAlmostEqual(report["arms"][1]["first_cycle"]["between_trial_variance"], 0.5)


if __name__ == "__main__":
    unittest.main()
