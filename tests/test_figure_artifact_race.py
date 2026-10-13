"""Artifact confinement races, with real filesystem replacements."""
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from backend.figure_artifacts import JSON_BYTES, FigureJSONTooLarge, read_figure_artifact


class FigureArtifactRaceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='artifact-race-', dir=ROOT)
        self.addCleanup(self.temp.cleanup)
        self.jobs = Path(self.temp.name) / 'jobs'
        self.job = 'a' * 32
        self.a = self.jobs / self.job
        self.b = self.jobs / ('b' * 32)
        self.a.mkdir(parents=True)
        self.b.mkdir()

    def test_final_symlink_swap_after_resolve_never_reads_other_job(self):
        for suffix in ('json', 'png'):
            with self.subTest(suffix=suffix):
                source = self.a / ('figure-1.' + suffix)
                other = self.b / source.name
                source.write_bytes(b'job A')
                other.write_bytes(b'job B')
                original = Path.resolve
                swapped = []

                def raced(path, *args, **kwargs):
                    resolved = original(path, *args, **kwargs)
                    if path == source and not swapped:
                        source.unlink()
                        source.symlink_to(other)
                        swapped.append(True)
                    return resolved

                with patch.object(Path, 'resolve', raced):
                    with self.assertRaises(PermissionError):
                        read_figure_artifact(self.jobs, self.job, source.name)
                self.assertTrue(swapped)

    def test_directory_swaps_after_resolve_are_rejected(self):
        for swap_job in (False, True):
            with self.subTest(swap_job=swap_job):
                nested = self.a / 'nested'
                nested.mkdir()
                source = nested / 'figure-2.json'
                source.write_bytes(b'job A')
                (self.b / source.name).write_bytes(b'job B')
                requested = self.a / source.name
                requested.symlink_to(source)
                victim = self.a if swap_job else nested
                parked = self.jobs / 'parked'
                original = Path.resolve
                swapped = []

                def raced(path, *args, **kwargs):
                    resolved = original(path, *args, **kwargs)
                    if path == requested and not swapped:
                        victim.rename(parked)
                        victim.symlink_to(self.b, target_is_directory=True)
                        swapped.append(True)
                    return resolved

                try:
                    with patch.object(Path, 'resolve', raced):
                        with self.assertRaises(PermissionError):
                            read_figure_artifact(self.jobs, self.job, requested.name)
                    self.assertTrue(swapped)
                finally:
                    victim.unlink()
                    parked.rename(victim)
                    requested.unlink()
                    source.unlink()
                    nested.rmdir()

    def test_swap_after_open_keeps_original_descriptor(self):
        source = self.a / 'figure-3.json'
        other = self.b / source.name
        source.write_bytes(b'job A')
        other.write_bytes(b'job B')
        original = os.fstat

        def raced(fd):
            info = original(fd)
            source.unlink()
            source.symlink_to(other)
            return info

        with patch('backend.figure_artifacts.os.fstat', raced):
            self.assertEqual(read_figure_artifact(self.jobs, self.job, source.name), b'job A')

    def test_growth_after_descriptor_stat_is_bounded(self):
        source = self.a / 'figure-4.json'
        source.write_bytes(b'[]')
        original = os.fstat

        def raced(fd):
            info = original(fd)
            source.write_bytes(b' ' * (JSON_BYTES + 1))
            return info

        with patch('backend.figure_artifacts.os.fstat', raced):
            with self.assertRaises(FigureJSONTooLarge):
                read_figure_artifact(self.jobs, self.job, source.name)

    def test_nonregular_artifact_is_rejected_without_blocking(self):
        source = self.a / 'figure-5.json'
        os.mkfifo(source)
        with self.assertRaises(PermissionError):
            read_figure_artifact(self.jobs, self.job, source.name)

    def test_real_kernel_job_staged_artifact(self):
        from backend.kernel import Kernel
        work = Path(self.temp.name) / 'work'
        work.mkdir()
        runtime = Path(self.temp.name) / 'runtime'
        kernel = Kernel(ROOT, runtime, work, executable='octave-cli')
        self.addCleanup(kernel.close)

        def idle():
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                state = kernel.snapshot()
                if state['status'] in ('idle', 'dead'):
                    self.assertEqual(state['status'], 'idle', state)
                    self.assertFalse(state['error'], state)
                    return
                time.sleep(.03)
            self.fail('Isolated Kernel did not finish within 20 seconds.')

        idle()
        job = kernel.submit("fid=fopen(fullfile(getappdata(0,'__mf_folder__'),'figure-1.json'),'wb'); "
                            "assert(fid>=0); fputs(fid,'{\"value\":42}'); fclose(fid);")
        idle()
        self.assertEqual(read_figure_artifact(runtime, job, 'figure-1.json'), b'{"value":42}')
        self.assertTrue((runtime / job / 'code.m').is_file())
        self.assertTrue((runtime / job / 'result.json').is_file())


if __name__ == '__main__':
    unittest.main()
