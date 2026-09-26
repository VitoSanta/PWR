#!/usr/bin/env python3
"""Renders stack-matrix runs as a static site: one page per task and run,
with the conversation as it happened, and an index with the matrix.

    site.py OUT_DIR RUN_ID [RUN_ID ...]

Everything a page shows comes from the run's own files: the protocol
transcript, the result and the diff. Paths on the machine that ran it are
replaced -- the workspace by `<workspace>`, the home directory by `~`.
"""

import html
import json
import os
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from run import RESULTS  # noqa: E402

HOME = str(pathlib.Path.home())

STYLE = """
:root { --bg: #fbfbfa; --fg: #1d1d1b; --muted: #6b6b66; --line: #e4e3df; --card: #ffffff;
  --user: #eef3ff; --ok: #1f7a4a; --bad: #b3261e; --warn: #8a5a00; --code: #f4f4f1; --accent: #3056d3; }
@media (prefers-color-scheme: dark) { :root { --bg: #161615; --fg: #ecebe6; --muted: #9a9993; --line: #2c2c2a;
  --card: #1e1e1c; --user: #1d2640; --ok: #5fcf92; --bad: #ff8a80; --warn: #e2b04a; --code: #232321; --accent: #8aa6ff; } }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--fg); font: 15px/1.55 ui-sans-serif, system-ui, -apple-system, sans-serif; }
main { max-width: 920px; margin: 0 auto; padding: 32px 16px 80px; }
h1 { font-size: 22px; margin: 0 0 4px; } h2 { font-size: 17px; margin: 32px 0 8px; }
a { color: var(--accent); }
.muted { color: var(--muted); } .ok { color: var(--ok); } .bad { color: var(--bad); } .warn { color: var(--warn); }
.meta { display: flex; flex-wrap: wrap; gap: 6px 18px; margin: 12px 0 24px; font-size: 13px; color: var(--muted); }
.meta b { color: var(--fg); font-weight: 600; }
.msg { border: 1px solid var(--line); border-radius: 10px; padding: 12px 14px; margin: 10px 0; background: var(--card); }
.msg.user { background: var(--user); }
.msg .who { font-size: 12px; font-weight: 600; letter-spacing: .02em; text-transform: uppercase; color: var(--muted); margin-bottom: 4px; }
.msg pre, pre.code { white-space: pre-wrap; word-break: break-word; font: 12.5px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace;
  background: var(--code); border-radius: 6px; padding: 8px 10px; margin: 6px 0 0; max-height: 420px; overflow: auto; }
.text { white-space: pre-wrap; word-break: break-word; }
.tool { display: grid; grid-template-columns: 22px 1fr; gap: 6px; padding: 4px 2px; font-size: 13.5px; }
.tool .mark { text-align: center; }
.tool details summary { cursor: pointer; list-style: none; }
.tool details summary::-webkit-details-marker { display: none; }
.note { font-size: 13px; color: var(--muted); padding: 2px 2px 2px 28px; }
.ask { border-left: 3px solid var(--warn); padding: 6px 10px; margin: 8px 0; font-size: 13.5px; background: var(--card); border-radius: 0 6px 6px 0; }
.verdict { border-radius: 10px; padding: 10px 14px; margin: 14px 0; font-size: 14px; border: 1px solid var(--line); }
.verdict.pass { border-color: var(--ok); } .verdict.fail { border-color: var(--bad); }
table { border-collapse: collapse; width: 100%; font-size: 13.5px; }
th, td { text-align: left; padding: 7px 8px; border-bottom: 1px solid var(--line); vertical-align: top; }
th { font-weight: 600; color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: .03em; }
.pill { display: inline-block; padding: 1px 8px; border-radius: 999px; font-size: 12px; border: 1px solid var(--line); margin: 1px 2px 1px 0; }
.scroll { overflow-x: auto; }
"""


def scrub(text, workspace):
    if not isinstance(text, str):
        return text
    if workspace:
        text = text.replace(workspace, "<workspace>")
        real = os.path.realpath(workspace)
        text = text.replace(real, "<workspace>")
    text = text.replace(HOME, "~")
    return re.sub(r"/private/var/folders/[^\s'\"]+|/var/folders/[^\s'\"]+", "<tmp>", text)


def esc(text):
    return html.escape(text or "")


