#!/usr/bin/env python3
"""Paired analysis of stack-matrix campaigns, under the MASTER_SPEC statistics.

    analyze.py compare --arm NAME=RUN [--arm NAME=RUN ...] [--split heldout] [--json]
    analyze.py power --p10 P --p01 P [--alpha 0.05] [--power 0.8]

Each arm is one run folder (`~/Desktop/pwr-evidence/runs/<RUN>`), a result per
task and attempt. Arms are compared only on the (task, attempt) pairs they
share, so a task one arm skipped is not a loss for the other.

Two outcomes are kept apart, as the contract asks: *first cycle* -- the task
passed after the brief alone, unattended -- and *final* -- passed after the
runner's generic nudges, an intervention by an external oracle.

No third-party packages: the exact tests are small enough to compute directly.
"""

import argparse
import json
import math
import os
import pathlib
import random
import statistics
import sys

RUNS = pathlib.Path(os.environ.get("PWR_EVIDENCE_RESULTS", pathlib.Path.home() / "Desktop/pwr-evidence/runs"))


# ---------------------------------------------------------------- statistics


def wilson(successes, n, z=1.959963984540054):
    """Wilson score interval for a proportion; (nan, nan) for no trials."""
    if n == 0:
        return (math.nan, math.nan)
    p = successes / n
    centre = (p + z * z / (2 * n)) / (1 + z * z / n)
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / (1 + z * z / n)
    return (max(0.0, centre - half), min(1.0, centre + half))


def mcnemar_exact(b, c):
    """Two-sided exact McNemar p-value: b pairs only A passed, c only B passed."""
    n = b + c
    if n == 0:
        return 1.0
    tail = sum(math.comb(n, k) for k in range(0, min(b, c) + 1)) / 2 ** n
    return min(1.0, 2 * tail)


def holm(pvalues):
    """Holm step-down adjusted p-values, in the input order."""
    order = sorted(range(len(pvalues)), key=lambda i: pvalues[i])
    adjusted = [0.0] * len(pvalues)
    running = 0.0
    for rank, index in enumerate(order):
        running = max(running, min(1.0, (len(pvalues) - rank) * pvalues[index]))
        adjusted[index] = running
    return adjusted


def bootstrap_difference(per_task_a, per_task_b, rounds=20000, seed=0):
    """95% percentile interval of mean(A) - mean(B), resampling tasks."""
    tasks = sorted(set(per_task_a) & set(per_task_b))
    if not tasks:
        return (math.nan, math.nan)
    differences = [per_task_a[t] - per_task_b[t] for t in tasks]
    rng = random.Random(seed)
    means = sorted(statistics.fmean(rng.choices(differences, k=len(differences))) for _ in range(rounds))
    return (means[int(0.025 * rounds)], means[int(0.975 * rounds) - 1])


def sign_flip_p(per_task_a, per_task_b, rounds=100000, seed=0):
    """Two-sided paired permutation p-value over tasks (exact up to 20 tasks)."""
    differences = [per_task_a[t] - per_task_b[t] for t in sorted(set(per_task_a) & set(per_task_b))]
    differences = [d for d in differences if d != 0]
    if not differences:
        return 1.0
    observed = abs(sum(differences))
    if len(differences) <= 20:
        hits = 0
        for mask in range(2 ** len(differences)):
            total = sum(d if mask >> i & 1 else -d for i, d in enumerate(differences))
            hits += abs(total) >= observed - 1e-12
        return hits / 2 ** len(differences)
    rng = random.Random(seed)
    hits = sum(abs(sum(d if rng.random() < 0.5 else -d for d in differences)) >= observed - 1e-12
               for _ in range(rounds))
    return (hits + 1) / (rounds + 1)


