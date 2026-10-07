"""Package patch transactions use temporary copies, never the live package tree."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'scripts' / 'patch_packages.py'
PATCHES = ROOT / 'octave' / 'paket-yamalari' / 'datatypes-1.5.0'
SOURCE = ROOT / '.packages' / 'datatypes-1.5.0'
SPEC = importlib.util.spec_from_file_location('indymat_package_patches', SCRIPT)
patcher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(patcher)


def snapshot(folder):
    return {p.relative_to(folder).as_posix(): p.read_bytes()
            for p in folder.rglob('*') if p.is_file() and not p.is_symlink()}


class PackagePatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not SOURCE.is_dir():
            raise unittest.SkipTest('datatypes-1.5.0 is not installed in this test project')
        cls.manifest = patcher.load_manifest(PATCHES)

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='indymat-patch-test-')
        self.addCleanup(self.temp.cleanup)
        self.project = Path(self.temp.name) / 'project'
        self.package = self.project / '.packages' / 'datatypes-1.5.0'
        shutil.copytree(SOURCE, self.package, symlinks=True)
        # The source may already be patched. Restore ONLY this temporary copy.
        for item in self.manifest['files']:
            path = self.package / item['path']
            if patcher.digest(path.read_bytes()) == item['patched_sha256']:
                saved = self.package / patcher.STATE / 'pristine' / item['path']
                self.assertEqual(patcher.digest(saved.read_bytes()), item['pristine_sha256'])
                path.write_bytes(saved.read_bytes())
            self.assertEqual(patcher.digest(path.read_bytes()), item['pristine_sha256'])
        state = self.package / patcher.STATE
        if state.exists():
            shutil.rmtree(state)
        self.before = snapshot(self.package)

    def cli(self, *args):
        return subprocess.run([sys.executable, str(SCRIPT), '--project', str(self.project), *args],
                              capture_output=True, text=True, timeout=30)

    def apply(self, **kwargs):
        with contextlib.redirect_stdout(io.StringIO()) as output:
            patcher.patch_package(self.project, PATCHES, **kwargs)
        return output.getvalue()

    def test_round_trip_and_idempotence(self):
        self.assertIn('pristine (applied)', self.apply())
        after = snapshot(self.package)
        for item in self.manifest['files']:
            self.assertEqual(patcher.digest((self.package / item['path']).read_bytes()), item['patched_sha256'])
            self.assertEqual((self.package / patcher.STATE / 'pristine' / item['path']).read_bytes(), self.before[item['path']])
        self.assertIn('patched (skipped)', self.apply())
        self.assertEqual(snapshot(self.package), after)
        self.assertIn('patched (reverted)', self.apply(revert=True))
        # Registry/backups remain for auditing; every original file is identical.
        for name, data in self.before.items():
            self.assertEqual((self.package / name).read_bytes(), data)
        self.assertIn('pristine (skipped)', self.apply(revert=True))
        self.assertIn('pristine (applied)', self.apply())

    def test_check_never_writes(self):
        result = self.cli('--check')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('pristine (check)', result.stdout)
        self.assertEqual(snapshot(self.package), self.before)
        self.apply()
        after = snapshot(self.package)
        result = self.cli('--check')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('patched (check)', result.stdout)
        self.assertEqual(snapshot(self.package), after)

    def test_already_patched_without_backup_skips_but_cannot_revert(self):
        self.apply()
        shutil.rmtree(self.package / patcher.STATE)
        before = snapshot(self.package)
        self.assertIn('patched (skipped)', self.apply())
        self.assertIn('patched (check)', self.apply(check=True))
        self.assertEqual(snapshot(self.package), before)
        with self.assertRaisesRegex(patcher.PatchError, 'no verified pristine backup'):
            self.apply(revert=True)
        self.assertEqual(snapshot(self.package), before)

    def test_unknown_target_refuses_every_mode_without_writes(self):
        target = self.package / 'string.m'
        target.write_bytes(target.read_bytes() + b'\n% local modification\n')
        before = snapshot(self.package)
        for args in ((), ('--check',), ('--revert',)):
            result = self.cli(*args)
            self.assertEqual(result.returncode, 1)
            self.assertIn('Refusing modified or unsupported target', result.stderr)
            self.assertIn('nothing changed', result.stderr)
            self.assertEqual(snapshot(self.package), before)

    def test_modified_patched_target_and_backup_refuse(self):
        self.apply()
        saved = self.package / patcher.STATE / 'pristine' / 'string.m'
        saved.write_bytes(saved.read_bytes() + b'\n')
        before = snapshot(self.package)
        result = self.cli('--revert')
        self.assertEqual(result.returncode, 1)
        self.assertIn('modified pristine backup', result.stderr)
        self.assertEqual(snapshot(self.package), before)

    def test_registry_mismatch_refuses(self):
        self.apply()
        registry = self.package / patcher.STATE / 'registry.json'
        registry.write_text('{}\n')
        before = snapshot(self.package)
        self.assertEqual(self.cli().returncode, 1)
        self.assertEqual(snapshot(self.package), before)

    def test_older_patch_set_is_upgraded_in_one_step(self):
        self.apply()
        item = self.manifest['files'][0]
        target = self.package / item['path']
        older = target.read_bytes() + b'\n% from an earlier patch set\n'
        target.write_bytes(older)
        previous = json.loads(json.dumps(self.manifest))
        previous['files'][0]['patched_sha256'] = patcher.digest(older)
        registry = self.package / patcher.STATE / 'registry.json'
        registry.write_text(json.dumps(previous, indent=2) + '\n')
        self.assertEqual(self.cli('--check').returncode, 0)
        self.assertEqual(target.read_bytes(), older)
        self.assertIn('outdated (upgraded)', self.apply())
        self.assertEqual(patcher.digest(target.read_bytes()), item['patched_sha256'])
        self.assertEqual(registry.read_text(), json.dumps(self.manifest, indent=2) + '\n')
        self.assertIn('patched (skipped)', self.apply())
        # An unknown edit is still refused, even under a different registry.
        target.write_bytes(older + b'user edit\n')
        registry.write_text(json.dumps(previous, indent=2) + '\n')
        before = snapshot(self.package)
        self.assertEqual(self.cli().returncode, 1)
        self.assertEqual(snapshot(self.package), before)

    def test_symlink_target_cannot_escape(self):
        outside = Path(self.temp.name) / 'outside.m'
        outside.write_bytes(self.before['string.m'])
        (self.package / 'string.m').unlink()
        (self.package / 'string.m').symlink_to(outside)
        before = outside.read_bytes()
        result = self.cli()
        self.assertEqual(result.returncode, 1)
        self.assertIn('symlink', result.stderr)
        self.assertEqual(outside.read_bytes(), before)
        self.assertFalse((self.package / patcher.STATE).exists())

    def test_symlink_registry_directory_cannot_escape(self):
        outside = Path(self.temp.name) / 'outside'
        outside.mkdir()
        (self.package / patcher.STATE).symlink_to(outside, target_is_directory=True)
        result = self.cli()
        self.assertEqual(result.returncode, 1)
        self.assertIn('symlink', result.stderr)
        self.assertEqual(list(outside.iterdir()), [])
        self.assertEqual((self.package / 'string.m').read_bytes(), self.before['string.m'])

    def test_missing_version_refuses(self):
        self.package.rename(self.package.with_name('datatypes-1.6.0'))
        result = self.cli()
        self.assertEqual(result.returncode, 1)
        self.assertIn('version is absent', result.stderr)
        self.assertEqual(snapshot(self.package.with_name('datatypes-1.6.0')), self.before)

    def test_commit_failure_rolls_back_target_backup_and_registry(self):
        atomic = patcher.atomic
        calls = 0
        def fail_registry(*args):
            nonlocal calls
            calls += 1
            if calls == 3:
                raise OSError('simulated registry write failure')
            return atomic(*args)
        with mock.patch.object(patcher, 'atomic', side_effect=fail_registry):
            with self.assertRaisesRegex(OSError, 'simulated'):
                self.apply()
        self.assertEqual(snapshot(self.package), self.before)
        self.assertFalse((self.package / patcher.STATE).exists())

    def test_all_targets_are_validated_before_any_write(self):
        folder = Path(self.temp.name) / 'patches' / 'datatypes-1.5.0'
        shutil.copytree(PATCHES, folder)
        manifest = json.loads((folder / 'manifest.json').read_text())
        manifest['files'].append({'path': 'second.m', 'pristine_sha256': patcher.digest(b'expected\n'),
                                  'patched_sha256': patcher.digest(b'patched\n')})
        (folder / 'manifest.json').write_text(json.dumps(manifest))
        (self.package / 'second.m').write_bytes(b'unexpected\n')
        before = snapshot(self.package)
        with self.assertRaisesRegex(patcher.PatchError, 'Refusing modified'):
            patcher.patch_package(self.project, folder)
        self.assertEqual(snapshot(self.package), before)

    def test_exact_unified_context_is_required(self):
        bad = '--- a/x.m\n+++ b/x.m\n@@ -1 +1 @@\n-wrong\n+new\n'
        with self.assertRaisesRegex(patcher.PatchError, 'context'):
            patcher.unified(b'old\n', bad)
        self.assertEqual(patcher.unified(b'old\n', bad.replace('-wrong', '-old')), ('x.m', b'new\n'))
        for path in ('../x', '/x', 'a/../../x', 'a\\x'):
            with self.assertRaises(patcher.PatchError):
                patcher.relative(path)

    def test_unsupported_mixed_operands_raise_clear_errors(self):
        exe = shutil.which('octave-cli')
        if not exe:
            self.skipTest('octave-cli is unavailable')
        self.apply()
        calls = ["contains('abc', string(NaN))", "count('abc', string(NaN))",
                 "replace('abc', string(NaN), 'x')", "erase('abc', string(NaN))",
                 "split('a,b', string(NaN))",
                 "join({'a', 'b'}, string(NaN))"]
        unsupported_plus = ["string('x') + 3.5", "string('x') + 10000",
                            "string('x') + Inf", "string('x') + true",
                            "string('x') + ['ab'; 'cd']"]
        code = "warning ('off', 'Octave:shadowed-function');\n"
        # Measured in MATLAB R2025b (wave 19): string + integer array and compose with
        # string-array data are valid; compose with a missing element is an error.
        missing_compose = ["compose('%s', string(NaN))"]
        for call in calls + unsupported_plus + missing_compose:
            message = ('missing string arguments are not supported' if call in calls else
                       'non-string operands must be char' if call in unsupported_plus else
                       '<missing> string element not supported')
            code += (f"caught = false; try, {call}; catch e, "
                     f"caught = ! isempty(strfind(e.message, '{message}')); "
                     "end_try_catch; assert(caught);\n")
        code += ("assert(isequal(cellstr(string('x') + [1 2]), {'x1', 'x2'}));\n"
                 "assert(isequal(cellstr(compose('%s', string({'a', 'b'}))), {'a', 'b'}));\n")
        package = str(self.package).replace("'", "''")
        helpers = str(ROOT / 'octave' / 'compat').replace("'", "''")
        result = subprocess.run([exe, '--no-gui', '--quiet', '--no-init-file', '--no-site-file',
                                 '--no-history', '--eval', f"addpath('{package}'); addpath('{helpers}');" + code],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_octave_string_first_behaviour_is_preserved(self):
        exe = shutil.which('octave-cli')
        if not exe:
            self.skipTest('octave-cli is unavailable')
        # Compare real processes before/after; semantic fixes have separate cases.
        code = """
