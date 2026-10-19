#!/usr/bin/env python3
"""Verify assembled attribution integrity, without claiming legal clearance."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath


def verify(directory, target, uv=None):
    directory = Path(directory).resolve(strict=True)
    manifest = directory / 'inventory.json'
    if manifest.is_symlink():
        raise ValueError('The attribution inventory must not be a symlink')
    value = json.loads(manifest.read_text())
    if value.get('schema') != 1 or value.get('target') != target:
        raise ValueError('Attribution schema or target does not match the artifact')
    expected = {'inventory.json'}
    for record in value['files']:
        name = record['path']
        relative = PurePosixPath(name)
        if not name or '\\' in name or relative.is_absolute() or any(p in ('..', '.') for p in name.split('/')):
            raise ValueError('Unsafe attribution path')
        if name in expected:
            raise ValueError('Duplicate attribution path')
        expected.add(name)
        file = directory / name
        for component in [file, *file.parents]:
            if component == directory:
                break
            if component.is_symlink():
                raise ValueError('Attribution paths must not contain symlinks')
        if not file.is_file():
            raise ValueError(f'Missing attribution text: {name}')
        data = file.read_bytes()
        if len(data) != record['bytes'] or hashlib.sha256(data).hexdigest() != record['sha256']:
            raise ValueError(f'Attribution text does not match its inventory: {name}')
    actual = set()
    for file in directory.rglob('*'):
        if file.is_symlink():
            raise ValueError('Unlisted symlink in attribution directory')
        if file.is_file():
            actual.add(file.relative_to(directory).as_posix())
    if actual != expected:
        raise ValueError('Unlisted or missing files in attribution directory')
    if uv is not None:
        if hashlib.sha256(Path(uv).read_bytes()).hexdigest() != value['uv']['sha256']:
            raise ValueError('Bundled uv does not match the attribution inventory')
    return {'target': target, 'packages': len(value['rust']), 'texts': len(value['files']),
            'unresolved_review_items': len(value['unresolved']), 'legal_clearance': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--target', required=True)
    parser.add_argument('--uv', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.directory, args.target, args.uv)))
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f'verify-notices: {error}\n')
