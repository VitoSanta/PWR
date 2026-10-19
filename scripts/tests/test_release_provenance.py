import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('provenance', Path(__file__).resolve().parents[1] / 'release-provenance.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / 'repo'
        self.root.mkdir()
        self.git('init', '-q')
        self.git('config', 'user.name', 'Fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        (self.root / 'source').write_text('original')
        self.commit()
        self.record = {'schema': 1, 'kind': 'local-build-record', 'source': module.identity(self.root)}
        self.out = Path(self.tmp.name) / 'artifacts'
        self.out.mkdir()
        (self.out / 'app.dmg').write_bytes(b'fixture')

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], stderr=subprocess.DEVNULL, text=True)

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')

    def test_clean_artifact_identity(self):
        result = module.finish(self.root, self.record, self.out, ['app.dmg'])
        self.assertEqual(result['artifacts']['app.dmg'], {'bytes': 7, 'sha256': hashlib.sha256(b'fixture').hexdigest()})

    def test_dirty_tracked_and_untracked_refused(self):
        for name in ['source', 'untracked']:
            with self.subTest(name=name):
                path = self.root / name
                path.write_text('changed')
                with self.assertRaisesRegex(ValueError, 'clean source'):
                    module.identity(self.root)
                if name == 'source':
                    path.write_text('original')
                else:
                    path.unlink()

    def test_wrong_revision_refused(self):
        first = module.identity(self.root)['commit']
        (self.root / 'source').write_text('new')
        self.commit()
        with self.assertRaisesRegex(ValueError, 'does not match'):
            module.identity(self.root, first)
        with self.assertRaisesRegex(ValueError, 'identity changed'):
            module.finish(self.root, self.record, self.out, ['app.dmg'])

    def test_artifact_substitution_and_traversal_refused(self):
        (self.out / 'link').symlink_to(self.out / 'app.dmg')
        for names in [['link'], ['../app.dmg'], ['app.dmg', 'app.dmg'], ['missing'], []]:
            with self.subTest(names=names), self.assertRaises(ValueError):
                module.finish(self.root, self.record, self.out, names)

    def test_dirty_finish_refused(self):
        (self.root / 'source').write_text('changed during build')
        with self.assertRaisesRegex(ValueError, 'clean source'):
            module.finish(self.root, self.record, self.out, ['app.dmg'])


if __name__ == '__main__':
    unittest.main()
