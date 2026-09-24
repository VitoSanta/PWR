"""Shared by the suite corpus builders (build_a3_corpus.py, build_a4_corpus.py).

A task is written only if it proves its own worth in a scratch directory: the
visible verifier fails as given, the reference fix passes both verifiers, and
the recorded wrong fix -- the failure the task exists to catch -- fails the
hidden one. The reference and wrong fixes never enter the corpus.
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile

VISIBLE = {"executable": "python3", "args": ["-m", "unittest", "-q"]}
HIDDEN = {"executable": "python3", "args": ["hidden_check.py"]}


def task(id, statement, allowed, files, hidden, fix, wrong, origin, suite):
    return {
        "id": id,
        "kind": "bugfix",
        "statement": statement,
        "allowed_files": allowed,
        "files": files,
        "hidden_files": {"hidden_check.py": hidden},
        "visible_verifier": VISIBLE,
        "hidden_verifier": HIDDEN,
        "time_budget_secs": 600,
        "provenance": f"authored 2026-09-19 for PWR suite {suite}; modelled on {origin}; "
        "not derived from any public benchmark",
        # Not part of the corpus: stripped before writing.
        "_fix": fix,
        "_wrong": wrong,
    }


def run(verifier, cwd):
    result = subprocess.run(
        [verifier["executable"], *verifier["args"]], cwd=cwd, capture_output=True, text=True
    )
    return result.returncode == 0


def materialise(directory, files):
    for name, content in files.items():
        path = pathlib.Path(directory, name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content.encode())


def check(entry):
    problems = []
    with tempfile.TemporaryDirectory() as plain:
        materialise(plain, entry["files"])
        if run(VISIBLE, plain):
            problems.append("the visible verifier passes before any fix")
    for label, change, want_hidden in (
        ("reference fix", entry["_fix"], True),
        ("wrong fix", entry["_wrong"], False),
    ):
        with tempfile.TemporaryDirectory() as scratch:
            materialise(scratch, entry["files"])
            materialise(scratch, change)
            materialise(scratch, entry["hidden_files"])
            if label == "reference fix" and not run(VISIBLE, scratch):
                problems.append("the reference fix fails the visible verifier")
            if run(HIDDEN, scratch) != want_hidden:
                problems.append(
                    f"the {label} {'fails' if want_hidden else 'passes'} the hidden verifier"
                )
    return problems


def build(name, tasks, out):
    failed = False
    for entry in tasks:
        problems = check(entry)
        print(f"{entry['id']}: {'ok' if not problems else '; '.join(problems)}")
        failed |= bool(problems)
    if failed:
        sys.exit(1)
    corpus = {
        "name": name,
        "tasks": [{k: v for k, v in entry.items() if not k.startswith("_")} for entry in tasks],
    }
    pathlib.Path(out).write_text(json.dumps(corpus, indent=1, ensure_ascii=False) + "\n")
    print(f"wrote {out}")
