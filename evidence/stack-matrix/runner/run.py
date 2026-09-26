#!/usr/bin/env python3
"""The stack matrix: PWR on real tasks across languages, frameworks and tools.

    run.py list [--split dev|heldout]
    run.py reference [TASK ...]          seed fails and reference passes, in containers
    run.py run [TASK ...] --run RUN_ID [--split dev|heldout] [--repeat N]
    run.py verify TASK WORKSPACE         the independent verdict on one workspace

PWR is driven the way the desktop app drives it -- `pwr serve --stdio`, one
process per task, goal mode, the permission questions answered by a stand-in
person who grants what the task's `allow` names and refuses the rest. The
verdict never comes from PWR: a clean copy of the workspace, with the task's
hidden tests laid over it, is run in the task's container image.
"""

import argparse
import difflib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

HERE = pathlib.Path(__file__).resolve().parent
SUITE = HERE.parent
REPO = SUITE.parent.parent
TASKS = SUITE / "tasks"
RESULTS = pathlib.Path(os.environ.get("PWR_EVIDENCE_RESULTS", pathlib.Path.home() / "Desktop/pwr-evidence/runs"))
PWR_BIN = os.environ.get("PWR_BIN", str(REPO / "target/release/pwr"))
MLX_PYTHON = os.environ.get(
    "PWR_MLX_PYTHON",
    str(pathlib.Path.home() / "Library/Application Support/ai.pwr.desktop/engine/venv/bin/python"),
)
MODEL = os.environ.get("PWR_EVIDENCE_MODEL", "lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit")

sys.path.insert(0, str(HERE))
from acp import Core, initialize  # noqa: E402

# Never part of what is verified or diffed: build output, installed
# toolchains, caches and PWR's own state.
NOT_SOURCE = {
    ".git", ".pwr", ".pwr-scratch", ".toolchains", "node_modules", "bin", "obj", "target",
    "build", "dist", "__pycache__", ".venv", "venv", ".gradle", ".mvn-cache", ".pytest_cache",
    ".next", ".angular", ".dart_tool", "vendor", "_build", "deps", ".build", ".swiftpm",
}

NUDGE = "It is not finished yet: keep going until the acceptance checks pass."


def say(*parts):
    print(time.strftime("%H:%M:%S"), *parts, flush=True)


# ---------------------------------------------------------------- tasks


def load_tasks(names=None, split=None):
    tasks = []
    for path in sorted(TASKS.glob("*/task.json")):
        task = json.loads(path.read_text())
        task["dir"] = path.parent
        if names and task["id"] not in names:
            continue
        if split and task.get("split") != split:
            continue
        tasks.append(task)
    if names:
        missing = set(names) - {task["id"] for task in tasks}
        if missing:
            sys.exit(f"no such task: {', '.join(sorted(missing))}")
    return tasks


def copy_source(source, target, exclude=NOT_SOURCE):
    """Copies a tree without build output, caches and installed toolchains."""
    source = pathlib.Path(source)
    for path in source.rglob("*"):
        relative = path.relative_to(source)
        if any(part in exclude for part in relative.parts):
            continue
        destination = pathlib.Path(target) / relative
        if path.is_symlink():
            continue
        if path.is_dir():
            destination.mkdir(parents=True, exist_ok=True)
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)


def overlay(source, target):
    if pathlib.Path(source).is_dir():
        shutil.copytree(source, target, dirs_exist_ok=True)


# ---------------------------------------------------------------- verdict


