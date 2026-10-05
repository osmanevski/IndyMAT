"""MAT capabilities: kernel -> file lock order; Octave sees private stages only."""
from backend.i18n import tr
import ctypes
import hashlib
import math
import os
from pathlib import Path
import re
import secrets
import shutil
import stat
import sys
import tempfile

LIMIT = 200_000_000
VARIABLE_CLASSES = {'double','single','logical','char','int8','int16','int32','int64','uint8','uint16','uint32','uint64'}
VARIABLE_INTEGER_LIMITS = {'int8':(-128,127),'int16':(-32768,32767),'int32':(-2147483648,2147483647),'int64':(-9223372036854775808,9223372036854775807),'uint8':(0,255),'uint16':(0,65535),'uint32':(0,4294967295),'uint64':(0,18446744073709551615)}
VARIABLE_PAGE_ROWS = 100
VARIABLE_PAGE_COLUMNS = 30
VARIABLE_PAGE_CELLS = 3000


class MatOverwriteConflict(FileExistsError):
    """Stable conflict metadata alongside the unchanged legacy error text."""
    code = 'mat_overwrite_required'

    def __init__(self, message, expected_hash, approval):
        super().__init__(message)
        self.details = {'expected_hash': expected_hash, 'approval': approval}


def variable_name(value):
    if not isinstance(value, str) or len(value) > 63 or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', value) or value.startswith('__mf_'):
        raise ValueError(tr('Invalid or reserved variable name.'))
    return value


def variable_size(value, label='Size'):
    if not isinstance(value, list) or not 2 <= len(value) <= 32 or any(isinstance(item, bool) or not isinstance(item, int) or item < 0 or item > 2**53 - 1 for item in value):
        raise ValueError(tr('{label} is invalid.', label=tr('Size') if label=='Size' else label))
    return value


def variable_path(value):
    if not isinstance(value, list) or len(value) > 16:
        raise ValueError(tr('Invalid variable path.'))
    result = []
    for step in value:
        if not isinstance(step, dict) or step.get('kind') not in ('cell', 'element', 'field'):
            raise ValueError(tr('Invalid variable path.'))
        if step['kind'] == 'field':
            if set(step) != {'kind', 'name'} or not isinstance(step.get('name'), str) or not step['name'] or len(step['name'].encode('utf-8')) > 256 or '\0' in step['name']:
                raise ValueError(tr('Invalid struct field.'))
            result.append({'kind': 'field', 'name': step['name']})
        else:
            if set(step) != {'kind', 'indices'} or not isinstance(step.get('indices'), list) or not 2 <= len(step['indices']) <= 32:
                raise ValueError(tr('Invalid array index.'))
            indices = step['indices']
            if any(isinstance(index, bool) or not isinstance(index, int) or index < 1 or index > 2**53 - 1 for index in indices):
                raise ValueError(tr('Invalid array index.'))
            result.append({'kind': step['kind'], 'indices': indices})
    return result


def variable_slices(value):
    if not isinstance(value, list) or len(value) > 30 or any(isinstance(index, bool) or not isinstance(index, int) or index < 1 or index > 2**53 - 1 for index in value):
        raise ValueError(tr('Invalid slice indices.'))
    return value


def variable_scalar(class_name, value):
    if class_name == 'logical':
        if not isinstance(value, dict) or set(value) != {'type', 'value'} or value['type'] != 'logical' or not isinstance(value['value'], bool):
            raise ValueError(tr('The logical value must be true or false.'))
    elif class_name == 'char':
        if not isinstance(value, dict) or set(value) != {'type', 'value'} or value['type'] != 'text' or not isinstance(value['value'], str):
            raise ValueError(tr('Send the character row as complete UTF-8 text.'))
        try:
            encoded = value['value'].encode('utf-8', errors='strict')
        except UnicodeError as error:
            raise ValueError(tr('The text must be valid Unicode.')) from error
        if len(encoded) > 10000:
            raise ValueError(tr('The text editing limit is 10,000 UTF-8 bytes.'))
    elif class_name in VARIABLE_INTEGER_LIMITS:
        if not isinstance(value, dict) or set(value) != {'type', 'value'} or value['type'] != 'integer' or not isinstance(value['value'], str) or len(value['value']) > 21 or not re.fullmatch(r'-?(?:0|[1-9][0-9]*)', value['value']):
            raise ValueError(tr('Invalid integer value.'))
        number = int(value['value'])
        low, high = VARIABLE_INTEGER_LIMITS[class_name]
        if number < low or number > high:
            raise ValueError(tr('The {class_name} value exceeds the class range.', class_name=class_name))
    elif class_name in ('double', 'single'):
        if not isinstance(value, dict) or set(value) != {'type', 'value'} or value['type'] not in ('number', 'special'):
            raise ValueError(tr('Invalid floating-point value.'))
        if value['type'] == 'special':
            if value['value'] not in ('NaN', 'Inf', '-Inf', '-0'):
                raise ValueError(tr('The special floating-point value is invalid.'))
        elif isinstance(value['value'], bool) or not isinstance(value['value'], (int, float)) or not math.isfinite(value['value']):
            raise ValueError(tr('The numeric value must be finite.'))
        elif class_name == 'single' and abs(value['value']) > 3.4028234663852886e38:
            raise ValueError(tr('The single value exceeds the class range.'))
    else:
        raise ValueError(tr('This array class cannot be edited.'))
    return value


