"""Experimental engine kernel: the Rust engine process behind the Kernel interface. No Octave.

The child is `indy-kernel` (motor/crates/indy-kernel); the protocol is frozen in
motor/docs/Kernel-Protokolu.md. Lock order: kernel.lock -> session condition.
The stdout reader thread never takes kernel.lock, so a request made while
holding it (submit, input, reset) cannot deadlock against event delivery.
"""
from __future__ import annotations
from backend.i18n import tr
import base64, collections, json, os, queue, re, shutil, signal, subprocess, threading, time, uuid
from pathlib import Path
from backend.kernel import Kernel as _OctaveKernel  # its condition validator only; no Octave is started

START_TIMEOUT = 10.0
REQUEST_TIMEOUT = 10.0
STOP_GRACE = 5.0  # after an interrupt, kill the child if the eval has not ended
OUTPUT_LIMIT = 1_000_000
BUSY = ('running', 'starting', 'stopping', 'paused')
NAME = re.compile(r'[A-Za-z][A-Za-z0-9_]{0,62}')
# Kernel mode / workspace mode -> protocol op the engine reports as unsupported.
UNSUPPORTED_MODES = {'profile': 'profile', 'publish': 'publish', 'rotate': 'figure_rotate', 'package': 'package',
                     'workspace-assign-scalar': 'assign_scalar', 'variable-write': 'variable_write'}
# MAT-file workspace actions (protocol ops `save` / `load`); large files may take a while.
MAT_MODES = ('workspace-save', 'workspace-load-inspect', 'workspace-load')
MAT_TIMEOUT = 300.0
# UI features the Octave-free services (OctaveFreeServices) never provide, on top of
# the kernel's `info.unsupported` list; the UI hides them (state `unsupported`).
SERVICE_UNSUPPORTED = ('packages', 'help', 'lint', 'source_adapter')
WORKSPACE_MODES = {'workspace-rename', 'workspace-assign-scalar', 'workspace-clear-names', 'workspace-save',
                   'workspace-load-inspect', 'workspace-load', 'variable-read', 'variable-write'}

# Engine-level messages stay in English, like the engine's own messages.
CLOSED = 'The Rust engine closed. Reset the session.'
NO_SOURCE_ADAPTER = 'Source adaptation is not available in the Rust engine yet.'
NOT_ANSWERING = 'The Rust engine did not answer in time; it was terminated. Reset the session. Variables could not be recovered.'
NOT_STOPPABLE = 'A MAT-file operation cannot be stopped; it ends by itself.'


def kernel_executable(root):
    explicit = os.environ.get('INDYMAT_R_KERNEL')
    if explicit:
        # which() takes a path or a bare name; only a file that can really start counts.
        found = shutil.which(explicit)
        if not found:
            raise RuntimeError(tr('INDYMAT_R_KERNEL does not name an executable file: {path}', path=explicit))
        return found
    # The app root first, then this checkout (the binary is its build artifact).
    for base in dict.fromkeys((Path(root), Path(__file__).resolve().parents[1])):
        for profile in ('release', 'debug'):
            candidate = base / 'motor' / 'target' / profile / 'indy-kernel'
            if candidate.is_file() and os.access(candidate, os.X_OK):
                return str(candidate)
    raise RuntimeError(tr('The Rust engine is not built yet. Build it with: cd motor && cargo build --release'))


class EngineFailure(Exception):
    """The child refused or could not answer a request."""
    def __init__(self, message, code='engine'):
        super().__init__(message)
        self.code = code


class EngineTimeout(EngineFailure):
    """The child is alive but did not answer: it is wedged, and every later request would wait too."""