def events_of(transcript, workspace):
    """The conversation, in order, from the protocol transcript."""
    events = []
    tools = {}
    questions = {}
    prompts = {}
    for line in transcript.read_text().splitlines():
        record = json.loads(line)
        if "out" in record:
            message = record["out"]
            if message.get("method") == "session/prompt":
                text = " ".join(b.get("text", "") for b in message["params"]["prompt"] if b.get("type") == "text")
                prompts[message["id"]] = True
                events.append({"type": "user", "text": scrub(text, workspace)})
            elif "result" in message and message.get("id") in questions:
                questions[message["id"]]["answer"] = message["result"]["outcome"].get("optionId")
            continue
        message = record.get("in") or {}
        method = message.get("method")
        params = message.get("params") or {}
        if method == "session/request_permission":
            meta = (params.get("_meta") or {}).get("pwr", {})
            question = {"type": "ask", "approval": meta.get("approval", ""),
                        "text": scrub(params.get("toolCall", {}).get("title", ""), workspace), "answer": None}
            questions[message.get("id")] = question
            events.append(question)
        elif method == "session/update":
            update = params.get("update", {})
            kind = update.get("sessionUpdate")
            if kind == "agent_message_chunk":
                live = (update.get("_meta") or {}).get("pwr", {}).get("live")
                text = update.get("content", {}).get("text", "")
                if not live and text.strip():
                    events.append({"type": "assistant", "text": scrub(text, workspace)})
            elif kind == "tool_call":
                detail = ((update.get("_meta") or {}).get("pwr") or {}).get("detail") or ""
                title = update.get("title", "")
                if detail and detail not in title:
                    title = f"{title}: {detail}" if title else detail
                tool = {"type": "tool", "title": scrub(title, workspace),
                        "kind": update.get("kind", ""), "status": update.get("status", "pending"), "output": ""}
                tools[update.get("toolCallId")] = tool
                events.append(tool)
            elif kind == "tool_call_update":
                tool = tools.get(update.get("toolCallId"))
                if tool is None:
                    continue
                if update.get("status"):
                    tool["status"] = update["status"]
                if update.get("title") and update["title"] not in tool["title"]:
                    tool["title"] = scrub(update["title"], workspace)
                for content in update.get("content") or []:
                    if content.get("type") == "content":
                        tool["output"] = scrub(content.get("content", {}).get("text", ""), workspace)
                    elif content.get("type") == "diff":
                        tool["output"] = scrub(f"--- {content.get('path', '')}\n{content.get('newText', '')}"[:4000], workspace)
        elif method == "_pwr/turn_event" and params.get("event") == "note":
            events.append({"type": "note", "text": scrub(params.get("text", ""), workspace)})
        elif message.get("id") in prompts and "result" in message:
            result = message["result"]
            events.append({"type": "turn_end", "stop": result.get("stopReason"),
                           "meta": (result.get("_meta") or {}).get("pwr", {})})
    return events


