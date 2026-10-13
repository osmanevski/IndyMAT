"""MATLAB R2025b sos next rows 1..13 and gain rows from olc26_signal_next.m.

Patching and all execution use a private package copy under dalga26. No live
package mutation. The polynomial shape rule is bounded to N=2/3 real doubles.
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
PATCHES = ROOT / 'octave' / 'paket-yamalari' / 'signal-1.4.8'
SOURCE = ROOT / '.packages' / 'signal-1.4.8'
SCRATCH = Path(os.environ.get('INDYMAT_FLEET_SCRATCH', str(
    Path.home() / '.cache' / 'indymat-fleet' / 'dalga26' / 'uyum-tmp')))


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


@unittest.skipUnless(OCTAVE, 'octave-cli unavailable')
class Sos2tf26Tests(unittest.TestCase):
    def setUp(self):
        if not SOURCE.is_dir():
            self.skipTest('signal package unavailable')
        SCRATCH.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(prefix='sos2tf26-', dir=SCRATCH)
        self.addCleanup(temporary.cleanup)
        self.project = Path(temporary.name) / 'project'
        self.package = self.project / '.packages' / 'signal-1.4.8'
        shutil.copytree(SOURCE, self.package, symlinks=True)
        self.manifest = patcher.load_manifest(PATCHES)
        self.assertIn('16-sos2tf-wave26.patch', self.manifest['patches'])
        for item in self.manifest['files']:
            target = self.package / item['path']
            if patcher.digest(target.read_bytes()) == item['patched_sha256']:
                saved = self.package / patcher.STATE / 'pristine' / item['path']
                self.assertEqual(patcher.digest(saved.read_bytes()), item['pristine_sha256'])
                target.write_bytes(saved.read_bytes())
            self.assertEqual(patcher.digest(target.read_bytes()), item['pristine_sha256'])
        shutil.rmtree(self.package / patcher.STATE, ignore_errors=True)
        self.native = Path(temporary.name) / 'native'
        self.native.mkdir()
        shutil.copy2(self.package / 'sos2tf.m', self.native / 'sos2tf.m')

    def apply(self, **kwargs):
        with contextlib.redirect_stdout(io.StringIO()):
            return patcher.patch_package(self.project, PATCHES, **kwargs)

    def run_octave(self, code):
        self.apply()
        setup = (f"addpath({quote(self.native)});f=@sos2tf;"
                 f"addpath({quote(self.package)},'-begin');"
                 f"assert(strcmp(which('sos2tf'),{quote(self.package / 'sos2tf.m')}));")
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file',
                                 '--no-site-file', '--no-history', '--eval', setup + code.replace('\n', ' ')],
                                cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_thirteen_measured_polynomial_rows(self):
        self.run_octave("""
A=[1 2 3 1 -.5 .1];Z=[1 0 0 1 -.2 .05];
ss={[A;Z],[Z;A],[A;Z;Z],[Z;A;Z],[Z;Z;A],[Z;Z;Z],
[1 2 0 1 -.5 .1;1 3 0 1 -.2 .05],
[1 2 0 1 -.5 .1;1 -2 0 1 -.2 .05],
[1 0 0 1 0 0;1 0 0 1 0 0],
[0 0 0 1 -.5 .1;1 2 3 1 -.2 .05],
[0 1 0 1 -.5 .1;1 0 0 1 -.2 .05],
[2 4 6 2 -1 .2;1 0 0 1 -.2 .05],
[1 2 3 1 -.5 0;1 2 1 1 -.2 0;1 1 0 1 -.1 0]};
eb={[1 2 3 0],[1 2 3 0],[1 2 3 0 0 0],[1 2 3 0 0 0],
[1 2 3 0 0 0],[1 0 0 0 0 0],[1 5 6 0],[1 0 -4 0],
[1 0 0 0],[0 0 0 0],[0 1 0 0],[2 4 6 0],[1 5 12 16 11 3]};
a2=[1 -.7 .25 -.045 .005];a3=[1 -.9 .44 -.13 .0265 -.00325 .00025];
ea={a2,a2,a3,a3,a3,[1 -.6 .27 -.068 .0135 -.0015 .000125],
a2,a2,[1 0 0 0],a2,a2,2*a2,[1 -.8 .17 -.01 0 0]};
for k=1:13,[b,a]=sos2tf(ss{k});
assert(isa(b,'double') && isa(a,'double'));
assert(isequal(size(b),size(eb{k})) && isequal(size(a),size(ea{k})));
assert(max(abs(b-eb{k}))<1e-14 && max(abs(a-ea{k}))<1e-14);end;
""")

    def test_measured_gains_and_original_delay_rows(self):
        self.run_octave("""
s=[1 2 3 1 -.5 .1;1 0 0 1 -.2 .05];
for g={0,2,[2 3]},[b,a]=sos2tf(s,g{1});
assert(isequal(b,prod(g{1})*[1 2 3 0]));
assert(max(abs(a-[1 -.7 .25 -.045 .005]))<1e-14);end;
[b,a]=sos2tf([0 0 1 1 -.5 .1;0 0 1 1 -.2 .05]);
assert(isequal(b,[0 0 0 0 1]) && numel(a)==5);
[b,a]=sos2tf([1 2 3 1 -.5 0;1 2 1 1 -.2 0]);
assert(isequal(b,[1 4 8 8 3]) && numel(a)==4);
assert(max(abs(a-[1 -.7 .1 0]))<1e-14);
""")

    def test_outside_subset_preserves_native_values_and_errors(self):
        self.run_octave("""
s=[1 0 0 1 -.2 .05];
for args={{s},{repmat(s,4,1)},{single([s;s])},
{[s;s]+1i},{[s;s],single(2)},{[s;s],[]},{[s;s],[2 3;4 5]},
{[s;s],Inf},{[s;s],'bad'},{[s;s;s;s],2}},
try,[eb,ea]=f(args{1}{:});ne='';catch e,ne=e.message;end;
try,[b,a]=sos2tf(args{1}{:});se='';catch e,se=e.message;end;
assert(strcmp(ne,se));if(isempty(ne)),assert(isequaln(b,eb)&&isequaln(a,ea));end;end;
""")

    def test_registered_hashes_idempotence_and_revert(self):
        self.assertEqual(self.apply(check=True)['sos2tf.m'], 'pristine')
        self.apply()
        item = next(x for x in self.manifest['files'] if x['path'] == 'sos2tf.m')
        self.assertEqual(patcher.digest((self.package / 'sos2tf.m').read_bytes()),
                         item['patched_sha256'])
        self.assertEqual(self.apply()['sos2tf.m'], 'patched')
        self.apply(revert=True)
        self.assertEqual(patcher.digest((self.package / 'sos2tf.m').read_bytes()),
                         item['pristine_sha256'])


if __name__ == '__main__':
    unittest.main()
