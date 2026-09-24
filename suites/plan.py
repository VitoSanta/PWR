"""Prints, for a suite file, one line per corpus: the corpus and its tasks."""
import json
import sys

groups = {}
for case in json.load(open(sys.argv[1]))["cases"]:
    if case["type"] == "task":
        groups.setdefault(case["corpus"], []).append(case["task"])
for corpus, tasks in groups.items():
    print(corpus, *tasks)
