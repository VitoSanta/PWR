import os
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent.parent / "bin" / "rotate"


def rotate(*args):
    return subprocess.run(["bash", str(SCRIPT), *map(str, args)], capture_output=True, text=True)


class Hidden(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_several_logs_in_name_order(self):
        for name in ["b.log", "a.log", "c.log"]:
            (self.dir / name).write_text("x")
        self.assertEqual(rotate(self.dir).stdout, "rotated a.log\nrotated b.log\nrotated c.log\n")

    def test_symlinks_are_left_alone(self):
        target = self.dir / "real.txt"
        target.write_text("x")
        os.symlink(target, self.dir / "link.log")
        done = rotate(self.dir)
        self.assertEqual(done.stdout, "")
        self.assertTrue((self.dir / "link.log").is_symlink())

    def test_keep_one_deletes_older_copies_already_there(self):
        (self.dir / "app.log").write_text("new")
        (self.dir / "app.log.1").write_text("old1")
        (self.dir / "app.log.2").write_text("old2")
        (self.dir / "app.log.3").write_text("old3")
        rotate("--keep", 1, self.dir)
        self.assertEqual(sorted(p.name for p in self.dir.iterdir()), ["app.log", "app.log.1"])
        self.assertEqual((self.dir / "app.log.1").read_text(), "new")

    def test_keep_99_and_100(self):
        (self.dir / "app.log").write_text("x")
        self.assertEqual(rotate("--keep", 99, self.dir).returncode, 0)
        self.assertEqual(rotate("--keep", 100, self.dir).returncode, 2)

    def test_a_file_is_not_a_directory(self):
        path = self.dir / "file"
        path.write_text("x")
        self.assertEqual(rotate(path).returncode, 2)

    def test_names_with_glob_characters(self):
        (self.dir / "[weird] *.log").write_text("x")
        done = rotate(self.dir)
        self.assertEqual(done.stdout, "rotated [weird] *.log\n")
        self.assertTrue((self.dir / "[weird] *.log.1").exists())
