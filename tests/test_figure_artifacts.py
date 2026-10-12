"""Bounded artifact-reader checks; no server or graphics process needed."""
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from backend.figure_artifacts import JSON_BYTES, FigureJSONTooLarge, read_figure_artifact


class FigureArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="figure-artifact-test-", dir=ROOT)
        self.addCleanup(self.temp.cleanup)
        self.jobs = Path(self.temp.name) / "jobs"
        self.job = "a" * 32
        self.folder = self.jobs / self.job
        self.folder.mkdir(parents=True)
        self.file = self.folder / "figure-1.json"

    def read(self):
        return read_figure_artifact(self.jobs, self.job, self.file.name)

    def test_empty_singleton_utf8_and_exact_byte_budget(self):
        for data in [b"", b"[]", b"[1]", '{"title":"ğ"}'.encode("utf-8"), b" " * JSON_BYTES]:
            with self.subTest(size=len(data)):
                self.file.write_bytes(data)
                self.assertEqual(self.read(), data)

    def test_oversized_json_is_rejected_before_open(self):
        self.file.write_bytes(b" " * (JSON_BYTES + 1))
        with patch.object(Path, "open", side_effect=AssertionError("unbounded file opened")):
            with self.assertRaises(FigureJSONTooLarge) as caught:
                self.read()
        self.assertEqual(caught.exception.reason_args, {"actual": JSON_BYTES + 1, "limit": JSON_BYTES})

    def test_file_growth_after_stat_cannot_bypass_read_budget(self):
        self.file.write_bytes(b" " * (JSON_BYTES + 1))
        with patch.object(Path, "stat", return_value=SimpleNamespace(st_size=1)):
            with self.assertRaises(FigureJSONTooLarge):
                self.read()

    def test_missing_file_and_symlink_escape(self):
        with self.assertRaises(FileNotFoundError):
            self.read()
        outside = Path(self.temp.name) / "outside.json"
        outside.write_bytes(b"[]")
        self.file.symlink_to(outside)
        with self.assertRaises(PermissionError):
            self.read()

    def test_png_bytes_remain_available_independently_of_json_budget(self):
        image = self.folder / "figure-1.png"
        image.write_bytes(b"\x89PNG\r\n\x1a\n")
        self.file.write_bytes(b" " * (JSON_BYTES + 1))
        with self.assertRaises(FigureJSONTooLarge):
            self.read()
        self.assertEqual(read_figure_artifact(self.jobs, self.job, image.name), image.read_bytes())

    def test_cross_job_file_symlinks_are_rejected(self):
        other = self.jobs / ("b" * 32)
        other.mkdir()
        for suffix in ["json", "png"]:
            with self.subTest(suffix=suffix):
                name = "figure-1." + suffix
                (other / name).write_bytes(b"[]")
                (self.folder / name).symlink_to(other / name)
                with self.assertRaises(PermissionError):
                    read_figure_artifact(self.jobs, self.job, name)

    def test_job_directory_symlink_cannot_alias_another_job(self):
        self.file.write_bytes(b"[]")
        alias = "b" * 32
        (self.jobs / alias).symlink_to(self.folder, target_is_directory=True)
        with self.assertRaises(PermissionError):
            read_figure_artifact(self.jobs, alias, self.file.name)

    def test_same_job_file_symlinks_remain_readable(self):
        nested = self.folder / "nested"
        nested.mkdir()
        for suffix in ["json", "png"]:
            with self.subTest(suffix=suffix):
                name = "figure-1." + suffix
                data = b"[]" if suffix == "json" else b"\x89PNG\r\n\x1a\n"
                (nested / name).write_bytes(data)
                (self.folder / name).symlink_to(nested / name)
                self.assertEqual(read_figure_artifact(self.jobs, self.job, name), data)


if __name__ == "__main__":
    unittest.main()