def variable_read_request(value):
    allowed = {'action','name','path','row','column','rows','columns','slices','epoch'}
    if not isinstance(value, dict) or set(value) - allowed or value.get('action') != 'read':
        raise ValueError(tr('Invalid variable read request.'))
    request = {'name': variable_name(value.get('name')), 'path': variable_path(value.get('path', [])), 'slices': variable_slices(value.get('slices', []))}
    for key, maximum in (('row', 2**53 - 1), ('column', 2**53 - 1), ('rows', VARIABLE_PAGE_ROWS), ('columns', VARIABLE_PAGE_COLUMNS)):
        number = value.get(key)
        if isinstance(number, bool) or not isinstance(number, int) or number < 1 or number > maximum:
            raise ValueError(tr('Invalid page bounds.'))
        request[key] = number
    if request['rows'] * request['columns'] > VARIABLE_PAGE_CELLS:
        raise ValueError(tr('The page exceeds the 3000-cell limit.'))
    epoch = value.get('epoch')
    if isinstance(epoch, bool) or not isinstance(epoch, int) or epoch < 1:
        raise ValueError(tr('Invalid session ID.'))
    request['epoch'] = epoch
    return request


def variable_write_request(value):
    allowed = {'action','name','path','row','column','height','width','slices','epoch','read_job','class','size','values'}
    if not isinstance(value, dict) or set(value) - allowed or value.get('action') != 'write':
        raise ValueError(tr('Invalid variable write request.'))
    class_name = value.get('class')
    if class_name not in VARIABLE_CLASSES:
        raise ValueError(tr('This array class cannot be edited.'))
    path = variable_path(value.get('path', []))
    if path:
        raise ValueError(tr('Cell and struct contents are read-only in this version.'))
    request = {'name': variable_name(value.get('name')), 'path': path, 'slices': variable_slices(value.get('slices', [])), 'class': class_name, 'size': variable_size(value.get('size'))}
    for key, maximum in (('row', 2**53 - 1), ('column', 2**53 - 1), ('height', VARIABLE_PAGE_ROWS), ('width', VARIABLE_PAGE_COLUMNS)):
        number = value.get(key)
        if isinstance(number, bool) or not isinstance(number, int) or number < 1 or number > maximum:
            raise ValueError(tr('Invalid write range.'))
        request[key] = number
    count = request['height'] * request['width']
    if count > VARIABLE_PAGE_CELLS or not isinstance(value.get('values'), list) or len(value['values']) != count:
        raise ValueError(tr('Write values do not match the rectangular range.'))
    if class_name == 'char' and (count != 1 or request['row'] != 1 or request['column'] != 1 or request['slices']):
        raise ValueError(tr('A character row can be edited only as complete text.'))
    request['values'] = [variable_scalar(class_name, item) for item in value['values']]
    epoch = value.get('epoch')
    read_job = value.get('read_job')
    if isinstance(epoch, bool) or not isinstance(epoch, int) or epoch < 1 or not isinstance(read_job, str) or not re.fullmatch(r'[a-f0-9]{32}', read_job):
        raise ValueError(tr('Invalid edit precondition.'))
    request['epoch'] = epoch
    request['read_job'] = read_job
    return request


def digest(data):
    return hashlib.sha256(data).hexdigest()


