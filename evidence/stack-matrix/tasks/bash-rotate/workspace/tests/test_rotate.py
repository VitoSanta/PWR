import gzip
import os
import pathlib
import stat
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent.parent / "bin" / "rotate"


def rotate(*args):
    return subprocess.run(["bash", str(SCRIPT), *map(str, args)], capture_output=True, text=True)


class Rotate(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, name, text):
        path = self.dir / name
        path.write_text(text)
        return path

    def names(self):
        return sorted(p.name for p in self.dir.iterdir())

    def test_rotates_and_recreates(self):
        self.write("app.log", "one\n")
        done = rotate(self.dir)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout, "rotated app.log\n")
        self.assertEqual(self.names(), ["app.log", "app.log.1"])
        self.assertEqual((self.dir / "app.log").read_text(), "")
        self.assertEqual((self.dir / "app.log.1").read_text(), "one\n")

    def test_shifts_and_keeps_n(self):
        for generation in range(1, 5):
            self.write("app.log", f"gen {generation}\n")
            self.assertEqual(rotate("--keep", 2, self.dir).returncode, 0)
        self.assertEqual(self.names(), ["app.log", "app.log.1", "app.log.2"])
        self.assertEqual((self.dir / "app.log.1").read_text(), "gen 4\n")
        self.assertEqual((self.dir / "app.log.2").read_text(), "gen 3\n")

    def test_compress(self):
        self.write("web.log", "first\n")
        rotate("--compress", self.dir)
        self.write("web.log", "second\n")
        rotate("--compress", self.dir)
        self.assertEqual(self.names(), ["web.log", "web.log.1.gz", "web.log.2.gz"])
        with gzip.open(self.dir / "web.log.1.gz", "rt") as handle:
            self.assertEqual(handle.read(), "second\n")
        with gzip.open(self.dir / "web.log.2.gz", "rt") as handle:
            self.assertEqual(handle.read(), "first\n")

    def test_skips_empty_logs_and_other_files(self):
        self.write("empty.log", "")
        self.write("notes.txt", "x")
        (self.dir / "sub").mkdir()
        (self.dir / "sub" / "deep.log").write_text("x")
        done = rotate(self.dir)
        self.assertEqual(done.stdout, "")
        self.assertEqual(self.names(), ["empty.log", "notes.txt", "sub"])

    def test_keeps_permissions_and_handles_spaces(self):
        path = self.write("my app.log", "data\n")
        os.chmod(path, 0o640)
        done = rotate(self.dir)
        self.assertEqual(done.stdout, "rotated my app.log\n")
        self.assertEqual(stat.S_IMODE(os.stat(self.dir / "my app.log").st_mode), 0o640)
        self.assertEqual((self.dir / "my app.log.1").read_text(), "data\n")

    def test_usage_errors(self):
        self.write("app.log", "keep me\n")
        for args in [(), ("--keep", "0", self.dir), ("--keep", "x", self.dir), ("--keep",), ("--bogus", self.dir), (self.dir / "missing",)]:
            done = rotate(*args)
            self.assertEqual(done.returncode, 2, args)
            self.assertNotEqual(done.stderr.strip(), "", args)
        self.assertEqual(self.names(), ["app.log"])


if __name__ == "__main__":
    unittest.main()
