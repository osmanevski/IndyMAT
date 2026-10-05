"""Descriptor-bound Current Folder operations; caller serializes kernel -> files."""
from backend.i18n import tr
import ctypes
import errno
import hashlib
import os
import re
import secrets
import stat
import sys
import unicodedata
from contextlib import contextmanager
from pathlib import Path


RENAME_EXCL = 0x00000004
RENAME_NOFOLLOW_ANY = 0x00000010
TRAVERSAL_MAX_ITEMS = 10000
TRAVERSAL_MAX_DEPTH = 64
TRAVERSAL_MAX_OPEN_FDS = 128


def _trash_protection_unavailable():
    return tr('This platform cannot guarantee that the Trash path will not follow symbolic links. Checks before and after the operation cannot detect a temporary path change in between; Move to Trash is unavailable, and the item was not moved.')


def exclusive_rename(source_fd, source, target_fd, target, nofollow_any=False):
    """Atomic no-replace rename. Never emulate it with a replaceable placeholder."""
    libc = ctypes.CDLL(None, use_errno=True)
    if sys.platform == 'darwin':
        function = getattr(libc, 'renameatx_np', None)
        flag = RENAME_EXCL | (RENAME_NOFOLLOW_ANY if nofollow_any else 0)
    elif sys.platform.startswith('linux'):
        function = getattr(libc, 'renameat2', None)
        flag = 1  # RENAME_NOREPLACE
    else:
        function = None
    if nofollow_any and (sys.platform != 'darwin' or function is None):
        raise ValueError(_trash_protection_unavailable())
    if function is None:
        raise ValueError(tr('Safe moves without overwriting are unavailable on this system.'))
    function.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    function.restype = ctypes.c_int
    result = function(source_fd, os.fsencode(source), target_fd, os.fsencode(target), flag)
    if result:
        code = ctypes.get_errno()
        if nofollow_any and code in (errno.EINVAL, errno.ENOTSUP, errno.EOPNOTSUPP, errno.ENOSYS):
            # Never retry without protection: a swap/rename/restore between
            # the lstat checks leaves matching inodes while escaping Trash.
            # Without a pinned parent, unsupported protection must fail closed.
            raise ValueError(_trash_protection_unavailable())
        raise OSError(code, os.strerror(code), target)