class _Session:
    """One indy-kernel child process and its JSON-lines streams."""

    def __init__(self, executable, cwd):
        env = {k: v for k, v in os.environ.items() if k != '__CFBundleIdentifier'}
        # IndyMAT's own MIT helpers (netlist layer im_*), never octave/compat.
        env.setdefault('INDYMAT_R_PATH', str(Path(__file__).resolve().parents[1] / 'octave'))
        self.proc = subprocess.Popen([executable], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     cwd=cwd, env=env, start_new_session=True, bufsize=0)
        self.cond = threading.Condition()
        self.write_lock = threading.Lock()
        self.responses = {}
        self.next_id = 0
        self.events = queue.Queue()
        self.ready = None
        self.alive = True
        self.stderr = collections.deque(maxlen=20)
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._drain_stderr, daemon=True).start()

    def _read(self):
        try:
            for raw in self.proc.stdout:
                try:
                    message = json.loads(raw)
                except ValueError:
                    continue
                if not isinstance(message, dict):
                    continue
                kind = message.get('type')
                if kind == 'event':
                    self.events.put(message)
                elif kind in ('ready', 'response'):
                    with self.cond:
                        if kind == 'ready':
                            self.ready = message
                        else:
                            self.responses[message.get('id')] = message
                        self.cond.notify_all()
        except (OSError, ValueError):
            pass
        finally:
            with self.cond:
                self.alive = False
                self.cond.notify_all()
            self.events.put(None)

    def _drain_stderr(self):
        try:
            for raw in self.proc.stderr:
                self.stderr.append(raw.decode('utf-8', 'replace').rstrip())
        except (OSError, ValueError):
            pass

    def wait_ready(self, timeout=START_TIMEOUT):
        with self.cond:
            self.cond.wait_for(lambda: self.ready is not None or not self.alive, timeout)
            return self.ready

    def send(self, op, **params):
        with self.write_lock:
            self.next_id += 1
            ident = self.next_id
            line = json.dumps({'id': ident, 'op': op, **params}, ensure_ascii=False, separators=(',', ':')) + '\n'
            try:
                self.proc.stdin.write(line.encode('utf-8'))
                self.proc.stdin.flush()
            except (BrokenPipeError, OSError, ValueError) as exc:
                raise EngineFailure(CLOSED) from exc
            return ident

    def wait(self, ident, timeout=REQUEST_TIMEOUT):
        with self.cond:
            self.cond.wait_for(lambda: ident in self.responses or not self.alive, timeout)
            reply = self.responses.pop(ident, None)
        if reply is None:
            if self.alive:
                raise EngineTimeout('The Rust engine did not answer in time.')
            raise EngineFailure(CLOSED)
        return reply

    def request(self, op, timeout=REQUEST_TIMEOUT, **params):
        """The result object of a successful response, else EngineFailure."""
        reply = self.wait(self.send(op, **params), timeout)
        if reply.get('ok') is True:
            return reply.get('result') or {}
        error = reply.get('error') or {}
        raise EngineFailure(str(error.get('message') or 'The engine refused the request.'), error.get('code') or 'engine')

    def interrupt(self):
        try:
            os.kill(self.proc.pid, signal.SIGINT)
        except ProcessLookupError:
            pass

    def kill(self):
        if self.proc.poll() is None:
            try:
                os.killpg(self.proc.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                self.proc.kill()
        try:
            self.proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            pass
        for pipe in (self.proc.stdin, self.proc.stdout, self.proc.stderr):
            try:
                pipe.close()
            except OSError:
                pass

    def close(self):
        if self.alive and self.proc.poll() is None:
            try:
                self.request('close', timeout=2)
                self.proc.wait(timeout=2)
            except (EngineFailure, subprocess.TimeoutExpired):
                pass
        self.kill()


class RustKernel:
    """Same public interface as backend.kernel.Kernel; the engine is indy-kernel."""

    def __init__(self, root, runtime, workspace, executable=None):
        self.root, self.runtime, self.workspace = map(Path, (root, runtime, workspace))
        self.executable = executable or kernel_executable(self.root)
        self.lock = threading.RLock()
        self.session = None
        self.job = None
        self.breakpoints = {}
        self.run_to_cursor_state = None
        self._stale_breakpoints = set()
        self._debug_serial = 0
        self.generation = 0
        self.closed = False
        self.engine = {}
        self.state = {'status': 'starting', 'output': '', 'variables': [], 'figures': [], 'error': None, 'version': '', 'unsupported': [],
                      'packages': [], 'cwd': str(self.workspace), 'waiting_input': False, 'console_clear': 0, 'debug': None}
        self.runtime.mkdir(parents=True, exist_ok=True)
        self.start()

    @staticmethod
    def quote(s):
        return "'" + str(s).replace("'", "''") + "'"

    # Session lifecycle

    def start(self):
        self.generation += 1
        self._eval_id = None
        session = _Session(self.executable, self.workspace)
        ready = session.wait_ready()
        if not ready or ready.get('protocol') != 1:
            detail = ' '.join(session.stderr) or 'no ready message'
            session.kill()
            self.state.update(status='dead', error=CLOSED + ' (' + detail + ')')
            raise RuntimeError('The Rust engine did not start: ' + detail)
        self.session = session
        self.engine = dict(ready.get('engine') or {})
        try:
            info = session.request('info')
        except EngineFailure:
            info = {}
        unsupported = [f for f in info.get('unsupported') or [] if isinstance(f, str)]
        unsupported += [f for f in SERVICE_UNSUPPORTED if f not in unsupported]
        self.job = uuid.uuid4().hex
        self.state.update(status='idle', job=self.job, kind='code', output='', error=None, started=time.time(), elapsed=0,
                          variables=[], figures=[], waiting_input=False, console_clear=0, debug=None, detail=None,
                          workspace_action=None, variable_action=None, cwd=str(ready.get('cwd') or self.workspace),
                          version=str(self.engine.get('description') or self.engine.get('name') or 'IndyMAT Rust engine'),
                          engine={'name': self.engine.get('name'), 'stub': bool(self.engine.get('stub'))},
                          unsupported=unsupported)
        self.run_to_cursor_state = None
        self._stale_breakpoints = set()
        # A new child knows no breakpoints; they belong to the session.
        for path, points in sorted(self.breakpoints.items()):
            for line, item in sorted(points.items()):
                if not item['enabled']:
                    continue
                try:
                    operation = {'action': 'set', 'path': path, 'line': line, 'condition': item['condition']}
                    result = session.request('breakpoint', **operation)
                    if result.get('actual_line') not in (None, line):
                        self._snap_breakpoint(session, self.generation, operation, result['actual_line'])
                except EngineFailure as exc:
                    self.state['output'] += tr('Could not apply breakpoint: %s').replace('%s', str(exc)) + '\n'

    def snapshot(self):
        with self.lock:
            state = json.loads(json.dumps(self.state))
            if state['status'] in ('starting', 'running', 'stopping'):
                state['elapsed'] = round(time.time() - state['started'], 2)
            state['breakpoints'] = [{'file': path, 'lines': sorted(line for line, item in points.items() if item['enabled']),
                                     'breakpoints': [{'line': line, **item} for line, item in sorted(points.items())]}
                                    for path, points in sorted(self.breakpoints.items())]
            if self.run_to_cursor_state:
                rtc = self.run_to_cursor_state
                state['run_to_cursor'] = {'file': rtc['file'], 'line': rtc.get('actual_line') or rtc['line'], 'continued': rtc.get('continued', False)}
            return state

    def _alive(self):
        if not self.session or not self.session.alive or self.session.proc.poll() is not None:
            raise ValueError(CLOSED)

    def _job_finished(self):
        """Hook for the app's completion fence; called under self.lock."""

    # Jobs

    def _launch(self, kind, work, timeout=0):
        """Start a job; `work(job)` runs on a worker thread and returns state updates."""
        self._alive()
        job = uuid.uuid4().hex
        self.job = job
        self._stop_at = None
        self._stop_reason = None
        self._eval_id = None
        self._debug_serial = 0
        self.run_to_cursor_state = None
        self.state.update(status='running', job=job, kind=kind, output='', error=None, started=time.time(), elapsed=0,
                          waiting_input=False, console_clear=0, debug=None, detail=None, workspace_action=None,
                          variable_action=None, breakpoint_relocation=None)
        threading.Thread(target=self._run, args=(job, self.generation, self.session, kind, work, float(timeout or 0)),
                         daemon=True).start()
        return job

    def _run(self, job, gen, session, kind, work, timeout):
        try:
            self._clear_stale_breakpoints(session)
            try:
                updates = work(job, gen, session, timeout)
                error = updates.pop('error', None)
            except EngineTimeout:
                raise
            except EngineFailure as exc:
                updates, error = {}, str(exc)
            variables = None
            if session.alive:
                try:
                    variables = session.request('workspace').get('variables', [])
                except EngineTimeout:
                    raise
                except EngineFailure as exc:
                    error = error or str(exc)
        except (_Dead, EngineTimeout) as exc:
            with self.lock:
                if gen != self.generation or self.closed:
                    return
                session.kill()
                self.run_to_cursor_state = None
                self.state.update(status='dead', error=NOT_ANSWERING if isinstance(exc, EngineTimeout) else str(exc), waiting_input=False, debug=None,
                                  elapsed=round(time.time() - self.state['started'], 3))
                self._job_finished()
            return
        with self.lock:
            if gen != self.generation or self.closed or self.job != job:
                return
            quit_requested = updates.pop('quit', False)
            if self.state['status'] == 'stopping' and not quit_requested:
                error = self._stop_reason or error or tr('Operation stopped.')
            self.state.update(updates)
            if variables is not None:
                self.state['variables'] = [self._variable(item) for item in variables]
            alive = session.alive and session.proc.poll() is None
            self.run_to_cursor_state = None
            self.state.update(status='idle' if alive else 'dead', error=error or (None if alive else CLOSED), job=job, debug=None,
                              waiting_input=False, elapsed=round(time.time() - self.state['started'], 3))
            self._eval_id = None
            self._job_finished()
            self._prune()

    def _clear_stale_breakpoints(self, session):
        """Locations a relocation had to leave in the child because it was busy; before anything else runs."""
        with self.lock:
            stale, self._stale_breakpoints = self._stale_breakpoints, set()
        for path in sorted(stale):
            try:
                session.request('breakpoint', action='clear_all', path=path)
            except EngineTimeout:
                raise
            except EngineFailure:
                pass

    @staticmethod
    def _variable(item):
        size = item.get('size') or []
        return {'name': item.get('name'), 'size': 'x'.join(str(n) for n in size), 'class': item.get('class'),
                'bytes': item.get('bytes', 0), 'complex': bool(item.get('complex')), 'preview': item.get('preview', ''),
                'global': bool(item.get('global'))}

    def _eval_work(self, params):
        def work(job, gen, session, timeout):
            ident = session.send('eval', **params)
            reply = session.wait(ident)
            if reply.get('ok') is not True:
                return {'error': str((reply.get('error') or {}).get('message') or 'The engine refused the code.')}
            with self.lock:
                if gen != self.generation:
                    return {}
                self._eval_id = ident
                if self.state['status'] == 'stopping':
                    session.interrupt()  # Stop arrived before the eval was running.
            deadline = time.monotonic() + timeout if timeout else None
            raw, figures, terminal = '', [], None
            while terminal is None:
                try:
                    event = session.events.get(timeout=.1)
                except queue.Empty:
                    event = False
                with self.lock:
                    if gen != self.generation or self.closed:
                        return {}
                    stopping = self.state['status'] == 'stopping'
                    if deadline and time.monotonic() > deadline and not stopping and self.state['status'] != 'paused':
                        self.state['status'] = 'stopping'
                        self._stop_at = time.monotonic()
                        self._stop_reason = tr('The {seconds:g}-second time limit was exceeded.', seconds=timeout)
                        session.interrupt()
                    elif stopping and self._stop_at and time.monotonic() - self._stop_at > STOP_GRACE:
                        raise _Dead('The Rust engine did not stop; it was terminated. Reset the session. Variables could not be recovered.')
                if event is None:
                    detail = session.stderr[-1] if session.stderr else ''
                    raise _Dead(CLOSED + (' (' + detail + ')' if detail else ''))
                if event is False or event.get('id') != ident:
                    continue
                kind = event.get('event')
                if kind == 'output':
                    raw += str(event.get('text', ''))
                    if len(raw) > OUTPUT_LIMIT:
                        raw = tr('[Previous output truncated]\n') + raw[-800_000:]
                elif kind == 'clc':
                    raw = ''
                    with self.lock:
                        self.state['console_clear'] += 1
                elif kind == 'figure':
                    figures.append(event)
                elif kind == 'input_request':
                    with self.lock:
                        if self.state['status'] == 'running':
                            self.state['waiting_input'] = True
                elif kind == 'paused':
                    with self.lock:
                        if gen == self.generation and self.state['status'] != 'stopping':
                            self._apply_pause(event)
                elif kind in ('done', 'error'):
                    terminal = event
                with self.lock:
                    if gen == self.generation:
                        self.state['output'] = raw
                        self.state['elapsed'] = round(time.time() - self.state['started'], 2)
            updates = {'output': raw, 'cwd': str(terminal.get('cwd') or self.state['cwd'])}
            if terminal['event'] == 'error':
                message = str(terminal.get('message') or '')
                location = terminal.get('location')
                if isinstance(location, dict) and location.get('file'):
                    message += f"\n  {Path(location['file']).stem}:{location.get('line')}"
                if terminal.get('quit'):
                    updates['quit'] = True
                else:
                    updates['error'] = tr('Operation stopped.') if terminal.get('interrupted') else message
            updates['figures'] = self._store_figures(job, figures)
            return updates
        return work

    def _apply_pause(self, event):
        """A `paused` event: the same state fields the Octave kernel's snapshot gives. Caller holds the lock."""
        self._debug_serial += 1
        target = event.get('run_to_cursor')
        if not target:
            self.run_to_cursor_state = None
        elif self.run_to_cursor_state:
            self.run_to_cursor_state['actual_line'] = target.get('line')
        self.state['error'] = None
        self.state['variables'] = [self._variable(item) for item in event.get('variables') or []]
        self.state.update(status='paused', waiting_input=False,
                          debug={'file': str(event.get('file') or ''), 'line': int(event.get('line') or 0), 'frame': int(event.get('frame') or 0),
                                 'reason': event.get('reason'), 'stack': event.get('stack') or [], 'ready': True, 'serial': self._debug_serial})

    def _debug_request(self, op, params, finish=None):
        """Send a request of a pause; the reader thread delivers its reply. Caller holds the lock."""
        session, gen, job = self.session, self.generation, self.job
        try:
            ident = session.send(op, **params)
        except EngineFailure as exc:
            raise ValueError(CLOSED) from exc
        self.state['error'] = None
        threading.Thread(target=self._debug_reply, args=(session, gen, job, ident, op, finish), daemon=True).start()

    def _debug_reply(self, session, gen, job, ident, op, finish):
        try:
            reply = session.wait(ident, None)  # code run at the prompt may take any time
        except EngineFailure:
            return  # the eval's reader reports a dead child
        with self.lock:
            if gen != self.generation or self.job != job or self.state['status'] == 'stopping':
                return
            if reply.get('ok') is True:
                if finish:
                    finish(reply.get('result') or {})
                return
            # Refused: the code is still paused, and a pending run to cursor stays pending.
            if op == 'run_to_cursor':
                self.run_to_cursor_state = None
            self.state['error'] = str((reply.get('error') or {}).get('message') or 'The engine refused the request.')
            self._debug_ready()

    def _debug_ready(self):
        self._debug_serial += 1
        if self.state.get('debug'):
            self.state['debug'].update(ready=True, serial=self._debug_serial)
        self.state['status'] = 'paused'

    def _store_figures(self, job, events):
        """The complete list of open figures after an eval (protocol `figure` events)."""
        figures = []
        folder = self.runtime / job
        for index, event in enumerate(events[:12], 1):
            data = event.get('figure') if isinstance(event.get('figure'), dict) else None
            png = event.get('png_base64')
            folder.mkdir(exist_ok=True)
            if data is not None and isinstance(data.get('source'), dict):
                # The export identity, as __mf_execute__ passes it to the extractor:
                # the UI only accepts figure data that names this job and ordinal.
                data['source'].update(job=job, figure=index)
            if data is not None:
                (folder / f'figure-{index}.json').write_text(json.dumps(data, ensure_ascii=False))
            if isinstance(png, str) and png:
                (folder / f'figure-{index}.png').write_bytes(base64.b64decode(png, validate=True))
            figures.append({'name': str(event.get('name') or f'Figure {index}'), 'file': f'figure-{index}.png',
                            'number': event.get('number', index), 'data_file': f'figure-{index}.json',
                            'interactive': bool(data and data.get('supported')), 'decimated': bool(data and data.get('decimated')),
                            'fallback_reason': str(event.get('reason') or (data or {}).get('reason') or ''), 'job': job})
        return figures

    def _request_work(self, op, params, finish):
        def work(job, gen, session, timeout):
            return finish(session.request(op, **params))
        return work

    def _mat_work(self, mode, request):
        """workspace-save / -load-inspect / -load (backend.workspace_actions args) -> `save` / `load`."""
        path = request.get('path')
        if not isinstance(path, str) or not path:
            raise ValueError(tr('Invalid MAT path.'))
        if mode == 'workspace-save':
            op, params = 'save', {'path': path, 'names': None if request.get('all') else request.get('names')}
        elif mode == 'workspace-load-inspect':
            op, params = 'load', {'path': path, 'inspect': True}
        else:
            op, params = 'load', {'path': path, 'inventory': request.get('inventory'), 'replacements': request.get('replacements')}

        def work(job, gen, session, timeout):
            result = session.request(op, timeout=MAT_TIMEOUT, **params)
            warnings = [str(w) for w in result.pop('warnings', None) or []]
            updates = {'workspace_action': {'action': mode, **result}}
            if warnings:
                updates['output'] = ''.join('warning: ' + w + '\n' for w in warnings)
            return updates
        return work

    def _unsupported(self, op, **params):
        """Ask the engine; raise its explicit `unsupported` message for the UI."""
        with self.lock:
            self._alive()
            try:
                self.session.request(op, **params)
            except EngineFailure as exc:
                raise ValueError(str(exc)) from exc
            raise ValueError(f'The engine supports {op}, but IndyMAT does not connect it yet.')

    # Kernel interface

    def submit(self, code='', mode='code', argument='', timeout=0, *, source_context=None, source_file=None):
        with self.lock:
            if self.state['status'] in BUSY:
                raise ValueError(tr('Stop the running operation first.'))
            self._alive()
            if source_context is not None or source_file is not None:
                raise ValueError(NO_SOURCE_ADAPTER)
            if mode == 'code':
                return self._launch('code', self._eval_work({'code': code, 'mode': 'code'}), timeout)
            if mode == 'file':
                return self._launch('file', self._eval_work({'code': '', 'mode': 'file', 'path': str(argument)}), timeout)
            if mode == 'inspect':
                if not isinstance(argument, str) or not NAME.fullmatch(argument):
                    raise ValueError(tr('Invalid or reserved variable name.'))
                return self._launch('inspect', self._request_work('inspect', {'name': argument}, lambda page: {'detail': self._detail(page)}))
            if mode in WORKSPACE_MODES:
                return self.workspace_action(mode, json.loads(argument) if isinstance(argument, str) and argument else {})
            if mode in UNSUPPORTED_MODES:
                self._unsupported(UNSUPPORTED_MODES[mode])
            raise ValueError(tr('Invalid operation.'))

    @staticmethod
    def _detail(page):
        detail = {'name': page.get('name'), 'class': page.get('class'), 'size': page.get('size') or [], 'rows': [],
                  'text': '', 'truncated': bool(page.get('truncated'))}
        if page.get('kind') == 'matrix':
            detail['rows'] = page.get('rows') or []
        else:
            detail['text'] = str(page.get('text') or '')
        return detail

    def package(self, action, name):
        if action not in ('load', 'unload') or not isinstance(name, str) or not NAME.fullmatch(name):
            raise ValueError(tr('Invalid package operation.'))
        self._unsupported('package', action=action, name=name)

    def workspace_action(self, mode, request):
        if mode not in WORKSPACE_MODES or not isinstance(request, dict):
            raise ValueError(tr('Invalid Workspace operation.'))
        with self.lock:
            if self.state['status'] == 'paused':
                raise ValueError(tr('Workspace is read-only while debugging is paused.'))
            if self.state['status'] in BUSY:
                raise ValueError(tr('Stop the running operation first.'))
            if mode == 'workspace-rename':
                params = {'old_name': request.get('old_name'), 'new_name': request.get('new_name')}
                return self._launch(mode, self._request_work('rename', params, lambda result: {'workspace_action': {'action': mode, **result}}))
            if mode == 'workspace-clear-names':
                return self._launch(mode, self._request_work('clear', {'names': request.get('names')},
                                                             lambda result: {'workspace_action': {'action': mode, 'names': result.get('names', [])}}))
            if mode == 'variable-read':
                if request.get('path') or request.get('slices'):
                    raise ValueError('Cell, struct and multidimensional pages are not supported in the Rust engine yet.')
                params = {key: request[key] for key in ('name', 'row', 'column', 'rows', 'columns')}
                def finish(page):
                    return {'variable_action': {**page, 'action': 'read', 'name': request['name'], 'root_class': page.get('class'),
                                                'root_size': page.get('size'), 'slices': []}}
                return self._launch(mode, self._request_work('inspect', params, finish))
            if mode in MAT_MODES:
                return self._launch(mode, self._mat_work(mode, request))
            self._unsupported(UNSUPPORTED_MODES[mode])

    def interrupt(self):
        """Idempotent: every call while running or stopping sends SIGINT (one acts as many)."""
        with self.lock:
            if self.state['status'] not in ('running', 'starting', 'stopping', 'paused'):
                return
            # Only an eval can stop: a request runs to its end and reports its real result.
            if self.state.get('kind') in MAT_MODES:
                raise ValueError(NOT_STOPPABLE)
            if self.state.get('kind') not in ('code', 'file'):
                return
            if self.state['status'] != 'stopping':
                self.state['status'] = 'stopping'
                self._stop_at = time.monotonic()
            self.state['waiting_input'] = False
            self.state['debug'] = None
            self.run_to_cursor_state = None
            if self._eval_id is not None and self.session:
                self.session.interrupt()

    def input(self, text):
        with self.lock:
            if self.state['status'] != 'running' or not self.state.get('waiting_input'):
                raise ValueError(tr('No operation is waiting for input.'))
            if not isinstance(text, str) or '\n' in text or '\r' in text:
                raise ValueError(tr('Use a single line of input.'))
            try:
                self.session.request('input_reply', text=text)
            except EngineFailure as exc:
                raise ValueError(str(exc)) from exc
            self.state['waiting_input'] = False

    # Debugger. The canonical map is `breakpoints = {path: {line: {enabled, condition}}}`,
    # as in the Octave kernel; each change is one identified job.

    _condition = staticmethod(_OctaveKernel._condition)

    @staticmethod
    def _breakpoint_target(path, line):
        path = Path(path).resolve()
        line = int(line)
        if path.suffix.lower() != '.m' or not path.is_file():
            raise ValueError(tr('Select a saved .m file for the breakpoint.'))
        if line < 1:
            raise ValueError(tr('Invalid line number.'))
        return path, line

    def _clone_breakpoints(self):
        return {path: {line: dict(item) for line, item in points.items()} for path, points in self.breakpoints.items()}

    def _breakpoint_job(self, operations, previous):
        """A job of `breakpoint` requests; a refusal restores the previous map."""
        def work(job, gen, session, timeout):
            try:
                for operation in operations:
                    result = session.request('breakpoint', **operation)
                    if operation['action'] == 'set' and result.get('actual_line') not in (None, operation['line']):
                        self._snap_breakpoint(session, gen, operation, result['actual_line'])
            except EngineTimeout:
                raise
            except EngineFailure as exc:
                with self.lock:
                    if gen == self.generation:
                        self.breakpoints = previous
                return {'error': str(exc)}
            return {}
        return self._launch('breakpoint', work)

    def _snap_breakpoint(self, session, gen, operation, actual):
        """A line without code: the marker moves to the line it stops at, as in MATLAB.

        A breakpoint already on that line keeps its condition and state.
        """
        path, line = operation['path'], operation['line']
        with self.lock:
            existing = self.breakpoints.get(path, {}).get(actual)
        session.request('breakpoint', action='clear', path=path, line=line)
        if existing is None:
            session.request('breakpoint', **{**operation, 'line': actual})
        elif existing['enabled']:
            session.request('breakpoint', **{**operation, 'line': actual, 'condition': existing['condition']})
        else:
            session.request('breakpoint', action='clear', path=path, line=actual)
        with self.lock:
            points = self.breakpoints.get(path, {})
            if gen == self.generation and line in points:
                moved = points.pop(line)
                points.setdefault(actual, moved)

    def breakpoint(self, path, line, enabled, condition=''):
        return self.configure_breakpoint(path, line, 'set' if enabled else 'remove', condition, enabled)

    def configure_breakpoint(self, path, line, action, condition=None, enabled=None):
        path, line = self._breakpoint_target(path, line)
        key = str(path)
        if action not in ('set', 'enable', 'disable', 'remove'):
            raise ValueError(tr('Invalid breakpoint action.'))
        if enabled is not None and not isinstance(enabled, bool):
            raise ValueError(tr('Invalid breakpoint state.'))
        if condition is not None:
            condition = self._condition(condition)
        with self.lock:
            if self.state['status'] in BUSY:
                raise ValueError(tr('Finish the operation before changing a breakpoint.'))
            self._alive()
            previous = self._clone_breakpoints()
            points = self.breakpoints.setdefault(key, {})
            if action == 'remove':
                points.pop(line, None)
                if not points:
                    self.breakpoints.pop(key, None)
                operation = {'action': 'clear', 'path': key, 'line': line}
            else:
                item = dict(points.get(line, {'enabled': True, 'condition': ''}))
                if action == 'enable':
                    item['enabled'] = True
                if action == 'disable':
                    item['enabled'] = False
                if action == 'set':
                    item['enabled'] = True if enabled is None else enabled
                if condition is not None:
                    item['condition'] = condition
                points[line] = item
                # A disabled breakpoint exists only in this map, as in the Octave kernel.
                operation = {'action': 'set', 'path': key, 'line': line, 'condition': item['condition']} if item['enabled'] else {'action': 'clear', 'path': key, 'line': line}
            return self._breakpoint_job([operation], previous)

    def clear_breakpoints(self):
        with self.lock:
            if self.state['status'] in BUSY:
                raise ValueError(tr('Finish the operation before clearing breakpoints.'))
            self._alive()
            previous = self._clone_breakpoints()
            operations = [{'action': 'clear', 'path': path, 'line': line} for path, points in previous.items() for line in points]
            self.breakpoints = {}
            return self._breakpoint_job(operations, previous)

    def _require_pause(self, message):
        debug = self.state.get('debug')
        if self.state['status'] != 'paused' or not debug or not debug.get('ready'):
            raise ValueError(message)
        return debug

    def run_to_cursor(self, path, line):
        path, line = self._breakpoint_target(path, line)
        key = str(path)
        with self.lock:
            debug = self._require_pause(tr('Run to Cursor is available only at a real debugger pause.'))
            if self.run_to_cursor_state:
                raise ValueError(tr('Run to Cursor is already pending.'))
            self.run_to_cursor_state = {'file': key, 'line': line, 'actual_line': None, 'continued': False}
            def finish(result):
                if self.run_to_cursor_state:
                    self.run_to_cursor_state.update(actual_line=result.get('actual_line'), continued=True)
            self._debug_request('run_to_cursor', {'path': key, 'line': line}, finish)
            self.state['status'] = 'running'
            debug['ready'] = False

    def debug(self, command, code=''):
        with self.lock:
            debug = self._require_pause(tr('The debugger is not waiting for a command.'))
            params = {'command': command}
            finish = None
            if command == 'eval':
                if not isinstance(code, str) or not code.strip() or len(code) > 100_000 or '\n' in code or '\r' in code:
                    raise ValueError(tr('Enter a single-line Octave expression.'))
                if '\x00' in code:
                    raise ValueError(tr('Debug expressions cannot contain NUL (U+0000).'))
                params['code'] = code
            elif command == 'inspect':
                if not isinstance(code, str) or len(code) > 63 or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', code) or code.startswith('__mf_'):
                    raise ValueError(tr('Invalid or reserved variable name.'))
                params['name'] = code
                def finish(page):
                    if self.state.get('debug'):
                        self.state['debug']['detail'] = self._detail(page)
                    self._debug_ready()
            elif command == 'frame':
                try:
                    target = int(code)
                except (TypeError, ValueError):
                    raise ValueError(tr('Invalid call stack frame.')) from None
                stack = debug.get('stack') or []
                current = next((frame for frame in stack if frame.get('current')), None)
                if not current or not any(frame.get('index') == target for frame in stack):
                    raise ValueError(tr('The call stack frame is no longer valid.'))
                if target == current['index']:
                    return
                params['frame'] = target
            elif command not in ('continue', 'step', 'in', 'out', 'quit'):
                raise ValueError(tr('Invalid debugger command.'))
            self._debug_request('debug', params, finish)
            self.state['status'] = 'stopping' if command == 'quit' else 'running'
            debug['ready'] = False
            debug.pop('detail', None)
            if command == 'quit':
                self._stop_at = time.monotonic()
                self.run_to_cursor_state = None

    def relocate_file_breakpoints(self, source, destination):
        """File-operation hook, called after the filesystem move already succeeded.

        The canonical map is reconciled first and only ever describes existing
        .m targets. Then one identified job clears the old locations and sets
        the moved ones; the per-breakpoint outcome is state.breakpoint_relocation
        ({kind, moved, dropped, failed} rows with source/path/line/message).
        """
        source = str(Path(source).resolve())
        destination = str(Path(destination).resolve()) if destination is not None else None
        with self.lock:
            busy = self.state['status'] != 'idle' or self.state.get('waiting_input')
            affected = [path for path in self.breakpoints if path == source or path.startswith(source + '/')]
            if not affected:
                if busy:
                    raise ValueError(tr('Finish the Octave operation before relocating breakpoints.'))
                return None
            moved, dropped, installs, clears = [], [], [], []
            for path in affected:
                points = self.breakpoints.pop(path)
                target = destination + path[len(source):] if destination is not None else None
                valid = target is not None and Path(target).suffix.lower() == '.m' and Path(target).is_file()
                clears.append(path)
                for line, item in sorted(points.items()):
                    row = {'source': path, 'path': target or path, 'line': line, 'enabled': item['enabled'], 'condition': item['condition']}
                    if valid and item['enabled']:
                        installs.append(row)
                    elif valid:
                        moved.append(row)
                    else:
                        dropped.append({**row, 'message': tr('The breakpoint target is no longer a valid .m file.')})
                if valid:
                    self.breakpoints.setdefault(target, {}).update({line: dict(item) for line, item in points.items()})
            def forget(rows):
                for row in rows:
                    points = self.breakpoints.get(row['path'], {})
                    points.pop(row['line'], None)
                    if not points:
                        self.breakpoints.pop(row['path'], None)
            def work(job, gen, session, timeout):
                failed = []
                for path in clears:
                    try:
                        session.request('breakpoint', action='clear_all', path=path)
                    except EngineTimeout:
                        raise
                    except EngineFailure as exc:
                        failed.extend({**row, 'message': tr('Could not clear the previous breakpoint: {error}', error=str(exc))} for row in installs if row['source'] == path)
                for row in installs:
                    if any(item['source'] == row['source'] and item['line'] == row['line'] for item in failed):
                        continue
                    try:
                        operation = {'action': 'set', 'path': row['path'], 'line': row['line'], 'condition': row['condition']}
                        result = session.request('breakpoint', **operation)
                        actual = result.get('actual_line')
                        if actual not in (None, row['line']):
                            self._snap_breakpoint(session, gen, operation, actual)
                            row = {**row, 'line': actual}
                        moved.append(row)
                    except EngineTimeout:
                        raise
                    except EngineFailure as exc:
                        failed.append({**row, 'message': str(exc)})
                with self.lock:
                    if gen == self.generation:
                        forget(failed)
                return {'breakpoint_relocation': {'kind': 'relocate', 'moved': moved, 'dropped': dropped, 'failed': failed}}
            try:
                if busy:
                    raise ValueError(tr('Finish the Octave operation before relocating breakpoints.'))
                return self._launch('breakpoint', work)
            except Exception as exc:
                # The new locations were not set; never resurrect the removed source paths,
                # and take them out of the child before it runs anything again.
                forget(installs)
                self._stale_breakpoints.update(clears)
                self.state['breakpoint_relocation'] = {'kind': 'relocate', 'moved': moved, 'dropped': dropped, 'failed': [
                    {**row, 'message': tr('Could not submit the breakpoint job: {error}', error=str(exc))} for row in installs]}
                raise

    def reset(self):
        with self.lock:
            self.generation += 1
            session = self.session
            reused = False
            if session and session.alive and self.state['status'] == 'idle':
                try:
                    session.request('reset')
                    reused = True
                except EngineFailure:
                    reused = False
            try:
                if reused:
                    self.job = uuid.uuid4().hex
                    self._eval_id = None
                    self.run_to_cursor_state = None
                    self.state.update(status='idle', job=self.job, kind='code', variables=[], figures=[], error=None, output='',
                                      waiting_input=False, console_clear=0, debug=None, detail=None, workspace_action=None,
                                      variable_action=None, cwd=str(self.workspace), started=time.time(), elapsed=0)
                else:
                    if session:
                        session.kill()
                    self.session = None
                    self.state.update(status='starting', variables=[], figures=[], error=None, output='', waiting_input=False,
                                      console_clear=0, debug=None)
                    self.start()
            except Exception as exc:
                self.state.update(status='dead', error='Could not start the Rust engine: ' + str(exc))
                raise

    def close(self):
        with self.lock:
            self.closed = True
            self.generation += 1
            if self.session:
                self.session.close()

    def _prune(self):
        protected = {self.state.get('job')} | {f.get('job') for f in self.state.get('figures', [])}
        folders = sorted((p for p in self.runtime.iterdir() if p.is_dir()), key=lambda p: p.stat().st_mtime, reverse=True)
        for folder in folders[20:]:
            if folder.name not in protected:
                # The folder is shared with the Octave kernel, whose jobs may hold retained entry folders.
                _OctaveKernel._remove_job(folder)


class _Dead(Exception):
    """The child must be considered gone; the job ends the session."""


class OctaveFreeServices:
    """Rust-mode stand-in for the Octave-backed helpers: syntax check, help, packages.

    Each says it is unavailable instead of starting Octave or returning empty
    results that look like real answers.
    """
    executable = None

    def check(self, code):
        raise ValueError('The syntax check is not available in the Rust engine yet.')

    def help(self, name, cwd):
        return {'found': False, 'name': name, 'message': 'Help is not available in the Rust engine yet.'}

    def packages(self, loaded=()):
        raise ValueError('Packages are not supported in the Rust engine yet.')

    def start(self, action, name='', source=None):
        raise ValueError('Packages are not supported in the Rust engine yet.')

    def status(self, job):
        raise ValueError('Packages are not supported in the Rust engine yet.')

    def close(self):
        pass