def verify(task, workspace, label="verify"):
    """(passed, output): the task's own acceptance, run outside PWR."""
    spec = task["verify"]
    exclude = NOT_SOURCE - set(spec.get("keep", []))
    with tempfile.TemporaryDirectory(prefix=f"sm-{task['id']}-") as scratch:
        copy = pathlib.Path(scratch) / "w"
        copy.mkdir()
        copy_source(workspace, copy, exclude)
        # The owner's tests as the owner wrote them: a change to them is not a
        # way to pass.
        for relative in spec.get("restore", []):
            original = task["dir"] / "workspace" / relative
            if original.is_dir():
                shutil.copytree(original, copy / relative, dirs_exist_ok=True)
            elif original.is_file():
                (copy / relative).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(original, copy / relative)
        overlay(task["dir"] / "hidden", copy)
        timeout = spec.get("timeout", 1200)
        if "host" in spec:
            command = ["bash", "-c", spec["host"]]
            name = None
        else:
            name = f"sm-{task['id']}-{label}-{os.getpid()}-{int(time.time())}"
            command = ["docker", "run", "--rm", "--name", name, "-v", f"{copy}:/w", "-w", "/w"]
            for cache in spec.get("cache", []):
                volume, path = cache.split(":", 1)
                command += ["-v", f"pwr-sm-{volume}:{path}"]
            for key, value in spec.get("env", {}).items():
                command += ["-e", f"{key}={value}"]
            # An image whose entrypoint is its tool (terraform) takes the
            # shell as its entrypoint instead.
            if spec.get("entrypoint"):
                command += ["--entrypoint", spec["entrypoint"]]
                command += [spec["image"], "-c", spec["command"]]
            else:
                command += [spec["image"], "sh", "-c", spec["command"]]
        try:
            # One stream, in order: a verdict line printed after a build log
            # must end the output, not sit before the log.
            done = subprocess.run(command, cwd=copy, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                  text=True, timeout=timeout)
            output = done.stdout
            passed = done.returncode == 0
        except subprocess.TimeoutExpired as error:
            if name:
                subprocess.run(["docker", "kill", name], capture_output=True)
            output = f"{error.stdout or ''}{error.stderr or ''}\nTIMEOUT after {timeout}s"
            passed = False
        if passed and spec.get("expect"):
            # A run that exits 0 having run nothing is not a pass.
            passed = re.search(spec["expect"], output, re.M) is not None
        return passed, output


def cmd_reference(args):
    failures = 0
    for task in load_tasks(args.tasks or None):
        seed_passed, seed_output = verify(task, task["dir"] / "workspace", "seed")
        with tempfile.TemporaryDirectory() as scratch:
            solved = pathlib.Path(scratch) / "w"
            shutil.copytree(task["dir"] / "workspace", solved)
            overlay(task["dir"] / "reference", solved)
            ref_passed, ref_output = verify(task, solved, "reference")
        sound = not seed_passed and ref_passed
        failures += not sound
        say(f"{'OK  ' if sound else 'BAD '} {task['id']:<28} seed={'pass' if seed_passed else 'fail'} reference={'pass' if ref_passed else 'fail'}")
        if not sound:
            print((ref_output if not ref_passed else seed_output)[-3000:], flush=True)
    sys.exit(1 if failures else 0)


# ---------------------------------------------------------------- the person


class Person:
    """Answers PWR's questions the way the task says a person would, and
    watches what happens so the result can say it."""

    def __init__(self, core, allow):
        self.core = core
        self.allow = set(allow)
        self.questions = []
        self.actions = 0
        self.failed_actions = 0
        self.prompt_tokens = 0
        self.generated_tokens = 0
        self.prefill_ms = 0
        self.generation_ms = 0
        self.peak_context = 0
        self.answer = ""

    def __call__(self, message):
        method = message.get("method")
        params = message.get("params") or {}
        if method == "session/request_permission":
            meta = (params.get("_meta") or {}).get("pwr", {})
            kind = meta.get("approval", "")
            title = params.get("toolCall", {}).get("title", "")
            granted = kind in self.allow
            choice = "allow_always" if granted else "reject_once"
            self.questions.append({"approval": kind, "question": title, "answer": choice})
            say(f"  ? {kind}: {title[:140]} -> {choice}")
            self.core.answer(message["id"], {"outcome": {"outcome": "selected", "optionId": choice}})
        elif method == "_pwr/usage":
            self.peak_context = max(self.peak_context, int(params.get("used") or 0))
        elif method == "_pwr/turn_event" and params.get("event") == "generation":
            self.prompt_tokens += int(params.get("promptTokens") or 0)
            self.generated_tokens += int(params.get("generatedTokens") or 0)
            self.prefill_ms += int(params.get("promptEvalMs") or 0)
            self.generation_ms += int(params.get("generationMs") or 0)
        elif method == "session/update":
            update = params.get("update", {})
            kind = update.get("sessionUpdate")
            if kind == "tool_call":
                self.actions += 1
                say(f"  [{self.actions}] {update.get('title', '')[:150]}")
            elif kind == "tool_call_update" and update.get("status") == "failed":
                self.failed_actions += 1
                detail = ""
                for content in update.get("content") or []:
                    if content.get("type") == "content":
                        detail = content.get("content", {}).get("text", "")
                say(f"      failed: {detail[:200]}".replace("\n", " "))
            elif kind == "agent_message_chunk":
                live = (update.get("_meta") or {}).get("pwr", {}).get("live")
                text = update.get("content", {}).get("text", "")
                if not live and text.strip():
                    self.answer = text