def atomic_exchange(directory_fd, first, second):
    """Atomically exchange two descriptor-relative names, or refuse safely."""
    libc = ctypes.CDLL(None, use_errno=True)
    if sys.platform == 'darwin':
        function = getattr(libc, 'renameatx_np', None)
        flag = 2  # RENAME_SWAP
    elif sys.platform.startswith('linux'):
        function = getattr(libc, 'renameat2', None)
        flag = 2  # RENAME_EXCHANGE
    else:
        function = None
    if function is None:
        raise ValueError(tr('Atomic MAT overwrites are unavailable on this system; save with a different name.'))
    function.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    function.restype = ctypes.c_int
    if function(directory_fd, os.fsencode(first), directory_fd, os.fsencode(second), flag):
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code), second)


class MatTarget:
    def __init__(self, workspace, name):
        if not isinstance(name, str) or not name or any(c in name for c in '\0\r\n'):
            raise ValueError(tr('Invalid MAT path.'))
        path = Path(name).expanduser()
        if not path.is_absolute():
            path = workspace.current / path
        if '..' in path.parts or not workspace._inside(path):
            raise PermissionError(tr('The MAT path is outside the allowed folder.'))
        if any(path.is_relative_to(root) for root in getattr(workspace, 'workspace_private_roots', ())):
            raise PermissionError(tr('Private MAT job files cannot be accessed.'))
        if path.suffix.lower() != '.mat':
            raise ValueError(tr('The MAT file must have a .mat extension.'))
        self.path = path
        self.fd, self.parents = self.open_parent()
        try:
            self.data, self.identity = self.read(self.fd, path.name, absent=True)
            self.hash = digest(self.data) if self.data is not None else None
        except BaseException:
            self.close()
            raise

    def open_parent(self):
        fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
        chain = []
        try:
            for part in self.path.parts[1:-1]:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                os.close(fd)
                fd = child
                info = os.fstat(fd)
                chain.append((info.st_dev, info.st_ino))
            return fd, tuple(chain)
        except OSError as error:
            os.close(fd)
            raise PermissionError(tr('The MAT path contains a link or a changed folder.')) from error

    @staticmethod
    def read(fd, name, absent=False):
        try:
            source = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
        except FileNotFoundError:
            if absent:
                return None, None
            raise
        except OSError as error:
            raise PermissionError(tr('The MAT file cannot be a symbolic link.')) from error
        with os.fdopen(source, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode):
                raise PermissionError(tr('The MAT file must be a regular file.'))
            if before.st_size > LIMIT:
                raise ValueError(tr('The MAT file exceeds the 200 MB limit.'))
            data = stream.read(LIMIT + 1)
            after = os.fstat(stream.fileno())
        if len(data) > LIMIT:
            raise ValueError(tr('The MAT file exceeds the 200 MB limit.'))
        if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise FileExistsError(tr('The MAT file changed while being read.'))
        return data, (before.st_dev, before.st_ino)

    def check(self):
        check_fd, parents = self.open_parent()
        os.close(check_fd)
        if parents != self.parents:
            raise FileExistsError(tr('The MAT folder changed after approval.'))
        data, identity = self.read(self.fd, self.path.name, absent=True)
        if identity != self.identity or (digest(data) if data is not None else None) != self.hash:
            raise FileExistsError(tr('The MAT file changed after approval.'))

    def install(self, data):
        self.check()
        temporary = '.mf-workspace-' + secrets.token_hex(16)
        recovery = '.mf-workspace-recovery-' + secrets.token_hex(16) + '.mat'
        exchanged = False
        recovered = False
        out = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=self.fd)
        try:
            with os.fdopen(out, 'wb') as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            staged, staged_identity = self.read(self.fd, temporary)
            if staged != data:
                raise FileExistsError(tr('The private MAT save copy changed.'))
            self.check()
            if self.identity is not None:
                atomic_exchange(self.fd, temporary, self.path.name)
                exchanged = True
                os.fsync(self.fd)
                preserved, identity = self.read(self.fd, temporary)
                if identity != self.identity or digest(preserved) != self.hash:
                    atomic_exchange(self.fd, temporary, self.path.name)
                    exchanged = False
                    os.fsync(self.fd)
                    raise FileExistsError(tr('The MAT file changed while saving; it was not overwritten.'))
                os.rename(temporary, recovery, src_dir_fd=self.fd, dst_dir_fd=self.fd)
                recovered = True
                exchanged = False
                os.fsync(self.fd)
            else:
                # Atomic no-replace: a concurrent creation is NEVER overwritten.
                os.link(temporary, self.path.name, src_dir_fd=self.fd, dst_dir_fd=self.fd, follow_symlinks=False)
                os.fsync(self.fd)
            written, _ = self.read(self.fd, self.path.name)
            check_fd, parents = self.open_parent()
            os.close(check_fd)
            current = os.stat(self.path.name, dir_fd=self.fd, follow_symlinks=False)
            if written != data or (current.st_dev, current.st_ino) != staged_identity or parents != self.parents:
                raise FileExistsError(tr('The MAT file or folder changed while saving.'))
            if recovered:
                preserved, identity = self.read(self.fd, recovery)
                if identity != self.identity or digest(preserved) != self.hash:
                    raise FileExistsError(tr('The preserved MAT file changed: {path}', path=str(self.path.parent / recovery)))
                os.unlink(recovery, dir_fd=self.fd)
                recovered = False
                os.fsync(self.fd)
        except FileExistsError as error:
            note = tr('; recovery file: {path}', path=str(self.path.parent / recovery)) if recovered else ''
            raise FileExistsError(str(error) + note) from error
        finally:
            if exchanged:
                try:
                    atomic_exchange(self.fd, temporary, self.path.name)
                except OSError as rollback_error:
                    # The staging name still owns the displaced user file.
                    # Retain it even if recovery rename or directory sync fails.
                    preserved_name = temporary
                    notes = []
                    try:
                        os.rename(temporary, recovery, src_dir_fd=self.fd, dst_dir_fd=self.fd)
                        preserved_name = recovery
                    except OSError as recovery_error:
                        notes.append(tr('could not rename the recovery file: {error}', error=str(recovery_error)))
                    try:
                        os.fsync(self.fd)
                    except OSError as sync_error:
                        notes.append(tr('could not sync the recovery directory to disk: {error}', error=str(sync_error)))
                    message = tr('Could not roll back the MAT file; recovery file: {path}', path=str(self.path.parent / preserved_name))
                    if notes:
                        message += '; ' + '; '.join(notes)
                    raise FileExistsError(message) from rollback_error
                else:
                    exchanged = False
                    os.fsync(self.fd)
            if not exchanged:
                try:
                    os.unlink(temporary, dir_fd=self.fd)
                except FileNotFoundError:
                    pass

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


