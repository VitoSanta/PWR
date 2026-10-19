#!/usr/bin/env python3
"""Build a Tauri static update manifest from an already signed release archive."""
import argparse
import base64
import json
import pathlib
import re


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('version', 'notes', 'archive', 'signature', 'repository', 'output'):
        p.add_argument('--' + name, required=True)
    a = p.parse_args()
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', a.version):
        p.error('invalid release version')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', a.repository):
        p.error('invalid repository')
    archive = pathlib.Path(a.archive)
    if not archive.is_file() or not archive.name.endswith('.app.tar.gz'):
        p.error('a built .app.tar.gz archive is required')
    signature = pathlib.Path(a.signature).read_text().strip()
    try:
        decoded = base64.b64decode(signature, validate=True)
        if not decoded.startswith(b'untrusted comment:'):
            raise ValueError('not a minisign signature')
    except (ValueError, base64.binascii.Error):
        p.error('invalid encoded updater signature')
    # The plugin verifies cryptographic trust at download; syntax here is no
    # substitute for key verification or the installed-version upgrade test.
    data = {'version': a.version, 'notes': pathlib.Path(a.notes).read_text(),
            'platforms': {'darwin-aarch64': {'signature': signature,
                'url': f'https://github.com/{a.repository}/releases/download/v{a.version}/{archive.name}'}}}
    pathlib.Path(a.output).write_text(json.dumps(data, indent=2) + '\n')


if __name__ == '__main__':
    main()