def render_task(run_dir, result):
    workspace = str(run_dir / "workspace")
    events = events_of(run_dir / "transcript.jsonl", workspace)
    turns = iter(result.get("turns", []))
    parts = []
    for event in events:
        kind = event["type"]
        if kind == "user":
            parts.append(f'<div class="msg user"><div class="who">Person</div><div class="text">{esc(event["text"])}</div></div>')
        elif kind == "assistant":
            parts.append(f'<div class="msg"><div class="who">PWR</div><div class="text">{esc(event["text"])}</div></div>')
        elif kind == "tool":
            mark = {"completed": '<span class="ok">✓</span>', "failed": '<span class="bad">✗</span>'}.get(event["status"], '<span class="muted">•</span>')
            body = esc(event["title"] or event["kind"])
            if event["output"]:
                body = f'<details><summary>{body}</summary><pre>{esc(event["output"][:6000])}</pre></details>'
            parts.append(f'<div class="tool"><div class="mark">{mark}</div><div>{body}</div></div>')
        elif kind == "ask":
            answer = {"allow_always": "allowed for the session", "allow_once": "allowed once", "reject_once": "refused"}.get(event["answer"], event["answer"] or "no answer")
            parts.append(f'<div class="ask"><b>PWR asked</b> ({esc(event["approval"])}): {esc(event["text"])}<br><span class="muted">The person {esc(answer)}.</span></div>')
        elif kind == "note":
            parts.append(f'<div class="note">{esc(event["text"])}</div>')
        elif kind == "turn_end":
            goal = event["meta"].get("goal") or {}
            said = f'PWR ended the turn: {esc(event["stop"] or "")}'
            if event["meta"].get("terminal"):
                said += f' ({esc(event["meta"]["terminal"])})'
            if goal.get("reason"):
                said += f'<br><span class="muted">{esc(goal["reason"][:600])}</span>'
            parts.append(f'<div class="note">{said}</div>')
            turn = next(turns, None)
            if turn is not None:
                verdict = "pass" if turn["passed"] else "fail"
                label = "Independent verification passed" if turn["passed"] else "Independent verification failed"
                parts.append(
                    f'<div class="verdict {verdict}"><b class="{"ok" if turn["passed"] else "bad"}">{label}</b>'
                    f' <span class="muted">after turn {turn["turn"]} · {turn["minutes"]} min</span>'
                    f'<details><summary class="muted">verifier output</summary><pre class="code">{esc(scrub(turn.get("verdict_tail", ""), workspace)[-3000:])}</pre></details></div>'
                )
    tokens = result.get("tokens", {})
    diff = (run_dir / "diff.patch").read_text() if (run_dir / "diff.patch").exists() else ""
    stacks = "".join(f'<span class="pill">{esc(s)}</span>' for s in result.get("stacks", []))
    status = '<b class="ok">PASS</b>' if result.get("passed") else '<b class="bad">FAIL</b>'
    return f"""<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(result["task"])} · PWR stack matrix</title><style>{STYLE}</style></head><body><main>
<p class="muted"><a href="index.html">← all tasks</a></p>
<h1>{esc(result["title"])}</h1>
<div>{stacks}</div>
<div class="meta"><span>Result {status}</span><span><b>{result.get("minutes")}</b> min</span>
<span><b>{result.get("actions")}</b> actions ({result.get("failed_actions")} failed)</span>
<span><b>{len(result.get("questions", []))}</b> questions to the person</span>
<span>peak context <b>{tokens.get("peak_context", 0):,}</b> tokens</span>
<span><b>{tokens.get("generated", 0):,}</b> tokens generated</span>
<span>model <b>{esc(result.get("model", ""))}</b></span><span>PWR <b>{esc(result.get("revision", ""))}</b></span>
<span>split <b>{esc(result.get("split") or "")}</b></span></div>
<h2>The conversation</h2>
{"".join(parts)}
<h2>What changed</h2>
<pre class="code">{esc(scrub(diff, workspace)[:60000]) or "(no change)"}</pre>
</main></body></html>"""


def render_index(rows, runs):
    passed = sum(1 for r in rows if r["passed"])
    body = []
    for r in sorted(rows, key=lambda r: (r["category"], r["task"], r["run"])):
        stacks = "".join(f'<span class="pill">{esc(s)}</span>' for s in r["stacks"])
        status = '<b class="ok">PASS</b>' if r["passed"] else '<b class="bad">FAIL</b>'
        body.append(
            f'<tr><td><a href="{esc(r["page"])}">{esc(r["title"])}</a><div>{stacks}</div></td>'
            f'<td>{esc(r["category"])}</td><td>{status}</td><td>{r["minutes"]}</td><td>{r["actions"]}</td>'
            f'<td>{len(r["questions"])}</td><td>{r["tokens"].get("peak_context", 0):,}</td><td>{esc(r["run"])}</td></tr>'
        )
    return f"""<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>PWR stack matrix</title><style>{STYLE}</style></head><body><main>
<h1>PWR stack matrix</h1>
<p class="muted">A local model driven by PWR on real tasks across languages, frameworks, databases and tools.
Every verdict comes from an independent run of the task's tests -- the owner's, restored as written, plus hidden ones --
in the task's official container image, never from PWR's own report. Runs: {esc(", ".join(runs))}.</p>
<div class="meta"><span><b>{passed}</b> of <b>{len(rows)}</b> passed</span></div>
<div class="scroll"><table><thead><tr><th>Task</th><th>Kind</th><th>Result</th><th>Min</th><th>Actions</th><th>Asked</th><th>Peak ctx</th><th>Run</th></tr></thead>
<tbody>{"".join(body)}</tbody></table></div>
</main></body></html>"""


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    runs = sys.argv[2:]
    rows = []
    for run in runs:
        for result_path in sorted((RESULTS / run).glob("*/result.json")):
            result = json.loads(result_path.read_text())
            page = f"{run}--{result_path.parent.name}.html"
            (out / page).write_text(render_task(result_path.parent, result))
            rows.append({**result, "run": run, "page": page})
    (out / "index.html").write_text(render_index(rows, runs))
    print(f"{len(rows)} pages in {out}")


if __name__ == "__main__":
    main()