# ---------------------------------------------------------------- one task


def diff_against_seed(seed, workspace, exclude=NOT_SOURCE):
    seed, workspace = pathlib.Path(seed), pathlib.Path(workspace)

    def files(root):
        found = {}
        for path in root.rglob("*"):
            relative = path.relative_to(root)
            if any(part in exclude for part in relative.parts) or not path.is_file():
                continue
            found[str(relative)] = path
        return found

    def text(path):
        data = path.read_bytes()
        if b"\0" in data[:8192] or len(data) > 512 * 1024:
            return None, len(data)
        try:
            return data.decode("utf-8").splitlines(True), len(data)
        except UnicodeDecodeError:
            return None, len(data)

    before, after = files(seed), files(workspace)
    chunks = []
    for name in sorted(set(before) | set(after)):
        old, old_size = text(before[name]) if name in before else ([], 0)
        new, new_size = text(after[name]) if name in after else ([], 0)
        if old is None or new is None:
            if before.get(name) is None or after.get(name) is None or before[name].read_bytes() != after[name].read_bytes():
                chunks.append(f"Binary or large file {name}: {old_size} -> {new_size} bytes\n")
        elif old != new:
            chunks.extend(difflib.unified_diff(old, new, f"a/{name}", f"b/{name}"))
    return "".join(chunks)


def sidecar_path():
    """The sidecar pinned beside the binary (`sidecar-<revision>/`), else the
    source tree's, which is what an unpinned binary reads."""
    if os.environ.get("PWR_MLX_SIDECAR"):
        return pathlib.Path(os.environ["PWR_MLX_SIDECAR"])
    name = os.path.basename(PWR_BIN)
    if name.startswith("pwr-"):
        pinned = pathlib.Path(PWR_BIN).parent / f"sidecar-{name[4:]}" / "pwr_mlx.py"
        if pinned.exists():
            return pinned
    return REPO / "crates/pwr-mlx/sidecar/pwr_mlx.py"


def sidecar_digest():
    import hashlib
    path = sidecar_path()
    return hashlib.sha256(path.read_bytes()).hexdigest()[:12] if path.exists() else None