class WorkspaceActions:
    def __init__(self, app):
        self.app = app
        if hasattr(app, 'runtime'):
            app.workspace.workspace_private_roots = (app.runtime / '.workspace-stages',)
        self.pending = {}
        self.approvals = {}
        self.inspections = {}

    def dispose(self, ticket):
        if ticket.get('target'):
            ticket['target'].close()
        if ticket.get('stage'):
            shutil.rmtree(ticket['stage'])

    def remember(self, collection, token, ticket):
        while len(collection) >= 32:
            self.dispose(collection.pop(next(iter(collection))))
        collection[token] = ticket

    def discard(self):
        for collection in (self.pending, self.approvals, self.inspections):
            for ticket in collection.values():
                self.dispose(ticket)
            collection.clear()

    def stage(self, ticket, data=None):
        private = self.app.kernel.runtime.parent / '.workspace-stages'
        private.mkdir(mode=0o700, exist_ok=True)
        self.app.workspace.workspace_private_roots = (private,)
        folder = Path(tempfile.mkdtemp(prefix='workspace-', dir=private))
        ticket['stage'] = folder
        ticket['file'] = folder / 'workspace.mat'
        if data is not None:
            with open(ticket['file'], 'xb') as stream:
                stream.write(data)
            os.chmod(ticket['file'], 0o400)
            info = ticket['file'].stat()
            ticket['stage_identity'] = (info.st_dev, info.st_ino)

    def stage_bytes(self, ticket):
        fd = os.open(ticket['stage'], os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            data, identity = MatTarget.read(fd, 'workspace.mat')
        finally:
            os.close(fd)
        if 'hash' in ticket and (digest(data) != ticket['hash'] or identity != ticket['stage_identity']):
            raise FileExistsError(tr('The private MAT copy changed; inspect it again.'))
        return data

    def submit(self, request, names_validator):
        k = self.app.kernel
        # Serialize preparation with jobs and reset, including other browser tabs.
        with k.lock:
            state = k.snapshot()
            if state['status'] == 'paused':
                raise ValueError(tr('Workspace is read-only while debugging is paused.'))
            if state['status'] != 'idle':
                raise ValueError(tr('Finish the running operation first.'))
            with self.app.file_lock:
                return self.prepare(request, names_validator)

    def prepare(self, request, names_validator):
        k = self.app.kernel
        action = request['action']
        ticket = None
        try:
            if action == 'load':
                if request.get('confirm') is not True:
                    raise ValueError(tr('Explicit confirmation is required to load the MAT file.'))
                ticket = self.inspections.pop(request.get('inspection', ''), None)
                if not ticket or ticket['generation'] != k.generation:
                    raise ValueError(tr('Invalid MAT inspection approval; inspect it again.'))
                if request.get('path') != ticket['path'] or request.get('hash') != ticket['hash']:
                    raise ValueError(tr('The MAT inspection approval does not match the file.'))
                self.stage_bytes(ticket)
                args = {'path': str(ticket['file']), 'inventory': ticket['inventory'], 'replacements': names_validator(request.get('replacements'))}
            elif action == 'load-inspect':
                target = MatTarget(self.app.workspace, request.get('path'))
                ticket = {'target': target, 'path': str(target.path), 'generation': k.generation}
                if target.data is None:
                    raise FileNotFoundError(tr('MAT file not found.'))
                target.check()
                ticket['hash'] = target.hash
                self.stage(ticket, target.data)
                target.close()
                ticket.pop('target')
                args = {'path': str(ticket['file'])}
            else:
                overwrite = request.get('overwrite')
                if not isinstance(overwrite, bool):
                    raise ValueError(tr('An overwrite choice is required.'))
                all_variables = request.get('all')
                if not isinstance(all_variables, bool):
                    raise ValueError(tr('The save scope must be specified.'))
                names = [] if all_variables else names_validator(request.get('names'))
                if not all_variables and not names:
                    raise ValueError(tr('No variables selected for saving.'))
                if overwrite:
                    if request.get('confirm_overwrite') is not True:
                        raise ValueError(tr('Explicit confirmation is required to overwrite.'))
                    ticket = self.approvals.pop(request.get('approval', ''), None)
                    if not ticket or ticket['generation'] != k.generation:
                        raise ValueError(tr('Invalid MAT overwrite approval.'))
                    target = ticket['target']
                    supplied = Path(request.get('path', '')).expanduser()
                    if not supplied.is_absolute():
                        supplied = self.app.workspace.current / supplied
                    if supplied != target.path or request.get('expected_hash') != target.hash:
                        raise ValueError(tr('The MAT overwrite approval does not match the file.'))
                    target.check()
                else:
                    target = MatTarget(self.app.workspace, request.get('path'))
                    ticket = {'target': target, 'path': str(target.path), 'generation': k.generation}
                    if target.data is not None:
                        target.data = None
                        token = secrets.token_hex(24)
                        self.remember(self.approvals, token, ticket)
                        ticket = None
                        raise MatOverwriteConflict(tr('The MAT file already exists; approval is required. [sha256:{hash}] [approval:{approval}]', hash=target.hash, approval=token), target.hash, token)
                self.stage(ticket)
                args = {'path': str(ticket['file']), 'all': all_variables, 'names': names}
            job = k.workspace_action('workspace-' + action, args)
            ticket['action'] = action
            self.pending[job] = ticket
            ticket = None
            return job
        finally:
            if ticket is not None:
                self.dispose(ticket)

    def finish(self, kernel):
        # Called under kernel.lock before idle is exposed or another job starts.
        state = kernel.state
        if state['status'] not in ('idle', 'dead'):
            return
        ticket = self.pending.pop(state.get('job'), None)
        if not ticket:
            return
        retain = False
        try:
            if state.get('error') or ticket['generation'] != kernel.generation:
                return
            result = state.get('workspace_action') or {}
            with self.app.file_lock:
                if ticket['action'] == 'save':
                    ticket['target'].install(self.stage_bytes(ticket))
                elif ticket['action'] == 'load-inspect':
                    token = secrets.token_hex(24)
                    ticket['inventory'] = result['variables']
                    self.stage_bytes(ticket)
                    self.remember(self.inspections, token, ticket)
                    result.update(hash=ticket['hash'], inspection=token)
                    retain = True
            result['path'] = ticket['path']
        except Exception as error:
            state['error'] = tr('Could not complete the MAT operation: {error}', error=str(error))
            state['workspace_action'] = None
        finally:
            if not retain:
                self.dispose(ticket)
