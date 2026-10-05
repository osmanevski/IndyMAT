import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from backend.files import Workspace
from backend.workspace_actions import MatTarget, atomic_exchange


class WorkspaceAtomicOverwriteTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        self.workspace = Workspace(self.root, self.root)
        self.path = self.root / 'atomic.mat'
        self.path.write_bytes(b'old-complete-data')

    def tearDown(self):
        self.temporary.cleanup()

    def target(self):
        return MatTarget(self.workspace, str(self.path))

    def test_recovery_rename_happens_after_atomic_target_exchange(self):
        target = self.target()
        original_rename = __import__('os').rename
        observed = []

        def observe(source, destination, **kwargs):
            observed.append((self.path.exists(), self.path.read_bytes()))
            return original_rename(source, destination, **kwargs)

        try:
            with patch('backend.workspace_actions.os.rename', side_effect=observe):
                target.install(b'new-complete-data')
        finally:
            target.close()
        self.assertEqual(observed, [(True, b'new-complete-data')])
        self.assertEqual(self.path.read_bytes(), b'new-complete-data')
        self.assertEqual(list(self.root.glob('.mf-workspace-*')), [])

    def test_replacement_immediately_before_exchange_is_restored(self):
        target = self.target()
        raced = False

        def replace_then_exchange(directory_fd, first, second):
            nonlocal raced
            if not raced:
                raced = True
                self.path.write_bytes(b'concurrent-complete-data')
            atomic_exchange(directory_fd, first, second)

        try:
            with patch('backend.workspace_actions.atomic_exchange', side_effect=replace_then_exchange):
                with self.assertRaisesRegex(FileExistsError, 'üzerine yazılmadı'):
                    target.install(b'new-complete-data')
        finally:
            target.close()
        self.assertEqual(self.path.read_bytes(), b'concurrent-complete-data')
        self.assertEqual(list(self.root.glob('.mf-workspace-*')), [])

    def test_platform_without_exchange_refuses_and_keeps_old_target(self):
        target = self.target()
        try:
            with patch('backend.workspace_actions.atomic_exchange', side_effect=ValueError('atomik takas yok')):
                with self.assertRaisesRegex(ValueError, 'atomik takas yok'):
                    target.install(b'new-complete-data')
        finally:
            target.close()
        self.assertEqual(self.path.read_bytes(), b'old-complete-data')
        self.assertEqual(list(self.root.glob('.mf-workspace-*')), [])

    def failed_rollback(self, *, rename_fails=False, fsync_fails=False):
        target = self.target()
        attempts = 0
        displaced_identity = None
        saved_name = None
        recovery_synced = False
        original_rename = os.rename
        original_fsync = os.fsync
        displaced = b'concurrent-complete-data'
        removed = self.root / 'externally-moved.mat'

        def race(directory_fd, first, second):
            nonlocal attempts, displaced_identity, saved_name
            attempts += 1
            if attempts == 1:
                incoming = self.root / 'incoming.mat'
                incoming.write_bytes(displaced)
                incoming.replace(self.path)
                displaced_identity = self.path.stat().st_ino
                saved_name = first
            elif attempts == 2:
                # The initial exchange succeeded, but the new target disappears
                # before either rollback. Both rollback calls use the real syscall.
                self.path.replace(removed)
            atomic_exchange(directory_fd, first, second)

        def recovery_rename(source, destination, **kwargs):
            if rename_fails:
                raise PermissionError('injected recovery rename failure')
            return original_rename(source, destination, **kwargs)

        def sync(directory_fd):
            nonlocal recovery_synced
            if attempts == 3 and directory_fd == target.fd:
                recovery_synced = True
                if fsync_fails:
                    raise OSError('injected directory fsync failure')
            return original_fsync(directory_fd)

        try:
            with patch('backend.workspace_actions.atomic_exchange', side_effect=race), \
                    patch('backend.workspace_actions.os.rename', side_effect=recovery_rename), \
                    patch('backend.workspace_actions.os.fsync', side_effect=sync):
                with self.assertRaises(OSError) as raised:
                    target.install(b'new-complete-data')
            survivors = list(self.root.glob('.mf-workspace-*'))
            self.assertEqual(len(survivors), 1, 'rollback deleted the only displaced copy')
            preserved = survivors[0]
            self.assertEqual(preserved.read_bytes(), displaced)
            self.assertEqual(preserved.stat().st_ino, displaced_identity)
            self.assertIn('kurtarma dosyası: ' + str(preserved), str(raised.exception))
            self.assertTrue(recovery_synced, 'recovery directory must be fsynced')
            self.assertEqual(attempts, 3)
            self.assertFalse(self.path.exists())
            self.assertEqual(removed.read_bytes(), b'new-complete-data')
            if rename_fails:
                self.assertEqual(preserved.name, saved_name)
            else:
                self.assertTrue(preserved.name.startswith('.mf-workspace-recovery-'))
            if fsync_fails:
                self.assertIn('fsync', str(raised.exception))
        finally:
            target.close()

    def test_failed_rollback_retains_displaced_inode_and_reports_recovery_path(self):
        self.failed_rollback()

    def test_failed_recovery_rename_keeps_temporary_and_reports_its_path(self):
        self.failed_rollback(rename_fails=True)

    def test_failed_recovery_fsync_still_reports_preserved_file(self):
        self.failed_rollback(fsync_fails=True)



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':
    unittest.main(verbosity=2)
