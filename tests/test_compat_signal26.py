"""Table-pinned interp validation and native delegation via real octave-cli.

MATLAB provenance: dalga26/olc/olc26.m and olcum-uyum-snippet.txt,
interp rows 1-11. Only validation and printed shapes are MATLAB assertions;
interpolation coefficients and values deliberately remain native Octave.
"""
import shutil
import os
import subprocess
import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

from scripts import compat
from scripts import patch_packages as patcher

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')
PATCHES = ROOT / 'octave' / 'paket-yamalari' / 'signal-1.4.8'
SOURCE = ROOT / '.packages' / 'signal-1.4.8'
SCRATCH = Path(os.environ.get('INDYMAT_FLEET_SCRATCH', str(Path.home() / '.cache' / 'indymat-fleet' / 'dalga26' / 'uyum-tmp')))


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class Signal26CompatibilityTests(unittest.TestCase):
    def setUp(self):
        if not SOURCE.is_dir():
            self.skipTest('signal-1.4.8 is unavailable')
        SCRATCH.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix='signal26-', dir=SCRATCH)
        self.addCleanup(self.temp.cleanup)
        self.project = Path(self.temp.name) / 'project'
        self.package = self.project / '.packages' / 'signal-1.4.8'
        shutil.copytree(SOURCE, self.package, symlinks=True)
        self.manifest = patcher.load_manifest(PATCHES)
        # Reconstruct pristine inputs only in the private package copy.
        for item in self.manifest['files']:
            target = self.package / item['path']
            if patcher.digest(target.read_bytes()) == item['patched_sha256']:
                saved = self.package / patcher.STATE / 'pristine' / item['path']
                self.assertEqual(patcher.digest(saved.read_bytes()), item['pristine_sha256'])
                target.write_bytes(saved.read_bytes())
            self.assertEqual(patcher.digest(target.read_bytes()), item['pristine_sha256'])
        state = self.package / patcher.STATE
        if state.exists():
            shutil.rmtree(state)

    def apply(self):
        with contextlib.redirect_stdout(io.StringIO()) as output:
            patcher.patch_package(self.project, PATCHES)
        return output.getvalue()

    def run_octave(self, body):
        self.apply()
        # Bind native interp before prepending the transaction-patched copy.
        setup = compat.setup() + "f=str2func('interp'); "
        setup += f"addpath({compat.quote(self.package)},'-begin'); "
        setup += f"assert(strcmp(which('interp'),{compat.quote(self.package / 'interp.m')})); "
        result = subprocess.run(
            [OCTAVE, '--quiet', '--no-init-file', '--eval', setup + body],
            cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_measured_n_four_short_vector_rejection(self):
        self.run_octave("for args={{1:3,1},{1:3,2},{1:8,2},"
                        "{1:8,2,4},{1:8,2,4,.5}}, "
                        "failed=false; try, interp(args{1}{:}); catch e, "
                        "failed=strcmp(e.identifier,'signal:interp:InvalidDimensions') "
                        "&& strcmp(e.message,['Length of data sequence must be at least 9.' "
                        "char(10) ' You either need more data or a shorter filter (L).']); "
                        "end; assert(failed); end;")

    def test_long_row_column_values_delegate_unchanged(self):
        self.run_octave("for x={1:9, (1:9)', sin(1:24)}, for r=[2 3], "
                        "expected=f(x{1},r); actual=interp(x{1},r); "
                        "assert(isequal(actual,expected)); "
                        "assert(numel(actual)==numel(x{1})*r); "
                        "assert(isrow(actual)==isrow(x{1})); end; end;")

    def test_other_filter_forms_and_long_rate_one_delegate(self):
        self.run_octave("for args={{1:3,2,1},{1:3,2,1,1},{1:5,2,2},{1:9,1,4}}, "
                        "expected=f(args{1}{:}); actual=interp(args{1}{:}); "
                        "assert(isequal(actual,expected)); end;")

    def test_measured_single_default_rate_two_class(self):
        self.run_octave("x=single(1:9); expected=single(f(x,2)); "
                        "actual=interp(x,2); assert(isa(actual,'single')); "
                        "assert(isequal(size(actual),[1 18])); "
                        "assert(isequal(actual,expected)); "
                        "for args={{x,3},{x,2,4},{x,2,1}}, "
                        "assert(isequal(interp(args{1}{:}),single(f(args{1}{:})))); end;")

    def test_invalid_rate_retains_original_error(self):
        self.run_octave("for r=[0 -1 1.5 Inf NaN], "
                        "native_failed=false; shadow_failed=false; "
                        "try, f(1:3,r); catch e, native_failed=true; native=e; end; "
                        "try, interp(1:3,r); catch e, shadow_failed=true; shadow=e; end; "
                        "assert(native_failed && shadow_failed); "
                        "assert(strcmp(native.identifier,shadow.identifier)); "
                        "assert(strcmp(native.message,shadow.message)); end;")

    def test_manifest_transaction_check_and_idempotence(self):
        def snapshot():
            return {p.relative_to(self.package).as_posix(): p.read_bytes()
                    for p in self.package.rglob('*') if p.is_file() and not p.is_symlink()}
        def check():
            result = subprocess.run(
                [sys.executable, str(ROOT / 'scripts' / 'patch_packages.py'),
                 '--project', str(self.project), '--package', 'signal-1.4.8', '--check'],
                capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return result.stdout
        before = snapshot()
        self.assertIn('interp.m: pristine (check)', check())
        self.assertEqual(snapshot(), before)
        self.assertIn('interp.m: pristine (applied)', self.apply())
        after = snapshot()
        for item in self.manifest['files']:
            self.assertEqual(patcher.digest((self.package / item['path']).read_bytes()),
                             item['patched_sha256'])
        self.assertIn('interp.m: patched (check)', check())
        self.assertEqual(snapshot(), after)
        self.assertIn('interp.m: patched (skipped)', self.apply())
        self.assertEqual(snapshot(), after)