def binary_revision():
    """The revision the binary under test was built from: a pinned copy is
    named for it (`pwr-<revision>`); otherwise the checkout's, marked so."""
    name = os.path.basename(PWR_BIN)
    if name.startswith("pwr-"):
        return name[4:]
    head = subprocess.run(["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    return f"{head} (unpinned build)"


def run_task(task, run_id, attempt, turns_override=None):
    label = task["id"] if attempt == 1 else f"{task['id']}~{attempt}"
    out = RESULTS / run_id / label
    if (out / "result.json").exists():
        say(f"{label}: already done")
        return json.loads((out / "result.json").read_text())
    if out.exists():
        shutil.rmtree(out)
    workspace = out / "workspace"
    shutil.copytree(task["dir"] / "workspace", workspace)
    git = dict(os.environ, GIT_AUTHOR_NAME="owner", GIT_AUTHOR_EMAIL="owner@example.com",
               GIT_COMMITTER_NAME="owner", GIT_COMMITTER_EMAIL="owner@example.com")
    subprocess.run(["git", "init", "-q", "-b", "main"], cwd=workspace, check=True)
    subprocess.run(["git", "add", "-A"], cwd=workspace, check=True)
    subprocess.run(["git", "commit", "-qm", "Initial"], cwd=workspace, env=git, check=True)

    brief = (task["dir"] / "brief.md").read_text()
    turns = turns_override or task.get("turns", 3)
    budget = task.get("minutes", 90) * 60
    result = {
        "task": task["id"], "title": task["title"], "stacks": task["stacks"],
        "category": task["category"], "split": task.get("split"), "attempt": attempt,
        "run": run_id, "model": MODEL, "binary": PWR_BIN,
        "revision": binary_revision(),
        # The MLX sidecar is read from the source tree, not from the binary:
        # what it was is recorded beside the binary's revision.
        "sidecar": sidecar_digest(),
        "started": time.strftime("%Y-%m-%dT%H:%M:%S"), "turns": [],
    }
    say(f"=== {label}: {task['title']}")
    started = time.time()
    core = Core(PWR_BIN, out / "transcript.jsonl",
                {"PWR_MLX_PYTHON": MLX_PYTHON, "PWR_MLX_SIDECAR": str(sidecar_path())})
    person = Person(core, task.get("allow", []))
    passed, output = False, ""
    watchdog = None
    try:
        initialize(core)
        core.request("_pwr/models", {"cwd": str(workspace), "model": MODEL, "acknowledgeProvisional": True},
                     on_message=person, timeout=900)
        core.request("_pwr/approvals", {"cwd": str(workspace), "mode": "ask"}, timeout=60)
        session = core.request("session/new", {"cwd": str(workspace), "mcpServers": []},
                               on_message=person, timeout=900)["sessionId"]
        stopped_for_time = threading.Event()

        def stop_when_over():
            while not stopped_for_time.wait(15):
                if time.time() - started > budget:
                    say(f"  budget of {budget // 60} min reached: cancelling")
                    core.notify("session/cancel", {"sessionId": session})
                    stopped_for_time.set()

        watchdog = threading.Thread(target=stop_when_over, daemon=True)
        watchdog.start()
        for turn in range(1, turns + 1):
            text = brief if turn == 1 else NUDGE
            turn_started = time.time()
            reply = core.request(
                "session/prompt",
                {"sessionId": session, "prompt": [{"type": "text", "text": text}], "goalMode": True},
                on_message=person,
            )
            meta = (reply.get("_meta") or {}).get("pwr", {})
            passed, output = verify(task, workspace, f"t{turn}")
            record = {
                "turn": turn, "prompt": text if turn > 1 else "(brief)",
                "stop": reply.get("stopReason"), "terminal": meta.get("terminal"),
                "goal": meta.get("goal"), "minutes": round((time.time() - turn_started) / 60, 1),
                "passed": passed, "verdict_tail": output[-2500:],
            }
            result["turns"].append(record)
            say(f"  turn {turn}: {'PASS' if passed else 'fail'} stop={record['stop']} terminal={record['terminal']} {record['minutes']} min")
            if passed or stopped_for_time.is_set():
                break
        stopped_for_time.set()
    except Exception as error:  # the run is evidence either way
        result["error"] = repr(error)
        say(f"  error: {error!r}")
    finally:
        core.close()
    result.update({
        "passed": passed,
        "minutes": round((time.time() - started) / 60, 1),
        "actions": person.actions, "failed_actions": person.failed_actions,
        "questions": person.questions, "final_answer": person.answer,
        "tokens": {"prompt": person.prompt_tokens, "generated": person.generated_tokens,
                   "prefill_ms": person.prefill_ms, "generation_ms": person.generation_ms,
                   "peak_context": person.peak_context},
    })
    kept = NOT_SOURCE - set(task["verify"].get("keep", []))
    (out / "diff.patch").write_text(diff_against_seed(task["dir"] / "workspace", workspace, kept))
    (out / "verdict.txt").write_text(output)
    (out / "result.json").write_text(json.dumps(result, indent=1))
    say(f"=== {label}: {'PASS' if passed else 'FAIL'} in {result['minutes']} min, {person.actions} actions")
    return result


def cmd_run(args):
    tasks = load_tasks(args.tasks or None, args.split)
    summary = []
    for attempt in range(1, args.repeat + 1):
        for task in tasks:
            result = run_task(task, args.run, attempt, args.turns)
            summary.append((result["task"], attempt, result["passed"], result["minutes"]))
    say("--- summary")
    for name, attempt, passed, minutes in summary:
        say(f"{'PASS' if passed else 'FAIL'}  {name}  #{attempt}  {minutes} min")


def cmd_verify(args):
    task = load_tasks([args.task])[0]
    passed, output = verify(task, args.workspace)
    print(output[-4000:])
    say("PASS" if passed else "FAIL")
    sys.exit(0 if passed else 1)


def cmd_list(args):
    for task in load_tasks(split=args.split):
        print(f"{task['id']:<28} {task.get('split', '-'):<8} {','.join(task['stacks']):<32} {task['title']}")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)
    listing = sub.add_parser("list")
    listing.add_argument("--split")
    reference = sub.add_parser("reference")
    reference.add_argument("tasks", nargs="*")
    run = sub.add_parser("run")
    run.add_argument("tasks", nargs="*")
    run.add_argument("--run", required=True)
    run.add_argument("--split")
    run.add_argument("--repeat", type=int, default=1)
    run.add_argument("--turns", type=int)
    check = sub.add_parser("verify")
    check.add_argument("task")
    check.add_argument("workspace")
    args = parser.parse_args()
    {"list": cmd_list, "reference": cmd_reference, "run": cmd_run, "verify": cmd_verify}[args.cmd](args)


if __name__ == "__main__":
    main()
