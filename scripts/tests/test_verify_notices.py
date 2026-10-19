"""Artifact-integrity regressions; these do not certify license obligations."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('notices', Path(__file__).resolve().parents[1] / 'verify-notices.py')
notices = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(notices)

class Notices(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.data = b'Attribution fixture\n'
        (self.root/'LICENSE.txt').write_bytes(self.data)
        self.value = {'schema':1, 'target':'aarch64-apple-darwin', 'files':[{'path':'LICENSE.txt','bytes':len(self.data),'sha256':hashlib.sha256(self.data).hexdigest()}], 'rust':[{}], 'unresolved':[{'name':'pending review'}], 'uv':{'sha256':hashlib.sha256(b'uv fixture').hexdigest()}}
        self.save()
    def save(self):
        (self.root/'inventory.json').write_text(json.dumps(self.value))
    def test_reports_open_review_items_without_claiming_clearance(self):
        result = notices.verify(self.root, 'aarch64-apple-darwin')
        self.assertEqual(result['unresolved_review_items'], 1)
        self.assertFalse(result['legal_clearance'])
    def test_rejects_changed_missing_or_unlisted_text(self):
        (self.root/'LICENSE.txt').write_bytes(b'altered')
        with self.assertRaises(ValueError): notices.verify(self.root, 'aarch64-apple-darwin')
        (self.root/'LICENSE.txt').unlink()
        with self.assertRaises(ValueError): notices.verify(self.root, 'aarch64-apple-darwin')
        (self.root/'LICENSE.txt').write_bytes(self.data)
        (self.root/'unexpected.key').write_bytes(b'synthetic extra file')
        with self.assertRaises(ValueError): notices.verify(self.root, 'aarch64-apple-darwin')
    def test_rejects_wrong_target_and_duplicate_paths(self):
        with self.assertRaises(ValueError): notices.verify(self.root, 'x86_64-pc-windows-msvc')
        self.value['files'] *= 2; self.save()
        with self.assertRaises(ValueError): notices.verify(self.root, 'aarch64-apple-darwin')
    def test_rejects_traversal_and_symlink(self):
        self.value['files'][0]['path'] = '../outside'; self.save()
        with self.assertRaises(ValueError): notices.verify(self.root, 'aarch64-apple-darwin')
        self.value['files'][0]['path'] = 'LICENSE.txt'; self.save()
        file = self.root/'LICENSE.txt'; file.unlink()
        try: file.symlink_to(self.root/'inventory.json')
        except OSError: self.skipTest('Symlinks unavailable on this test host')
        with self.assertRaises(ValueError): notices.verify(self.root, 'aarch64-apple-darwin')
    def test_rejects_unmatched_uv_binary(self):
        binary = self.root.parent/(self.root.name+'-uv')
        self.addCleanup(lambda: binary.unlink(missing_ok=True))
        binary.write_bytes(b'uv fixture')
        notices.verify(self.root, 'aarch64-apple-darwin', binary)
        binary.write_bytes(b'other executable')
        with self.assertRaises(ValueError): notices.verify(self.root, 'aarch64-apple-darwin', binary)

if __name__ == '__main__': unittest.main()
