import ctypes
import errno
import os
import stat
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app import Handler
from backend.file_operations import FileOperations, exclusive_rename, RENAME_EXCL, RENAME_NOFOLLOW_ANY
from backend.files import Workspace


class FileOperationHardeningTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name).resolve()
        self.home = self.base / 'home'
        self.trash = self.home / '.Trash'
        self.trash.mkdir(parents=True)
        self.work = self.home / 'work'
        self.workspace = Workspace(self.work, self.home)
        self.files = FileOperations(self.workspace)

    def tearDown(self):
        self.tmp.cleanup()

    def operate(self, operation, **fields):
        return self.workspace.operate({'operation': operation, **fields}, str(self.work))

    def test_iterative_traversal_refuses_depth_without_recursion_error(self):
        root = self.work / 'deep'
        root.mkdir()
        current = root
        for _index in range(65):
            current = current / 'd'
            current.mkdir()
        for operation, fields in (
                ('inspect', {}),
                ('duplicate', {'name': 'deep-copy'}),
                ('rename', {'name': 'deep-renamed'}),
                ('move', {'destination': str(self.home)}),
                ('trash', {})):
            with self.subTest(operation=operation):
                with self.assertRaisesRegex(ValueError, 'en fazla 64 düzey'):
                    self.operate(operation, source=str(root), **fields)
                self.assertTrue(root.is_dir())
        self.assertFalse((self.work / 'deep-copy').exists())

    def test_copy_has_its_own_item_limit_and_cleans_private_stage(self):
        root = self.work / 'many'
        root.mkdir()
        for name in ('a', 'b', 'c'):
            (root / name).write_text(name)
        with patch('backend.file_operations.TRAVERSAL_MAX_ITEMS', 2):
            with self.assertRaisesRegex(ValueError, 'Klasör kopyalama sınırı 2'):
                self.operate('duplicate', source=str(root), name='many-copy')
        self.assertFalse((self.work / 'many-copy').exists())
        self.assertFalse(any(item.name.startswith('.indymat-copy-') for item in self.work.iterdir()))

    def test_count_enforces_a_separate_open_descriptor_limit(self):
        root = self.work / 'descriptor-depth'
        root.mkdir()
        current = root
        for _index in range(5):
            current = current / 'd'
            current.mkdir()
        with patch('backend.file_operations.TRAVERSAL_MAX_DEPTH', 100), patch('backend.file_operations.TRAVERSAL_MAX_OPEN_FDS', 3):
            with self.assertRaisesRegex(ValueError, 'açık dosya sınırını'):
                self.operate('inspect', source=str(root))

    def test_duplicate_enforces_copy_open_descriptor_limit_and_cleans_stage(self):
        root = self.work / 'copy-descriptor-depth'
        root.mkdir()
        current = root
        for _index in range(43):
            current = current / 'd'
            current.mkdir()
        with self.assertRaisesRegex(ValueError, 'Klasör kopyası güvenli açık dosya sınırını'):
            self.operate('duplicate', source=str(root), name='copy-descriptor-copy')
        self.assertFalse((self.work / 'copy-descriptor-copy').exists())
        self.assertFalse(any(item.name.startswith('.indymat-copy-') for item in self.work.iterdir()))

    @unittest.skipUnless(sys.platform == 'darwin', 'RENAME_NOFOLLOW_ANY is a macOS flag')
    def test_rename_nofollow_any_rejects_intermediate_symlink(self):
        source_folder = self.work / 'source'
        source_folder.mkdir()
        source = source_folder / 'item.m'
        source.write_text('x')
        outside = self.base / 'outside'
        outside.mkdir()
        (self.home / 'link').symlink_to(outside, target_is_directory=True)
        source_fd = os.open(source_folder, os.O_RDONLY | os.O_DIRECTORY)
        home_fd = os.open(self.home, os.O_RDONLY | os.O_DIRECTORY)
        try:
            with self.assertRaises(OSError) as caught:
                exclusive_rename(source_fd, source.name, home_fd, 'link/item.m', nofollow_any=True)
            self.assertEqual(caught.exception.errno, errno.ELOOP)
        finally:
            os.close(source_fd)
            os.close(home_fd)
        self.assertTrue(source.exists())
        self.assertFalse((outside / 'item.m').exists())

    def test_trash_swap_is_failure_and_reports_actual_destination(self):
        source = self.work / 'raced.m'
        source.write_text('content')
        outside = self.base / 'outside-trash'
        outside.mkdir()
        original = self.home / '.Trash-original'

        def raced(source_fd, source_name, target_fd, target_name, nofollow_any=False):
            self.assertTrue(nofollow_any)
            self.trash.rename(original)
            self.trash.symlink_to(outside, target_is_directory=True)
            os.rename(source_name, target_name, src_dir_fd=source_fd, dst_dir_fd=target_fd)

        with patch('backend.file_operations.exclusive_rename', side_effect=raced):
            with self.assertRaisesRegex(ValueError, str(outside / 'raced.m').replace('\\', '\\\\')):
                self.operate('trash', source=str(source))
        self.assertFalse(source.exists())
        self.assertEqual((outside / 'raced.m').read_text(), 'content')

    def test_unsupported_nofollow_never_retries_through_swap_and_restore(self):
        # Emulate an old kernel. An unprotected retry performs the review's
        # swap/rename/restore attack, leaving the before/after Trash inode equal.
        for code in sorted({errno.EINVAL, errno.ENOTSUP, errno.EOPNOTSUPP, errno.ENOSYS}):
            with self.subTest(errno=code):
                source = self.work / f'unsupported-{code}.m'
                source.write_text('source')
                outside = self.base / f'outside-{code}'
                outside.mkdir()
                held = self.home / f'trash-held-{code}'
                before = self.trash.stat()

                def old_kernel(source_fd, source_name, target_fd, target_name, flags):
                    if flags & RENAME_NOFOLLOW_ANY:
                        ctypes.set_errno(code)
                        return -1
                    self.trash.rename(held)
                    self.trash.symlink_to(outside, target_is_directory=True)
                    try:
                        os.rename(source_name, target_name, src_dir_fd=source_fd, dst_dir_fd=target_fd)
                    finally:
                        self.trash.unlink()
                        held.rename(self.trash)
                    return 0

                function = Mock(side_effect=old_kernel)
                libc = SimpleNamespace(renameatx_np=function)
                with patch('backend.file_operations.sys.platform', 'darwin'), patch('backend.file_operations.ctypes.CDLL', return_value=libc):
                    with self.assertRaisesRegex(ValueError, 'güvence altına alınamıyor.*öğe taşınmadı'):
                        self.operate('trash', source=str(source))
                self.assertEqual(function.call_count, 1)
                self.assertEqual(function.call_args.args[-1], RENAME_EXCL | RENAME_NOFOLLOW_ANY)
                self.assertEqual(source.read_text(), 'source')
                self.assertEqual(list(outside.iterdir()), [])
                after = self.trash.stat()
                self.assertEqual((before.st_dev, before.st_ino), (after.st_dev, after.st_ino))

    def test_nofollow_unavailable_platform_or_symbol_fails_before_syscall(self):
        for platform in ('darwin', 'linux', 'unsupported'):
            with self.subTest(platform=platform):
                source = self.work / f'platform-{platform}.m'
                source.write_text('source')
                linux_rename = Mock(return_value=0)
                libc = SimpleNamespace(renameat2=linux_rename)
                with patch('backend.file_operations.sys.platform', platform), patch('backend.file_operations.ctypes.CDLL', return_value=libc):
                    with self.assertRaisesRegex(ValueError, 'güvence altına alınamıyor.*öğe taşınmadı'):
                        self.operate('trash', source=str(source))
                linux_rename.assert_not_called()
                self.assertTrue(source.exists())

    def test_recovery_does_not_label_unrelated_symlink_target_as_verified(self):
        source = self.work / 'recovery.m'
        source.write_text('source')
        outside = self.base / 'recovery-outside'
        outside.mkdir()
        unrelated = outside / source.name
        unrelated.write_text('unrelated')
        moved = outside / 'actual-location.m'

        def raced(source_fd, source_name, target_fd, target_name, nofollow_any=False):
            os.rename(source_name, moved, src_dir_fd=source_fd)
            self.trash.rename(self.home / '.Trash-held')
            self.trash.symlink_to(outside, target_is_directory=True)

        with patch('backend.file_operations.exclusive_rename', side_effect=raced):
            with self.assertRaisesRegex(ValueError, 'yeni konumu doğrulanamadı') as caught:
                self.operate('trash', source=str(source))
        self.assertNotIn('doğrulanan yeni konum', str(caught.exception))
        self.assertNotIn(str(unrelated), str(caught.exception))
        self.assertEqual(unrelated.read_text(), 'unrelated')
        self.assertEqual(moved.read_text(), 'source')

    def test_symlink_recovery_requires_both_device_and_inode(self):
        outside = self.base / 'identity-outside'
        outside.mkdir()
        candidate = outside / 'candidate.m'
        candidate.write_text('candidate')
        identity = candidate.stat()
        trash_info = self.trash.stat()
        self.trash.rename(self.home / '.Trash-held')
        self.trash.symlink_to(outside, target_is_directory=True)
        with self.files.directory(self.home) as home_fd:
            for device, inode in ((identity.st_dev + 1, identity.st_ino), (identity.st_dev, identity.st_ino + 1)):
                with self.subTest(device=device, inode=inode):
                    source_info = SimpleNamespace(st_dev=device, st_ino=inode)
                    self.assertIsNone(self.files.raced_trash_location(home_fd, candidate.name, source_info, trash_info))
            self.assertEqual(self.files.raced_trash_location(home_fd, candidate.name, identity, trash_info), str(candidate))

    @unittest.skipUnless(sys.platform == 'darwin', 'RENAME_NOFOLLOW_ANY is a macOS flag')
    def test_trash_nofollow_race_fails_closed_with_source_intact(self):
        source = self.work / 'nofollow.m'
        source.write_text('content')
        outside = self.base / 'outside-nofollow'
        outside.mkdir()
        original = self.home / '.Trash-original'
        real_info = FileOperations.trash_info
        calls = 0

        def swap_after_check(instance, home_fd, source_info):
            nonlocal calls
            value = real_info(instance, home_fd, source_info)
            calls += 1
            if calls == 1:
                self.trash.rename(original)
                self.trash.symlink_to(outside, target_is_directory=True)
            return value

        with patch.object(FileOperations, 'trash_info', autospec=True, side_effect=swap_after_check):
            with self.assertRaisesRegex(ValueError, 'öğe taşınmadı'):
                self.operate('trash', source=str(source))
        self.assertTrue(source.exists())
        self.assertFalse((outside / source.name).exists())

    def test_trash_requires_owned_directory_on_same_device(self):
        source = self.work / 'owned.m'
        source.write_text('x')
        real_stat = os.stat

        def foreign_owner(path, *args, **kwargs):
            value = real_stat(path, *args, **kwargs)
            if path == '.Trash' and kwargs.get('follow_symlinks') is False:
                fields = list(value)
                fields[4] = os.getuid() + 1
                return os.stat_result(fields)
            return value

        with patch('backend.file_operations.os.stat', side_effect=foreign_owner):
            with self.assertRaisesRegex(ValueError, 'sahipliği'):
                self.operate('trash', source=str(source))
        self.assertTrue(source.exists())

    def handler(self, kernel):
        handler = object.__new__(Handler)
        handler.server = SimpleNamespace(app=SimpleNamespace(
            kernel=kernel,
            workspace=self.workspace,
            file_lock=threading.Lock(),
        ))
        handler.send = lambda status, data: (status, data)
        return handler

    def test_breakpoint_file_invalid_rename_is_rejected_before_move(self):
        source = self.work / 'valid_name.m'
        source.write_text('disp(1);')
        kernel = SimpleNamespace(
            lock=threading.RLock(),
            state={'status': 'idle', 'cwd': str(self.work)},
            breakpoints={str(source): {1: {'enabled': True, 'condition': ''}}},
        )
        kernel.snapshot = lambda: dict(kernel.state)
        kernel.relocate_file_breakpoints = lambda *_args: self.fail('relocation must not start')
        handler = self.handler(kernel)
        for name in ('bad-name.m', 'valid_name.txt'):
            with self.subTest(name=name):
                with self.assertRaisesRegex(ValueError, 'Kesme noktası bulunan'):
                    handler.post('/api/file-operation', {'operation': 'rename', 'source': str(source), 'name': name})
                self.assertTrue(source.exists())

    def test_post_move_breakpoint_failure_reports_real_destination(self):
        source = self.work / 'before.m'
        source.write_text('disp(1);')
        destination = self.work / 'after.m'
        kernel = SimpleNamespace(
            lock=threading.RLock(),
            state={'status': 'idle', 'cwd': str(self.work)},
            breakpoints={},
        )
        kernel.snapshot = lambda: dict(kernel.state)
        kernel.relocate_file_breakpoints = lambda *_args: (_ for _ in ()).throw(RuntimeError('injected breakpoint failure'))
        status, result = self.handler(kernel).post('/api/file-operation', {
            'operation': 'rename',
            'source': str(source),
            'name': destination.name,
        })
        self.assertEqual(status, 200)
        self.assertIn(str(destination), result['warning'])
        self.assertIn('injected breakpoint failure', result['warning'])
        self.assertFalse(source.exists())
        self.assertTrue(destination.exists())



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':
    unittest.main(verbosity=2)