def _log_pmf(n, k, p):
    """log P(Binomial(n, p) = k), finite for any n the tables need."""
    if p <= 0:
        return 0.0 if k == 0 else -math.inf
    if p >= 1:
        return 0.0 if k == n else -math.inf
    return (math.lgamma(n + 1) - math.lgamma(k + 1) - math.lgamma(n - k + 1)
            + k * math.log(p) + (n - k) * math.log1p(-p))


def _rejection_bound(d, alpha):
    """The largest m such that the exact McNemar test rejects min(b, c) = m of
    d discordant pairs; -1 when no split of d rejects."""
    tail, bound = 0.0, -1
    for m in range(d // 2 + 1):
        tail += math.exp(_log_pmf(d, m, 0.5))
        if 2 * tail > alpha:
            break
        bound = m
    return bound


class McNemarPower:
    """Exact power of the two-sided exact McNemar test.

    p10: probability a task passes under A and fails under B; p01 the reverse.
    With n paired tasks the discordant count is Binomial(n, p10 + p01); given
    d of them, A's share is Binomial(d, p10 / (p10 + p01)). The conditional
    rejection probability depends on d alone, so it is computed once per d.
    """

    def __init__(self, p10, p01, alpha=0.05):
        self.pd = p10 + p01
        self.share = p10 / self.pd if self.pd > 0 else 0.5
        self.alpha = alpha
        self.rejecting = []

    def _given(self, d):
        while len(self.rejecting) <= d:
            k = len(self.rejecting)
            m = _rejection_bound(k, self.alpha)
            if m < 0:
                self.rejecting.append(0.0)
                continue
            low = sum(math.exp(_log_pmf(k, b, self.share)) for b in range(m + 1))
            high = sum(math.exp(_log_pmf(k, b, self.share)) for b in range(k - m, k + 1))
            self.rejecting.append(min(1.0, low + high))
        return self.rejecting[d]

    def power(self, n):
        if self.pd <= 0:
            return 0.0
        return sum(math.exp(_log_pmf(n, d, self.pd)) * self._given(d) for d in range(n + 1))


def mcnemar_power(n, p10, p01, alpha=0.05):
    return McNemarPower(p10, p01, alpha).power(n)


def tasks_for_power(p10, p01, alpha=0.05, target=0.8, limit=2000):
    """The smallest number of paired tasks reaching `target` power, or None.

    Exact power is not monotone in n (the test is discrete): a size just above
    the one returned can fall slightly short, so the report gives the power at
    the sizes around it.
    """
    calculator = McNemarPower(p10, p01, alpha)
    for n in range(1, limit + 1):
        if calculator.power(n) >= target:
            return n
    return None


# ---------------------------------------------------------------- results


def load_run(path, split=None):
    """{(task, attempt): result} for one run folder."""
    found = {}
    for result_path in sorted(pathlib.Path(path).glob("*/result.json")):
        result = json.loads(result_path.read_text())
        if split and result.get("split") != split:
            continue
        found[(result["task"], result.get("attempt", 1))] = result
    return found


def first_cycle(result):
    turns = result.get("turns") or []
    return bool(turns) and bool(turns[0].get("passed"))


def final(result):
    return bool(result.get("passed"))


def per_task_rate(results, outcome):
    """Unbiased pass@1 per task: the share of its trials that passed."""
    by_task = {}
    for (task, _), result in results.items():
        by_task.setdefault(task, []).append(outcome(result))
    return {task: sum(values) / len(values) for task, values in by_task.items()}


def between_trial_variance(results, outcome):
    """Mean over tasks with two or more trials of the unbiased Bernoulli variance."""
    by_task = {}
    for (task, _), result in results.items():
        by_task.setdefault(task, []).append(outcome(result))
    variances = [statistics.variance(v) for v in by_task.values() if len(v) >= 2]
    return statistics.fmean(variances) if variances else None


PROVENANCE_FIELDS = [
    ("model", lambda r: (r.get("provenance") or {}).get("model", {}).get("revision") or r.get("model")),
    ("binary", lambda r: (r.get("provenance") or {}).get("binary_digest") or r.get("revision")),
    ("sidecar", lambda r: r.get("sidecar")),
    ("window", lambda r: ((r.get("provenance") or {}).get("window") or {}).get("granted_tokens")),
    ("sampling", lambda r: json.dumps({k: v.get("value") for k, v in
                                       (((r.get("provenance") or {}).get("sampling")) or {}).items()
                                       if isinstance(v, dict)}, sort_keys=True)),
    ("protocol", lambda r: r.get("protocol")),
    ("engine", lambda r: json.dumps((r.get("provenance") or {}).get("engine_libraries"), sort_keys=True)),
]


def conditions(results):
    """Each provenance field's distinct values across an arm's results."""
    return {name: sorted({str(read(r)) for r in results.values()}) for name, read in PROVENANCE_FIELDS}


def summarise(name, results):
    trials = len(results)
    first = sum(first_cycle(r) for r in results.values())
    passed = sum(final(r) for r in results.values())
    attempts = {}
    for (task, attempt) in results:
        attempts.setdefault(task, set()).add(attempt)
    single = all(len(a) == 1 for a in attempts.values())
    minutes = [r.get("minutes") for r in results.values() if r.get("minutes") is not None]
    return {
        "arm": name, "tasks": len(attempts), "trials": trials,
        "first_cycle": {"passed": first,
                        "pass_at_1": statistics.fmean(per_task_rate(results, first_cycle).values()) if trials else None,
                        "wilson_95": wilson(first, trials) if single else None,
                        "between_trial_variance": between_trial_variance(results, first_cycle)},
        "final": {"passed": passed,
                  "pass_at_1": statistics.fmean(per_task_rate(results, final).values()) if trials else None,
                  "wilson_95": wilson(passed, trials) if single else None,
                  "between_trial_variance": between_trial_variance(results, final)},
        "harness_errors": sum("error" in r for r in results.values()),
        "nudges": sum(max(0, len(r.get("turns") or []) - 1) for r in results.values()),
        "minutes_median": statistics.median(minutes) if minutes else None,
        "generated_tokens": sum(((r.get("tokens") or {}).get("generated") or 0) for r in results.values()),
        "actions": sum(r.get("actions") or 0 for r in results.values()),
        "conditions": conditions(results),
    }


def compare_pair(a_name, a, b_name, b, outcome):
    shared = sorted(set(a) & set(b))
    only_a = sum(outcome(a[k]) and not outcome(b[k]) for k in shared)
    only_b = sum(outcome(b[k]) and not outcome(a[k]) for k in shared)
    rate_a = per_task_rate({k: a[k] for k in shared}, outcome)
    rate_b = per_task_rate({k: b[k] for k in shared}, outcome)
    single = len({task for task, _ in shared}) == len(shared)
    return {
        "a": a_name, "b": b_name, "pairs": len(shared),
        "a_only": only_a, "b_only": only_b,
        "difference": (statistics.fmean(rate_a.values()) - statistics.fmean(rate_b.values())) if shared else None,
        "bootstrap_95": bootstrap_difference(rate_a, rate_b),
        # One trial per task: exact McNemar. Repeated trials are not new
        # tasks, so their test is the paired permutation over task means.
        "test": "exact McNemar" if single else "paired sign-flip over tasks",
        "p": mcnemar_exact(only_a, only_b) if single else sign_flip_p(rate_a, rate_b),
        "differing_conditions": {
            name: [conditions({k: a[k] for k in shared}).get(name), conditions({k: b[k] for k in shared}).get(name)]
            for name, _ in PROVENANCE_FIELDS
            if conditions({k: a[k] for k in shared}).get(name) != conditions({k: b[k] for k in shared}).get(name)
        },
    }


def compare(arms, split=None):
    loaded = {name: load_run(path, split) for name, path in arms}
    report = {"split": split, "arms": [summarise(name, results) for name, results in loaded.items()],
              "comparisons": []}
    names = list(loaded)
    for outcome_name, outcome in (("first_cycle", first_cycle), ("final", final)):
        pairs = [compare_pair(x, loaded[x], y, loaded[y], outcome)
                 for i, x in enumerate(names) for y in names[i + 1:]]
        for pair, adjusted in zip(pairs, holm([p["p"] for p in pairs])):
            pair["outcome"] = outcome_name
            pair["p_holm"] = adjusted
        report["comparisons"].extend(pairs)
    return report


def fmt(value, digits=3):
    if value is None or (isinstance(value, float) and math.isnan(value)):
        return "-"
    return f"{value:.{digits}f}" if isinstance(value, float) else str(value)


def print_report(report):
    print(f"split: {report['split'] or 'all'}")
    for arm in report["arms"]:
        print(f"\n{arm['arm']}: {arm['tasks']} tasks, {arm['trials']} trials, "
              f"{arm['harness_errors']} harness errors, {arm['nudges']} nudges, "
              f"median {fmt(arm['minutes_median'], 1)} min")
        for label in ("first_cycle", "final"):
            o = arm[label]
            interval = o["wilson_95"]
            print(f"  {label:<11} {o['passed']}/{arm['trials']}  pass@1 {fmt(o['pass_at_1'])}"
                  + (f"  Wilson95 [{fmt(interval[0])}, {fmt(interval[1])}]" if interval else "")
                  + (f"  between-trial var {fmt(o['between_trial_variance'])}"
                     if o["between_trial_variance"] is not None else ""))
        varied = {k: v for k, v in arm["conditions"].items() if len(v) > 1}
        if varied:
            print(f"  ! conditions vary inside this arm: {varied}")
    for c in report["comparisons"]:
        print(f"\n{c['outcome']}: {c['a']} vs {c['b']} on {c['pairs']} pairs: "
              f"{c['a']} only {c['a_only']}, {c['b']} only {c['b_only']}, "
              f"difference {fmt(c['difference'])} [{fmt(c['bootstrap_95'][0])}, {fmt(c['bootstrap_95'][1])}], "
              f"{c['test']} p={fmt(c['p'], 4)}, Holm p={fmt(c['p_holm'], 4)}")
        if c["differing_conditions"]:
            print(f"  differs besides the arm: {c['differing_conditions']}")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)
    cmp = sub.add_parser("compare")
    cmp.add_argument("--arm", action="append", required=True, help="NAME=RUN (a folder under the runs root, or a path)")
    cmp.add_argument("--split")
    cmp.add_argument("--json", action="store_true")
    pw = sub.add_parser("power")
    pw.add_argument("--p10", type=float, required=True, help="P(task passes under A and fails under B)")
    pw.add_argument("--p01", type=float, required=True, help="P(task fails under A and passes under B)")
    pw.add_argument("--alpha", type=float, default=0.05)
    pw.add_argument("--power", type=float, default=0.8)
    args = parser.parse_args()
    if args.cmd == "compare":
        arms = []
        for spec in args.arm:
            name, _, run = spec.partition("=")
            if not run:
                sys.exit(f"--arm takes NAME=RUN, not {spec!r}")
            path = pathlib.Path(run).expanduser()
            arms.append((name, path if path.is_dir() else RUNS / run))
        report = compare(arms, args.split)
        if args.json:
            print(json.dumps(report, indent=1))
        else:
            print_report(report)
    else:
        n = tasks_for_power(args.p10, args.p01, args.alpha, args.power)
        calculator = McNemarPower(args.p10, args.p01, args.alpha)
        around = {m: round(calculator.power(m), 3) for m in range(max(1, (n or 1) - 2), (n or 0) + 4)} if n else {}
        print(json.dumps({"p10": args.p10, "p01": args.p01, "alpha": args.alpha, "target_power": args.power,
                          "paired_tasks": n, "power_around": around,
                          "note": "exact two-sided McNemar, one trial per task; None means beyond 2,000"}))


if __name__ == "__main__":
    main()
