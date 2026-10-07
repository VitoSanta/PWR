#!/usr/bin/env python3
"""Check public Markdown links and accidental personal material, without network.

Use Git's tracked inventory plus untracked, non-ignored files: this checks a
proposed document before it is committed, while ignoring local build output.
Deleted files are absent from the working tree and must not remain link targets.
Historical evidence may be unpublished; only local Markdown links are resolved.
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit

ROOT = pathlib.Path(__file__).resolve().parent.parent
LINK = re.compile(r"!?\[[^\]\n]*\]\((<[^>]+>|[^\s)]+)(?:\s+\"[^\"]*\")?\)")
PERSONAL_PATH = re.compile(
    r"/Users/(?!(?:runner|you|me|someone|x|example|\.\.\.)(?:/|$))[A-Za-z0-9._-]+/"
)
PERSONAL_PATTERNS = (
    (PERSONAL_PATH, "personal macOS home path; use an example or evidence ID"),
    (re.compile(r"https?://(?:www\.)?linkedin\.com/in/", re.I), "personal profile URL; use an example in fixtures"),
    (re.compile(r"LinkedIn\s+(?:outline|scaletta)|scaletta\s+LinkedIn", re.I), "personal editorial plan belongs outside the checkout"),
)
TEXT_SUFFIXES = {".md", ".json", ".jsonl", ".toml", ".yml", ".yaml", ".py", ".rs", ".ts", ".sh"}
PRIVATE_DOCUMENTS = (
    "docs/plan/mission-status.md",
    "docs/reviews/2026-10-01-stato-del-prodotto.md",
)


def prose(text: str) -> str:
    return re.sub(r"(?ms)^(`{3,}|~{3,})[^\n]*\n.*?^\1\s*$", "", text)


def anchors(text: str) -> set[str]:
    """GitHub-style heading fragments, plus explicit HTML IDs."""
    text = prose(text)
    found = set(re.findall(r"\bid=[\"']([^\"']+)[\"']", text))
    seen: dict[str, int] = {}
    for heading in re.findall(r"^#{1,6} +(.+?) *#*$", text, re.M):
        heading = re.sub(r"<[^>]+>", "", heading).lower()
        slug = re.sub(r"[^\w\- ]", "", heading).replace(" ", "-")
        count = seen.get(slug, 0)
        seen[slug] = count + 1
        found.add(f"{slug}-{count}" if count else slug)
    return found


def main() -> int:
    result = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT, capture_output=True, check=True,
    )
    names = sorted(set(result.stdout.decode().split("\0")) - {""})
    errors: list[str] = []
    documents = 0
    checked_links = 0
    for name in names:
        path = ROOT / name
        if not path.is_file():
            continue
        if name in PRIVATE_DOCUMENTS or re.fullmatch(
            r"docs/pwr-(?:documentazione-tecnica|modifiche)-.*\.it\.md", name
        ):
            errors.append(f"{name}: private working document belongs outside the checkout")
        if path.suffix not in TEXT_SUFFIXES:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for pattern, message in PERSONAL_PATTERNS:
            match = pattern.search(text)
            if match:
                line = text.count("\n", 0, match.start()) + 1
                # Do not echo a potentially private string into a CI log.
                errors.append(f"{name}:{line}: {message}")
        if path.suffix != ".md":
            continue
        documents += 1
        # Links in fenced examples are illustrative, not document navigation.
        for match in LINK.finditer(prose(text)):
            target = match.group(1).strip("<>")
            parsed = urlsplit(target)
            if parsed.scheme or parsed.netloc:
                continue
            checked_links += 1
            relative = unquote(parsed.path)
            if relative.startswith("/"):
                errors.append(f"{name}: absolute local link; use a repository-relative link")
                continue
            resolved = (path.parent / relative).resolve() if relative else path.resolve()
            if not resolved.is_relative_to(ROOT) or not resolved.exists():
                errors.append(f"{name}: missing/out-of-repository link target: {relative}")
            elif parsed.fragment and resolved.suffix == ".md":
                if unquote(parsed.fragment) not in anchors(resolved.read_text(encoding="utf-8")):
                    errors.append(f"{name}: missing Markdown fragment: {target}")
    if errors:
        print("Public documentation checks failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"Public documentation: {documents} Markdown files, {checked_links} local links; no flagged personal material.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
