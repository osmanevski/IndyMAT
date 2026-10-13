"""Measured norm/norminv subsets, exercised only with real octave-cli.

Provenance: dalga26/olc/olc26.m norm p and norminv tables, printed in
olcum-uyum-snippet.txt. Negative nonzero vector norms and infinite p=0 inputs were measured in round4;
norminv confidence interval outputs remain outside the added subset.
"""
import contextlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from scripts import patch_packages as patcher

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')
PATCHES = ROOT / 'octave' / 'paket-yamalari' / 'statistics-2.0.0'
SOURCE = ROOT / '.packages' / 'statistics-2.0.0'
SCRATCH = Path(os.environ.get('INDYMAT_FLEET_SCRATCH', str(
    Path.home() / '.cache' / 'indymat-fleet' / 'dalga26' / 'uyum-tmp')))


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class Numeric26Round2Tests(unittest.TestCase):
    def run_octave(self, body, setup=''):
        result = subprocess.run(
            [OCTAVE, '--quiet', '--no-init-file', '--no-site-file', '--no-history',
             '--eval', "warning('off','Octave:shadowed-function');" + setup + body],
            cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_measured_vector_zero_norm_values_and_class(self):
        self.run_octave("""
xs={0,1,[0 0],[3 0 -2],single([3 0 -2]),[NaN 1]};
expected={1,1,Inf,Inf,single(Inf),NaN};
for k=1:numel(xs), actual=norm(xs{k},0);
assert(isequaln(actual,expected{k}));
assert(strcmp(class(actual),class(xs{k})));end;
""", f"addpath({quote(ROOT / 'octave' / 'compat')});")

    def test_measured_negative_vector_norm_subset(self):
        self.run_octave("""
xs={0,1,[0 0],[3 0 -2],single([3 0 -2]),[NaN 1]};
expected={0,1,0,0,single(0),NaN};
for k=1:numel(xs),for p=[-1 -2],actual=norm(xs{k},p);
assert(isequaln(actual,expected{k}));
assert(strcmp(class(actual),class(xs{k})));end;end;
""", f"addpath({quote(ROOT / 'octave' / 'compat')});")

    def test_measured_empty_and_matrix_rejections(self):
        self.run_octave("""
for x={[],[3 0;0 -2]},for p=[0 -1 -2 -Inf],failed=false;
try,norm(x{1},p);catch e,failed=strcmp(e.identifier,'MATLAB:norm:unknownNorm');end;
assert(failed);end;end;
""", f"addpath({quote(ROOT / 'octave' / 'compat')});")

    def test_unchanged_norm_forms_delegate(self):
        self.run_octave("""
for x={[3 4],single([3 4]),[1 2;3 4],[3+4i 1-2i],[]},
assert(isequaln(norm(x{1}),builtin('norm',x{1})));
for p={1,2,Inf,'fro'},assert(isequaln(norm(x{1},p{1}),builtin('norm',x{1},p{1})));end;end;
for x={0,1,[0 0],[3 0 -2],single([3 0 -2]),[NaN 1]},
assert(isequaln(norm(x{1},-Inf),builtin('norm',x{1},-Inf)));end;
""", f"addpath({quote(ROOT / 'octave' / 'compat')});")

    def private_package(self):
        if not SOURCE.is_dir():
            self.skipTest('statistics-2.0.0 unavailable')
        SCRATCH.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(prefix='numeric26-', dir=SCRATCH)
        self.addCleanup(temporary.cleanup)
        project = Path(temporary.name) / 'project'
        package = project / '.packages' / 'statistics-2.0.0'
        shutil.copytree(SOURCE, package, symlinks=True)
        manifest = patcher.load_manifest(PATCHES)
        for item in manifest['files']:
            target = package / item['path']
            if patcher.digest(target.read_bytes()) == item['patched_sha256']:
                saved = package / patcher.STATE / 'pristine' / item['path']
                self.assertEqual(patcher.digest(saved.read_bytes()), item['pristine_sha256'])
                target.write_bytes(saved.read_bytes())
            self.assertEqual(patcher.digest(target.read_bytes()), item['pristine_sha256'])
        state = package / patcher.STATE
        if state.exists():
            shutil.rmtree(state)
        return project, package, manifest

    def test_measured_norminv_fourth_argument_is_ignored(self):
        project, package, _ = self.private_package()
        with contextlib.redirect_stdout(io.StringIO()):
            patcher.patch_package(project, PATCHES)
        distribution = package / 'Distribution_Functions'
        self.run_octave("""
p=[0 .1 .5 .9 1];baseline=norminv(p,0,1);
assert(isequal(size(baseline),[1 5]));
assert(baseline(1)==-Inf && baseline(5)==Inf);
assert(max(abs(baseline(2:4)-[-1.28155 0 1.28155]))<5e-6);
for fourth={'upper','lower','bogus'},
assert(isequal(norminv(p,0,1,fourth{1}),baseline));end;
assert(isequal(norminv([.1 .5 .9],0,1,zeros(2,3)),baseline(2:4)));
assert(max(abs(norminv([.1 .5 .9],2,3)-(2+3*baseline(2:4))))<1e-12);
assert(isa(norminv(single([.1 .5 .9]),0,1),'single'));
""", f"addpath({quote(distribution)},'-begin');"
             f"assert(strcmp(which('norminv'),{quote(distribution / 'norminv.m')}));")

    def test_statistics_patch_hashes_check_and_idempotence(self):
        project, package, manifest = self.private_package()
        def snapshot():
            return {p.relative_to(package).as_posix(): p.read_bytes()
                    for p in package.rglob('*') if p.is_file() and not p.is_symlink()}
        def apply(**kwargs):
            with contextlib.redirect_stdout(io.StringIO()):
                return patcher.patch_package(project, PATCHES, **kwargs)
        before = snapshot()
        states = apply(check=True)
        self.assertEqual(set(states.values()), {'pristine'})
        self.assertEqual(snapshot(), before)
        apply()
        for item in manifest['files']:
            self.assertEqual(patcher.digest((package / item['path']).read_bytes()),
                             item['patched_sha256'])
        after = snapshot()
        self.assertEqual(set(apply(check=True).values()), {'patched'})
        self.assertEqual(snapshot(), after)
        self.assertEqual(set(apply().values()), {'patched'})
        self.assertEqual(snapshot(), after)
        apply(revert=True)
        for item in manifest['files']:
            self.assertEqual(patcher.digest((package / item['path']).read_bytes()),
                             item['pristine_sha256'])


if __name__ == '__main__':
    unittest.main()
