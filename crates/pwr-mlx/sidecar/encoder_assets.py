"""Pinned E5 assets. Network access is confined to explicit --install setup."""
import argparse
import hashlib
import json
from pathlib import Path

MANIFEST_PATH = Path(__file__).with_name('encoder.json')
MANIFEST = json.loads(MANIFEST_PATH.read_text())
MODEL = MANIFEST['model']
REVISION = MANIFEST['revision']


def verify(directory, manifest=MANIFEST):
    directory = Path(directory)
    for name, expected in manifest['files'].items():
        if not name or Path(name).is_absolute() or '\\' in name or any(p in ('', '.', '..') for p in name.split('/')):
            raise ValueError('Invalid encoder asset path')
        file = directory / name
        digest = hashlib.sha256()
        size = 0
        with file.open('rb') as stream:
            while chunk := stream.read(1024 * 1024):
                size += len(chunk)
                digest.update(chunk)
        if size != expected['bytes'] or digest.hexdigest() != expected['sha256']:
            raise ValueError(f'Encoder asset integrity mismatch: {name}')
    return directory


def resolve_for_runtime(name):
    from huggingface_hub import snapshot_download
    if name != MODEL and Path(name).exists():
        return Path(name)
    arguments = {'repo_id': name, 'local_files_only': True}
    if name == MODEL:
        arguments.update(revision=REVISION, allow_patterns=list(MANIFEST['files']))
    directory = snapshot_download(**arguments)
    return verify(directory) if name == MODEL else Path(directory)


def install():
    from huggingface_hub import snapshot_download
    directory = snapshot_download(repo_id=MODEL, revision=REVISION,
                                  allow_patterns=list(MANIFEST['files']))
    verify(directory)
    return {'model': MODEL, 'revision': REVISION,
            'files': len(MANIFEST['files']),
            'payload_bytes': sum(file['bytes'] for file in MANIFEST['files'].values())}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install', action='store_true', required=True)
    parser.parse_args()
    print(json.dumps(install()))