warning ('off', 'Octave:shadowed-function');
s = string ({'ABC', 'banana'; '', 'a.b'});
r = {contains(s, 'b', 'IgnoreCase', true), count(s, 'a'), ...
     cellstr(replace(s, 'b', 'X')), cellstr(erase(s, 'b')), ...
     cellstr(compose(string('%d-%s'), [1; 2], string('a'))), ...
     cellstr(split(string('a,,b,'), ',')), ...
     cellstr(join(string({'a', 'b'}), '-')), ...
     cellstr(string({'a', 'b'}) + string('x'))};
s = string ({'abc', NaN});
r = [r, {contains(s, 'x'), contains(string('abc'), string(NaN)), ...
         contains(string('abc'), string({'x', NaN})), count(s, 'x'), ismissing(replace(s, 'a', 'X')), ...
         ismissing(erase(s, 'a')), ismissing(s + string('x'))}];
disp (jsonencode(r));
"""
        def run():
            quoted = str(self.package).replace("'", "''")
            helpers = str(ROOT / 'octave' / 'compat').replace("'", "''")
            result = subprocess.run([exe, '--no-gui', '--quiet', '--no-init-file', '--no-site-file',
                                     '--no-history', '--eval', f"addpath('{quoted}'); addpath('{helpers}');" + code],
                                    capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr)
            return result.stdout
        before = run()
        self.apply()
        self.assertEqual(run(), before)


if __name__ == '__main__':
    unittest.main(verbosity=2)
