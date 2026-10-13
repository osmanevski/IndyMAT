"""Measured interp next rows 1..13; q>1 values remain native Octave.

Private patcher transactions use a scratch copy of the versioned manifest,
including patch17 until the root lane integrates it. No live package writes.
"""
import json
import shutil
from pathlib import Path
import subprocess
import unittest

from scripts import patch_packages as patcher
from tests import test_sos2tf26 as sos

OCTAVE, ROOT, PATCHES, quote = sos.OCTAVE, sos.ROOT, sos.PATCHES, sos.quote


@unittest.skipUnless(OCTAVE, 'octave-cli unavailable')
class Interp26Round4Tests(sos.Sos2tf26Tests):
    # This class reuses only private package preparation, not SOS test methods.
    test_thirteen_measured_polynomial_rows = None
    test_measured_gains_and_original_delay_rows = None
    test_outside_subset_preserves_native_values_and_errors = None
    test_registered_hashes_idempotence_and_revert = None

    def setUp(self):
        super().setUp()
        shutil.copy2(self.package / 'interp.m', self.native / 'interp.m')
        self.patch_folder = self.project.parent / 'patches' / PATCHES.name
        shutil.copytree(PATCHES, self.patch_folder)
        manifest = json.loads((self.patch_folder / 'manifest.json').read_text())
        if '17-interp-wave26.patch' not in manifest['patches']:
            manifest['patches'].append('17-interp-wave26.patch')
        for item in manifest['files']:
            if item['path'] == 'interp.m':
                item['patched_sha256'] = 'a01590f83d57a1b2aef47187d81e85048c9f955c5051ff566c9c8f90ee121a58'
        (self.patch_folder / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')

    def apply(self, **kwargs):
        import contextlib
        import io
        with contextlib.redirect_stdout(io.StringIO()):
            return patcher.patch_package(self.project, self.patch_folder, **kwargs)

    def run_octave(self, code):
        self.apply()
        setup = (f"addpath({quote(self.package)});addpath({quote(self.native)});f=@interp;"
                 f"addpath({quote(self.package)},'-begin');"
                 f"assert(strcmp(which('interp'),{quote(self.package / 'interp.m')}));")
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file',
                                 '--no-site-file', '--no-history', '--eval', setup + code],
                                cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_measured_n_dependent_validation_and_acceptance(self):
        self.run_octave("""
for n=1:3,failed=false;
try,interp(1:(2*n),2,n);catch e,
failed=strcmp(e.identifier,'signal:interp:InvalidDimensions') && ...
strcmp(e.message,[sprintf('Length of data sequence must be at least %d.',2*n+1) ...
char(10) ' You either need more data or a shorter filter (L).']);end;
assert(failed);y=interp(1:(2*n+1),2,n);
assert(isa(y,'double') && isequal(size(y),[1 2*(2*n+1)]));
assert(isequal(y,f(1:(2*n+1),2,n)));end;
failed=false;try,interp(1:8,2,4,1);catch e,
failed=strcmp(e.identifier,'signal:interp:InvalidDimensions');end;assert(failed);
""")

    def test_measured_rate_one_exact_identity(self):
        self.run_octave("""
for x={1:9,single(1:9)},y=interp(x{1},1);
assert(isequal(y,x{1}) && strcmp(class(y),class(x{1})));end;
""")

    def test_measured_single_forms_preserve_class_shape(self):
        self.run_octave("""
as={{single(1:9),3},{single(1:9),2,4},{single((1:9)'),2},{single(1:3),2,1}};
for k=1:numel(as),y=interp(as{k}{:});
assert(isa(y,'single'));
assert(isequal(y,single(f(as{k}{:}))));
assert(numel(y)==numel(as{k}{1})*as{k}{2});
assert(isrow(y)==isrow(as{k}{1}));end;
""")

    def test_unmeasured_forms_stay_native(self):
        self.run_octave("""
as={{1:4,3,1},{1:4,2,1,1},{single(1:5),2,2}, ...
{single(1:7),2,3},{1:11,2,5},{1:9,2,4,.8}, ...
{[1+1i 2 3],2,1},{1:9,1,4}};
for k=1:numel(as),assert(isequaln(interp(as{k}{:}),f(as{k}{:})));end;
""")

    def test_cumulative_hash_transaction_and_revert(self):
        self.assertEqual(self.apply(check=True)['interp.m'], 'pristine')
        self.apply()
        self.assertEqual(patcher.digest((self.package/'interp.m').read_bytes()),
                         'a01590f83d57a1b2aef47187d81e85048c9f955c5051ff566c9c8f90ee121a58')
        self.assertEqual(self.apply()['interp.m'], 'patched')
        self.apply(revert=True)
        self.assertEqual(patcher.digest((self.package/'interp.m').read_bytes()),
                         '7cdcd0e04831379fd5a892f4a989f8200e23db0ef3836ca11785b2ce8864f0e3')


if __name__ == '__main__':
    unittest.main()