class FileOperations:
    def __init__(self, workspace):
        self.w = workspace

    def path(self, value):
        if not isinstance(value, str) or not value or any(unicodedata.category(c).startswith('C') for c in value):
            raise ValueError(tr('Invalid file path.'))
        raw = Path(value).expanduser()
        if '..' in raw.parts:
            raise PermissionError(tr('The file path cannot traverse a parent folder.'))
        path = raw if raw.is_absolute() else self.w.current / raw
        if not self.w._inside(path):
            raise PermissionError(tr('The file is outside the allowed root.'))
        return path

    def name(self, value):
        if (not isinstance(value, str) or not value.strip() or value.startswith('.')
                or value in self.w.IGNORED
                or value in ('.', '..') or '/' in value or '\\' in value
                or any(unicodedata.category(c).startswith('C') for c in value)
                or len(value.encode('utf-8')) > 255):
            raise ValueError(tr('Invalid name: do not use hidden or reserved names, control characters, or path separators (255 bytes maximum).'))
        return value

    @staticmethod
    def breakpoint_name(value):
        """Reject a rename that would turn an existing breakpoint into a ghost."""
        if not isinstance(value, str) or Path(value).suffix.lower() != '.m' or not re.fullmatch(r'[A-Za-z]\w*', Path(value).stem):
            raise ValueError(tr('A file with breakpoints must remain a .m file with a valid Octave function name.'))
        return value

    @contextmanager
    def directory(self, path):
        # Walk even the root's parents. Every subsequent lookup is relative to
        # an already open directory, so replacing a path cannot redirect it.
        fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
        try:
            for part in path.parts[1:]:
                try:
                    next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                except OSError as exc:
                    if exc.errno in (errno.ELOOP, errno.ENOTDIR):
                        raise PermissionError(tr('Symbolic links are not allowed in the file path.')) from exc
                    raise
                os.close(fd)
                fd = next_fd
            yield fd
        finally:
            os.close(fd)

    @staticmethod
    def info(fd, name):
        value = os.stat(name, dir_fd=fd, follow_symlinks=False)
        if not (stat.S_ISREG(value.st_mode) or stat.S_ISDIR(value.st_mode)):
            raise PermissionError(tr('Symbolic links and special files cannot be processed.'))
        return value

    @staticmethod
    def digest(fd, name):
        source = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
        with os.fdopen(source, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode):
                raise PermissionError(tr('A regular file is required.'))
            digest = hashlib.sha256()
            for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                digest.update(chunk)
            after = os.fstat(stream.fileno())
        if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise FileExistsError(tr('The file changed while being read; try again.'))
        return digest.hexdigest()

    def count(self, fd, name, budget):
        info = self.info(fd, name)
        if not stat.S_ISDIR(info.st_mode):
            return
        child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
        stack = []
        try:
            stack.append([child, os.scandir(child), 1])
            child = None
            while stack:
                current, entries, depth = stack[-1]
                try:
                    entry = next(entries)
                except StopIteration:
                    entries.close()
                    os.close(current)
                    stack.pop()
                    continue
                budget[0] += 1
                if budget[0] >= TRAVERSAL_MAX_ITEMS:
                    return
                # Symlinks and special files are counted, never traversed.
                if not entry.is_dir(follow_symlinks=False):
                    continue
                next_depth = depth + 1
                if next_depth > TRAVERSAL_MAX_DEPTH:
                    raise ValueError(tr('A folder can contain at most {depth} levels; no operation was performed.', depth=TRAVERSAL_MAX_DEPTH))
                if (len(stack) + 1) * 2 > TRAVERSAL_MAX_OPEN_FDS:
                    raise ValueError(tr('The folder scan exceeds the safe open-file limit; no operation was performed.'))
                nested = os.open(entry.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=current)
                try:
                    nested_entries = os.scandir(nested)
                except Exception:
                    os.close(nested)
                    raise
                stack.append([nested, nested_entries, next_depth])
        finally:
            if child is not None:
                os.close(child)
            for current, entries, _depth in stack:
                entries.close()
                os.close(current)

    @staticmethod
    def _copy_file(source_fd, source, target_fd, target, info):
        incoming = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=source_fd)
        with os.fdopen(incoming, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode):
                raise PermissionError(tr('A regular file is required.'))
            outgoing = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, stat.S_IMODE(info.st_mode), dir_fd=target_fd)
            with os.fdopen(outgoing, 'wb') as out:
                for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                    out.write(chunk)
            after = os.fstat(stream.fileno())
            if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
                raise FileExistsError(tr('The file changed while being copied.'))

    def copy(self, source_fd, source, target_fd, target):
        info = self.info(source_fd, source)
        if not stat.S_ISDIR(info.st_mode):
            self._copy_file(source_fd, source, target_fd, target, info)
            return
        incoming = os.open(source, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=source_fd)
        outgoing = None
        stack = []
        try:
            os.mkdir(target, 0o755, dir_fd=target_fd)
            outgoing = os.open(target, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=target_fd)
            stack.append([incoming, outgoing, os.scandir(incoming), 1])
            incoming = outgoing = None
            copied = 0
            while stack:
                source_parent, target_parent, entries, depth = stack[-1]
                try:
                    entry = next(entries)
                except StopIteration:
                    entries.close()
                    os.close(source_parent)
                    os.close(target_parent)
                    stack.pop()
                    continue
                copied += 1
                if copied > TRAVERSAL_MAX_ITEMS:
                    raise ValueError(tr('The folder copy limit is {count} items; no operation was performed.', count=TRAVERSAL_MAX_ITEMS))
                value = self.info(source_parent, entry.name)
                if not stat.S_ISDIR(value.st_mode):
                    self._copy_file(source_parent, entry.name, target_parent, entry.name, value)
                    continue
                next_depth = depth + 1
                if next_depth > TRAVERSAL_MAX_DEPTH:
                    raise ValueError(tr('A folder can contain at most {depth} levels; no operation was performed.', depth=TRAVERSAL_MAX_DEPTH))
                if (len(stack) + 1) * 3 > TRAVERSAL_MAX_OPEN_FDS:
                    raise ValueError(tr('The folder copy exceeds the safe open-file limit; no operation was performed.'))
                nested_source = os.open(entry.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=source_parent)
                nested_target = None
                try:
                    os.mkdir(entry.name, 0o755, dir_fd=target_parent)
                    nested_target = os.open(entry.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=target_parent)
                    nested_entries = os.scandir(nested_source)
                except Exception:
                    os.close(nested_source)
                    if nested_target is not None:
                        os.close(nested_target)
                    raise
                stack.append([nested_source, nested_target, nested_entries, next_depth])
        finally:
            if incoming is not None:
                os.close(incoming)
            if outgoing is not None:
                os.close(outgoing)
            for source_parent, target_parent, entries, _depth in stack:
                entries.close()
                os.close(source_parent)
                os.close(target_parent)

    def cleanup(self, fd, name):
        """Only removes our private, unpublished copy stage."""
        info = os.stat(name, dir_fd=fd, follow_symlinks=False)
        if stat.S_ISDIR(info.st_mode):
            child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            try:
                for entry in os.listdir(child):
                    self.cleanup(child, entry)
            finally:
                os.close(child)
            os.rmdir(name, dir_fd=fd)
        else:
            os.unlink(name, dir_fd=fd)

    def rename(self, source_fd, source, target_fd, target, nofollow_any=False):
        try:
            if nofollow_any:
                exclusive_rename(source_fd, source, target_fd, target, True)
            else:
                exclusive_rename(source_fd, source, target_fd, target)
        except OSError as exc:
            if exc.errno == errno.EXDEV:
                raise ValueError(tr('Moving to a different disk or volume is not supported.')) from exc
            if exc.errno in (errno.EEXIST, errno.ENOTEMPTY):
                raise FileExistsError(tr('“{name}” already exists; it was not overwritten.', name=target)) from exc
            raise

    def trash_info(self, home_fd, source_info):
        try:
            trash = os.stat('.Trash', dir_fd=home_fd, follow_symlinks=False)
        except OSError as exc:
            raise ValueError(tr('Trash is unavailable; the item was not deleted.')) from exc
        if not stat.S_ISDIR(trash.st_mode) or trash.st_uid != os.getuid():
            raise ValueError(tr('Trash is unavailable; its ownership or type is unsafe, and the item was not deleted.'))
        if source_info.st_dev != trash.st_dev:
            raise ValueError(tr('An item on an external volume cannot be moved to Trash.'))
        return trash

    def raced_trash_location(self, home_fd, candidate, source_info, trash_info):
        """Return only an inode-verified recovery path, observed at check time.

        A concurrent writer can move it again after this check; this is a
        diagnostic location, not a pinned recovery capability.
        """
        try:
            current = os.stat('.Trash', dir_fd=home_fd, follow_symlinks=False)
            if stat.S_ISLNK(current.st_mode):
                link = Path(os.readlink('.Trash', dir_fd=home_fd))
                folder = link if link.is_absolute() else self.w.home / link
                location = folder.resolve(strict=True) / candidate
                leaf = os.stat(location, follow_symlinks=False)
                if (leaf.st_dev, leaf.st_ino) == (source_info.st_dev, source_info.st_ino):
                    return str(location)
            else:
                leaf = os.stat('.Trash/' + candidate, dir_fd=home_fd, follow_symlinks=False)
                if (leaf.st_dev, leaf.st_ino) == (source_info.st_dev, source_info.st_ino):
                    return str(self.w.home / '.Trash' / candidate)
        except (OSError, RuntimeError):
            pass
        # A common directory swap renames the checked Trash within home. Find
        # that same inode without traversing arbitrary descendants.
        try:
            for name in os.listdir(home_fd):
                try:
                    value = os.stat(name, dir_fd=home_fd, follow_symlinks=False)
                    if not stat.S_ISDIR(value.st_mode) or (value.st_dev, value.st_ino) != (trash_info.st_dev, trash_info.st_ino):
                        continue
                    leaf = os.stat(name + '/' + candidate, dir_fd=home_fd, follow_symlinks=False)
                    if (leaf.st_dev, leaf.st_ino) == (source_info.st_dev, source_info.st_ino):
                        return str(self.w.home / name / candidate)
                except OSError:
                    continue
        except OSError:
            pass
        return None

    def expected(self, request, source):
        values = request.get('expected', [])
        if not isinstance(values, list) or len(values) > 1000:
            raise ValueError(tr('Invalid open-file hash list.'))
        for value in values:
            if not isinstance(value, dict) or set(value) != {'path', 'hash'}:
                raise ValueError(tr('Invalid open-file hash entry.'))
            path = self.path(value['path'])
            if not path.is_relative_to(source) or not isinstance(value['hash'], str):
                raise ValueError(tr('The hash path is outside the operation source.'))
            with self.directory(path.parent) as fd:
                if self.digest(fd, path.name) != value['hash']:
                    raise FileExistsError(tr('“{name}” changed externally. Reopen it or save with a different name.', name=path.name))

    def operate(self, request, session_folder=None):
        schemas = {'inspect': {'source'}, 'create_file': {'source', 'name'}, 'create_folder': {'source', 'name'},
                   'rename': {'source', 'name'}, 'duplicate': {'source', 'name'}, 'move': {'source', 'destination'}, 'trash': {'source'}}
        if not isinstance(request, dict) or not isinstance(request.get('operation'), str):
            raise ValueError(tr('Invalid file operation.'))
        operation = request['operation']
        allowed = schemas.get(operation)
        extra = {'expected'} if operation in ('rename', 'move') else set()
        if allowed is None or set(request) - extra != allowed | {'operation'}:
            raise ValueError(tr('Invalid file operation fields.'))
        source = self.path(request['source'])
        if operation.startswith('create_'):
            target = source / self.name(request['name'])
            with self.directory(source) as fd:
                try:
                    if operation == 'create_folder':
                        os.mkdir(target.name, dir_fd=fd)
                    else:
                        os.close(os.open(target.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644, dir_fd=fd))
                except FileExistsError as exc:
                    raise FileExistsError(tr('“{name}” already exists; it was not overwritten.', name=target.name)) from exc
            return dict(operation=operation, source=str(source), path=str(target), directory=operation == 'create_folder', hash=hashlib.sha256(b'').hexdigest() if operation == 'create_file' else None, item_count=0, count_truncated=False)
        if source in self.w.roots:
            raise PermissionError(tr('The allowed root cannot be moved or deleted.'))
        with self.directory(source.parent) as source_fd:
            info = self.info(source_fd, source.name)
            directory = stat.S_ISDIR(info.st_mode)
            budget = [0 if directory else 1]
            if directory:
                self.count(source_fd, source.name, budget)
            digest = None if directory else self.digest(source_fd, source.name)
            result = dict(operation=operation, source=str(source), path=str(source), directory=directory, hash=digest, item_count=budget[0], count_truncated=budget[0] >= 10000)
            if operation == 'inspect':
                return result
            if operation in ('rename', 'move', 'trash') and directory and session_folder and Path(session_folder).is_relative_to(source):
                raise PermissionError(tr("The Octave session's Current Folder or its parent cannot be moved."))
            if operation in ('rename', 'move'):
                self.expected(request, source)
            if operation in ('rename', 'duplicate'):
                target = source.parent / self.name(request['name'])
            elif operation == 'move':
                target = self.path(request['destination']) / source.name
                if directory and target.parent.is_relative_to(source):
                    raise ValueError(tr('A folder cannot be moved into itself.'))
            else:
                target = self.w.home / '.Trash' / source.name
            if operation != 'trash' and self.w._boundary(source) != self.w._boundary(target):
                raise PermissionError(tr('Moving between roots is not allowed.'))
            try:
                # macOS privacy controls refuse to open ~/.Trash as a directory, so the
                # Trash is addressed relative to the opened home directory instead.
                with self.directory(self.w.home if operation == 'trash' else target.parent) as target_fd:
                    if operation == 'duplicate':
                        stage = '.indymat-copy-' + secrets.token_hex(16)
                        os.mkdir(stage, 0o700, dir_fd=target_fd)
                        stage_fd = os.open(stage, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=target_fd)
                        try:
                            self.copy(source_fd, source.name, stage_fd, 'item')
                            self.rename(stage_fd, 'item', target_fd, target.name)
                        finally:
                            os.close(stage_fd)
                            self.cleanup(target_fd, stage)
                    elif operation == 'trash':
                        for number in range(1, 10001):
                            candidate = source.name if number == 1 else (source.stem + f' {number}' + source.suffix if not directory else source.name + f' {number}')
                            # ~/.Trash cannot be opened on current macOS under
                            # privacy controls, so its inode cannot be pinned.
                            # RENAME_NOFOLLOW_ANY blocks intermediate symlinks;
                            # unsupported kernels/filesystems are refused with
                            # no unprotected retry. The lstat checks cannot see
                            # a swap restored before the post-check. Even with
                            # NOFOLLOW_ANY, a real-directory swap/restore by a
                            # writer in home is not prevented without a pinned
                            # Trash fd. Detected changes are reported as failed;
                            # recovery paths are inode-verified observations.
                            trash_before = self.trash_info(target_fd, info)
                            try:
                                self.rename(source_fd, source.name, target_fd, '.Trash/' + candidate, nofollow_any=True)
                                try:
                                    trash_after = self.trash_info(target_fd, info)
                                except ValueError:
                                    trash_after = None
                                if trash_after is None or (trash_before.st_dev, trash_before.st_ino) != (trash_after.st_dev, trash_after.st_ino):
                                    location = self.raced_trash_location(target_fd, candidate, info, trash_before)
                                    if location:
                                        raise ValueError(tr('Trash changed during the operation. The item was moved, but the operation was marked as failed; verified new location: {path}', path=location))
                                    attempted = self.w.home / '.Trash' / candidate
                                    raise ValueError(tr('Trash changed during the operation. The item was moved, but its new location could not be verified; attempted destination: {path}', path=attempted))
                                target = target.parent / candidate
                                break
                            except FileExistsError:
                                continue
                            except OSError as exc:
                                if exc.errno == errno.ELOOP:
                                    raise ValueError(tr('Trash became a symbolic link during the operation; the item was not moved.')) from exc
                                raise
                        else:
                            raise FileExistsError(tr('Could not find a unique name in Trash.'))
                    else:
                        case_change = operation == 'rename' and source.name != target.name and source.name.casefold() == target.name.casefold()
                        if case_change:
                            try:
                                other = self.info(target_fd, target.name)
                            except FileNotFoundError:
                                other = None
                            case_change = other is not None and (info.st_dev, info.st_ino) == (other.st_dev, other.st_ino)
                        if case_change:
                            stage = '.indymat-rename-' + secrets.token_hex(16)
                            self.rename(source_fd, source.name, source_fd, stage)
                            try:
                                self.rename(source_fd, stage, target_fd, target.name)
                            except Exception as exc:
                                try:
                                    self.rename(source_fd, stage, source_fd, source.name)
                                except Exception:
                                    raise FileExistsError(tr('The item was preserved at {path}; moving it back conflicted.', path=source.parent / stage)) from exc
                                raise
                        else:
                            self.rename(source_fd, source.name, target_fd, target.name)
            except FileNotFoundError as exc:
                if operation == 'trash':
                    raise ValueError(tr('macOS Trash is unavailable; the item was not deleted.')) from exc
                raise
            except PermissionError as exc:
                if operation == 'trash':
                    raise PermissionError(tr('Could not move the item to macOS Trash; it was not deleted.')) from exc
                raise
            result['path'] = str(target)
            return result
