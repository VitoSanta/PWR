"""Manifest syntax checks; these do not claim cryptographic verification."""
import base64
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / 'updater-manifest.py'

class Manifest(unittest.TestCase):
    def run_manifest(self, signature, repository='owner/project'):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / 'PWR.app.tar.gz').write_bytes(b'test archive, not installable')
            (root / 'notes').write_text('Changes\n')
            (root / 'sig').write_text(signature)
            output = root / 'latest.json'
            result = subprocess.run([sys.executable, str(SCRIPT), '--version', '0.4.2-alpha', '--notes', str(root/'notes'), '--archive', str(root/'PWR.app.tar.gz'), '--signature', str(root/'sig'), '--repository', repository, '--output', str(output)], capture_output=True)
            return result.returncode, json.loads(output.read_text()) if output.exists() else None
    def test_rejects_missing_or_malformed_signatures(self):
        for value in ('', 'garbage', base64.b64encode(b'not a signature').decode()):
            self.assertNotEqual(self.run_manifest(value)[0], 0)
    def test_manifest_preserves_release_identity_and_notes(self):
        signature = base64.b64encode(b'untrusted comment: synthetic syntax fixture\n').decode()
        code, manifest = self.run_manifest(signature)
        self.assertEqual(code, 0)
        self.assertEqual(manifest['version'], '0.4.2-alpha')
        self.assertEqual(manifest['notes'], 'Changes\n')
        self.assertEqual(manifest['platforms']['darwin-aarch64'], {'signature': signature, 'url':'https://github.com/owner/project/releases/download/v0.4.2-alpha/PWR.app.tar.gz'})
    def test_rejects_repository_path_injection(self):
        signature = base64.b64encode(b'untrusted comment: fixture\n').decode()
        self.assertNotEqual(self.run_manifest(signature, '../other/path')[0], 0)

if __name__ == '__main__':
    unittest.main()
