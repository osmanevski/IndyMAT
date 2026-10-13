"""Schema 2 creation transactions; all fixtures stay in private fleet scratch."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from scripts import patch_packages as patcher

SCRATCH = Path.home() / '.cache' / 'indymat-fleet' / 'dalga26'


def snapshot(folder):
    return {p.relative_to(folder).as_posix(): (p.read_bytes(), p.stat().st_mode)
            for p in folder.rglob('*') if p.is_file() and not p.is_symlink()}


class Create26Tests(unittest.TestCase):
    def setUp(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(prefix='patch-create26-', dir=SCRATCH)
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.project = self.base / 'project'
        self.package = self.project / '.packages' / 'control-4.2.3'
        self.package.mkdir(parents=True)
        self.folder = self.base / 'patches' / self.package.name
        self.folder.mkdir(parents=True)
        self.old = b'old\n'
        self.new = b'new\n'
        self.source = b'% inert transaction fixture\n'
        (self.package / 'edit.m').write_bytes(self.old)
        (self.package / 'edit.m').chmod(0o640)
        (self.folder / 'edit.patch').write_text('--- a/edit.m\n+++ b/edit.m\n@@ -1 +1 @@\n-old\n+new\n')
        (self.folder / 'new.m').write_bytes(self.source)
        self.manifest = {'schema': 2, 'package': self.package.name,
                         'files': [{'path': 'edit.m', 'pristine_sha256': patcher.digest(self.old),
                                    'patched_sha256': patcher.digest(self.new)}],
                         'patches': ['edit.patch'],
                         'creates': [{'path': '@zpk/zpk.m', 'source': 'new.m',
                                      'sha256': patcher.digest(self.source)}]}
        self.save_manifest()

    def save_manifest(self):
        (self.folder / 'manifest.json').write_text(json.dumps(self.manifest))

    def apply(self, **kwargs):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            states = patcher.patch_package(self.project, self.folder, **kwargs)
        return states, out.getvalue()

    def test_missing_created_revert_and_repeat(self):
        before = snapshot(self.package)
        self.assertEqual(self.apply(check=True)[0]['@zpk/zpk.m'], 'missing')
        self.assertEqual(snapshot(self.package), before)
        self.apply()
        after = snapshot(self.package)
        self.assertEqual(self.apply(check=True)[0]['@zpk/zpk.m'], 'created')
        self.apply()
        self.assertEqual(snapshot(self.package), after)
        self.apply(revert=True)
        self.assertFalse((self.package / '@zpk/zpk.m').exists())
        self.assertEqual((self.package / 'edit.m').read_bytes(), self.old)
        reverted = snapshot(self.package)
        self.apply(revert=True)
        self.assertEqual(snapshot(self.package), reverted)
        self.apply()
        self.assertEqual(snapshot(self.package), after)

    def test_preexisting_identical_is_never_owned(self):
        target = self.package / '@zpk/zpk.m'
        target.parent.mkdir()
        target.write_bytes(self.source)
        target.chmod(0o644)
        self.assertEqual(self.apply(check=True)[0]['@zpk/zpk.m'], 'present')
        self.apply()
        self.assertFalse((self.package / patcher.STATE / 'created.json').exists())
        self.apply(revert=True)
        self.assertEqual(target.read_bytes(), self.source)
        self.assertEqual(target.stat().st_mode & 0o777, 0o644)

    def test_modified_creation_reports_check_and_refuses_mutations(self):
        for existing in (False, True):
            if existing:
                self.apply()
            target = self.package / '@zpk/zpk.m'
            target.parent.mkdir(exist_ok=True)
            target.write_bytes(b'local edit\n')
            before = snapshot(self.package)
            self.assertEqual(self.apply(check=True)[0]['@zpk/zpk.m'], 'modified')
            with mock.patch.object(patcher, 'PATCH_ROOT', self.folder.parent):
                with contextlib.redirect_stdout(io.StringIO()) as output:
                    self.assertEqual(patcher.main(['--project', str(self.project), '--package',
                                                   self.package.name, '--check']), 1)
                self.assertIn('modified (check)', output.getvalue())
            for kwargs in ({}, {'revert': True}):
                with self.assertRaisesRegex(patcher.PatchError, 'modified creation'):
                    self.apply(**kwargs)
                self.assertEqual(snapshot(self.package), before)
            target.unlink()

    def test_failed_apply_and_revert_restore_all_bytes_modes_and_metadata(self):
        for revert in (False, True):
            if revert:
                self.apply()
            before = snapshot(self.package)
            directories = {str(p) for p in self.package.rglob('*') if p.is_dir()}
            atomic = patcher.atomic
            failed = False
            def fail_registry(path, *args):
                nonlocal failed
                if path.name == 'registry.json' and not failed:
                    failed = True
                    raise OSError('injected registry failure')
                return atomic(path, *args)
            with mock.patch.object(patcher, 'atomic', side_effect=fail_registry):
                with self.assertRaisesRegex(OSError, 'injected'):
                    self.apply(revert=revert)
            self.assertEqual(snapshot(self.package), before)
            self.assertEqual({str(p) for p in self.package.rglob('*') if p.is_dir()}, directories)

    def test_schema1_round_trip_upgrade_preserved(self):
        self.manifest.pop('creates')
        self.manifest['schema'] = 1
        self.save_manifest()
        self.apply()
        registry = self.package / patcher.STATE / 'registry.json'
        older = dict(self.manifest, files=[dict(self.manifest['files'][0],
                     patched_sha256=patcher.digest(b'older\n'))])
        registry.write_text(json.dumps(older))
        (self.package / 'edit.m').write_bytes(b'older\n')
        self.assertEqual(self.apply()[0]['edit.m'], 'outdated')
        self.apply(revert=True)
        self.assertEqual((self.package / 'edit.m').read_bytes(), self.old)

    def test_owned_creation_upgrade_and_dropped_path_refusal(self):
        self.apply()
        (self.folder / 'new.m').write_bytes(b'v2\n')
        self.manifest['creates'][0]['sha256'] = patcher.digest(b'v2\n')
        self.save_manifest()
        self.assertEqual(self.apply()[0]['@zpk/zpk.m'], 'outdated')
        self.assertEqual(self.apply(check=True)[0]['@zpk/zpk.m'], 'created')
        self.manifest['creates'] = []
        self.save_manifest()
        before = snapshot(self.package)
        with self.assertRaisesRegex(patcher.PatchError, 'absent from manifest'):
            self.apply()
        self.assertEqual(snapshot(self.package), before)

    def test_creates_only_supported(self):
        self.manifest['files'] = []
        self.manifest['patches'] = []
        self.save_manifest()
        self.apply()
        self.apply(revert=True)
        self.assertFalse((self.package / '@zpk/zpk.m').exists())

    def test_creation_sources_hash_paths_and_symlinks_guarded(self):
        before = snapshot(self.package)
        for name in ('../escape', '/escape', 'a/../../escape', '.',
                     '.indymat-patches/owned', 'edit.m/child', 'a\\b'):
            self.manifest['creates'][0]['path'] = name
            self.save_manifest()
            with self.assertRaises(patcher.PatchError):
                self.apply()
            self.assertEqual(snapshot(self.package), before)
        self.manifest['creates'][0]['path'] = '@zpk/zpk.m'
        self.manifest['creates'][0]['source'] = '../escape'
        self.save_manifest()
        with self.assertRaises(patcher.PatchError):
            self.apply()
        self.manifest['creates'][0]['source'] = 'new.m'
        self.save_manifest()
        (self.folder / 'new.m').write_bytes(b'changed\n')
        with self.assertRaisesRegex(patcher.PatchError, 'source hash'):
            self.apply()
        (self.folder / 'new.m').write_bytes(self.source)
        outside = self.base / 'outside'
        outside.mkdir()
        (self.package / '@zpk').symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(patcher.PatchError, 'symlink'):
            self.apply()
        self.assertEqual(list(outside.iterdir()), [])
        self.assertEqual(snapshot(self.package), before)

    def test_source_target_and_ledger_symlinks_refused(self):
        outside = self.base / 'outside.m'
        outside.write_bytes(self.source)
        source = self.folder / 'new.m'
        source.unlink()
        source.symlink_to(outside)
        with self.assertRaisesRegex(patcher.PatchError, 'symlink'):
            self.apply()
        source.unlink()
        source.write_bytes(self.source)
        target = self.package / '@zpk/zpk.m'
        target.parent.mkdir()
        target.symlink_to(outside)
        with self.assertRaisesRegex(patcher.PatchError, 'symlink'):
            self.apply()
        target.unlink()
        self.apply()
        ledger = self.package / patcher.STATE / 'created.json'
        ledger.unlink()
        ledger.symlink_to(outside)
        before = snapshot(self.package)
        with self.assertRaisesRegex(patcher.PatchError, 'symlink'):
            self.apply(revert=True)
        self.assertEqual(snapshot(self.package), before)
        self.assertEqual(outside.read_bytes(), self.source)

    def test_owned_upgrade_already_at_new_bytes_updates_ledger(self):
        self.apply()
        newer = b'updated by approved package reinstall\n'
        (self.folder / 'new.m').write_bytes(newer)
        (self.package / '@zpk/zpk.m').write_bytes(newer)
        self.manifest['creates'][0]['sha256'] = patcher.digest(newer)
        self.save_manifest()
        self.apply()
        self.assertEqual(self.apply(check=True)[0]['@zpk/zpk.m'], 'created')
        self.apply(revert=True)
        self.assertFalse((self.package / '@zpk/zpk.m').exists())

    def test_ledger_cannot_authorize_unregistered_deletion(self):
        self.apply()
        ledger = self.package / patcher.STATE / 'created.json'
        original = ledger.read_bytes()
        for creates in ({'../escape': '0' * 64}, {'@zpk/zpk.m': 'bad'},
                        {'@zpk/zpk.m': '0' * 64}, {'edit.m': patcher.digest(self.new)}):
            ledger.write_text(json.dumps({'schema': 1, 'package': self.package.name,
                                         'creates': creates}))
            before = snapshot(self.package)
            with self.assertRaises(patcher.PatchError):
                self.apply(revert=True)
            self.assertEqual(snapshot(self.package), before)
        ledger.write_bytes(original)
        registry = self.package / patcher.STATE / 'registry.json'
        registry.unlink()
        before = snapshot(self.package)
        with self.assertRaisesRegex(patcher.PatchError, 'registered manifest'):
            self.apply(revert=True)
        self.assertEqual(snapshot(self.package), before)

    def test_changed_ledger_during_preparation_refuses_every_write(self):
        self.apply()
        ledger = self.package / patcher.STATE / 'created.json'
        transaction = patcher.transaction
        before = snapshot(self.package)
        def change_ledger(package, changes, expected):
            ledger.write_bytes(ledger.read_bytes() + b'\n')
            return transaction(package, changes, expected)
        with mock.patch.object(patcher, 'transaction', side_effect=change_ledger):
            with self.assertRaisesRegex(patcher.PatchError, 'changed during'):
                self.apply(revert=True)
        after = snapshot(self.package)
        for name in before:
            if name != patcher.STATE + '/created.json':
                self.assertEqual(after[name], before[name])


if __name__ == '__main__':
    unittest.main(verbosity=2)
