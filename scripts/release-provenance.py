#!/usr/bin/env python3
"""Fail closed on dirty release sources; record local, unsigned build provenance."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def identity(root: Path, expected: str | None = None) -> dict:
    # Include untracked files and submodule dirt; ignored build outputs are excluded.
    if git(root, 'status', '--porcelain=v1', '--untracked-files=all', '--ignore-submodules=none'):
        raise ValueError('release requires a clean source tree, including untracked files and submodules')
    commit = git(root, 'rev-parse', '--verify', 'HEAD^{commit}')
    if expected and git(root, 'rev-parse', '--verify', expected + '^{commit}') != commit:
        raise ValueError('expected release revision does not match HEAD')
    # Submodules need separate content provenance; this repository uses none.
    if git(root, 'ls-files', '--stage').splitlines() and any(
        line.startswith('160000 ') for line in git(root, 'ls-files', '--stage').splitlines()
    ):
        raise ValueError('submodule release provenance is not implemented')
    return {'commit': commit, 'tree': git(root, 'rev-parse', 'HEAD^{tree}'), 'clean': True}


def version(command: list[str]) -> str:
    return subprocess.check_output(command, text=True, stderr=subprocess.STDOUT).strip()


def start(root: Path, expected: str | None) -> dict:
    return {'schema': 1, 'kind': 'local-build-record', 'source': identity(root, expected),
            'tools': {name: version(cmd) for name, cmd in {
                'rustc': ['rustc', '--version'], 'cargo': ['cargo', '--version'],
                'node': ['node', '--version'], 'npm': ['npm', '--version'],
                'uv': ['uv', '--version'], 'python': [sys.executable, '--version'],
                'macos': ['sw_vers', '-productVersion'],
            }.items()}}


def finish(root: Path, record: dict, directory: Path, names: list[str]) -> dict:
    if record.get('schema') != 1 or record.get('kind') != 'local-build-record':
        raise ValueError('unsupported start record')
    if identity(root) != record.get('source'):
        raise ValueError('source identity changed during release build')
    artifacts = {}
    for name in names:
        if Path(name).name != name or name in artifacts:
            raise ValueError('artifact names must be unique basenames')
        path = directory / name
        if path.is_symlink() or not path.is_file():
            raise ValueError('artifact must be a regular file: ' + name)
        digest = hashlib.sha256()
        with path.open('rb') as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b''):
                digest.update(block)
        artifacts[name] = {'bytes': path.stat().st_size, 'sha256': digest.hexdigest()}
    if not artifacts:
        raise ValueError('no release artifacts supplied')
    return {**record, 'artifacts': artifacts,
            'scope': 'Local source/tool/artifact record; not a signed attestation or proof that quality gates passed.'}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['start', 'finish'])
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--expected-ref')
    parser.add_argument('--record', type=Path, required=True)
    parser.add_argument('--directory', type=Path)
    parser.add_argument('--artifact', action='append', default=[])
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.mode == 'start':
        result = start(args.root, args.expected_ref)
        target = args.record
    else:
        if not args.directory or not args.output:
            parser.error('finish requires --directory and --output')
        result = finish(args.root, json.loads(args.record.read_text()), args.directory, args.artifact)
        target = args.output
    target.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print('release-provenance: ' + str(error), file=sys.stderr)
        sys.exit(1)
