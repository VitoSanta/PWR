#!/usr/bin/env python3
"""Watch a stack-matrix run as the model writes it, token by token.

    python3 runner/watch.py            # the newest run under the results folder
    python3 runner/watch.py RUN_DIR    # one run's folder (…/<run>/<task>)
    python3 runner/watch.py --all      # from the start of the run, not from now

It follows `transcript.jsonl`, the protocol record the runner keeps of the
conversation with `pwr serve`, and prints what the app would show: the model's
reasoning (dim), its answer, each action as it starts and how it ended, the
prefill progress while it reads a long prompt, and the harness's own notes.
When a newer task starts it moves to it. Ctrl-C leaves; nothing is written.
"""
import json
import os
import pathlib
import sys
import time

RESULTS = pathlib.Path(os.environ.get("PWR_EVIDENCE_RESULTS", pathlib.Path.home() / "Desktop/pwr-evidence/runs"))
DIM, RESET, CYAN, RED, YELLOW, GREEN, BOLD = "\033[2m", "\033[0m", "\033[36m", "\033[31m", "\033[33m", "\033[32m", "\033[1m"


def newest():
    best = None
    for path in RESULTS.glob("*/*/transcript.jsonl"):
        try:
            mtime = path.stat().st_mtime
        except OSError:
            continue
        if best is None or mtime > best[0]:
            best = (mtime, path)
    return best[1] if best else None


class View:
    def __init__(self):
        self.mode = None  # which stream the cursor is in: thought, text, or None
        self.calls = {}

    def switch(self, mode):
        if mode != self.mode:
            if self.mode is not None:
                sys.stdout.write(RESET + "\n")
            self.mode = mode
            if mode == "thought":
                sys.stdout.write(DIM + "· thinking · ")
            elif mode == "text":
                sys.stdout.write(BOLD + "▍ " + RESET)

    def line(self, text):
        if self.mode is not None:
            sys.stdout.write(RESET + "\n")
            self.mode = None
        sys.stdout.write(text + RESET + "\n")
        sys.stdout.flush()

    def handle(self, message):
        method = message.get("method")
        params = message.get("params") or {}
        if method == "session/update":
            update = params.get("update") or {}
            kind = update.get("sessionUpdate")
            if kind == "agent_thought_chunk":
                self.switch("thought")
                sys.stdout.write(DIM + (update.get("content") or {}).get("text", ""))
            elif kind == "agent_message_chunk":
                text = (update.get("content") or {}).get("text", "")
                live = ((update.get("_meta") or {}).get("pwr") or {}).get("live")
                if live:
                    self.switch("text")
                    sys.stdout.write(text)
            elif kind == "tool_call":
                title = update.get("title") or update.get("kind") or "action"
                detail = ((update.get("_meta") or {}).get("pwr") or {}).get("detail", "")
                self.calls[update.get("toolCallId")] = title
                self.line(f"{CYAN}⚙ {title} {detail}"[:240])
            elif kind == "tool_call_update":
                status = update.get("status")
                if status in ("failed", "completed"):
                    body = ""
                    for item in update.get("content") or []:
                        body = ((item.get("content") or {}).get("text") or "").replace("\n", " ")[:200]
                    colour = RED if status == "failed" else GREEN
                    self.line(f"{colour}  {'✗' if status == 'failed' else '✓'} {body}")
            sys.stdout.flush()
        elif method == "_pwr/model_progress" and params.get("prefill"):
            p = params["prefill"]
            self.line(f"{YELLOW}… reading the conversation: {p['processed']:,} of {p['total']:,} tokens")
        elif method == "_pwr/turn_event":
            text = params.get("text") or params.get("detail") or params.get("event")
            if text:
                self.line(f"{YELLOW}• {str(text)[:240]}")
        elif method == "_pwr/compacted":
            self.line(f"{YELLOW}• conversation compacted")


def follow(path, view, skip=True):
    print(f"{BOLD}{path.parent.parent.name} / {path.parent.name}{RESET}\n", flush=True)
    with open(path, errors="replace") as handle:
        if skip:
            # From now: the last stretch of the record, not the whole run.
            handle.seek(0, os.SEEK_END)
            handle.seek(max(0, handle.tell() - 60_000))
            handle.readline()
        while True:
            line = handle.readline()
            if line:
                try:
                    record = json.loads(line)
                except ValueError:
                    continue
                if "in" in record:
                    view.handle(record["in"])
                elif "out" in record and record["out"].get("method") == "session/prompt":
                    prompt = record["out"]["params"].get("prompt") or [{}]
                    view.line(f"{BOLD}▶ {str(prompt[0].get('text', ''))[:300]}")
                continue
            time.sleep(0.15)
            latest = newest()
            if latest and latest != path and latest.stat().st_mtime > path.stat().st_mtime + 1:
                return latest


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    everything = "--all" in sys.argv
    path = pathlib.Path(args[0]) / "transcript.jsonl" if args else newest()
    while path is None:
        time.sleep(2)
        path = newest()
    view = View()
    try:
        first = not everything
        while path is not None:
            path = follow(path, view, skip=first)
            first = False
            view.line("")
    except KeyboardInterrupt:
        sys.stdout.write(RESET + "\n")


if __name__ == "__main__":
    main()
