"""Threaded stdio client for codex-cli 0.160.0's app-server protocol.

Callbacks run on daemon threads; on_event must return promptly. Configuration
and executable arguments belong to the application, never to prompt text.
CALL_TIMEOUT bounds RPCs, not model turns or the human's approval decision.
The schema permits ThreadStartParams.config (an arbitrary JSON object); MCP
settings are passed there as mcp_servers, not command-line arguments.
"""
import collections
import json
import os
import queue
import re
import signal
import subprocess
import threading
import time


class CodexAppServerError(RuntimeError):
    pass


def _bounded(text, limit):
    data = str(text).encode('utf-8', 'replace')
    return data[:limit].decode('utf-8', 'ignore'), len(data) > limit


class CodexAppServer:
    CALL_TIMEOUT = 15.0
    WRITE_TIMEOUT = 5.0
    LINE_LIMIT = 2 * 1024 * 1024
    DIFF_LIMIT = 64 * 1024
    RAW_LIMIT = 16 * 1024
    STDERR_LIMIT = 16 * 1024
    # Explicitly ignored bookkeeping; other unknown notifications stay visible.
    # The second line was measured with the real program (codex-cli 0.160.0, 5 Oct 2026).
    BOOKKEEPING = frozenset(('thread/status/changed', 'thread/tokenUsage/updated',
                             'mcpServer/startupStatus/updated', 'hook/started', 'hook/completed', 'remoteControl/status/changed', 'account/updated', 'account/rateLimits/updated',
                             'item/reasoning/summaryPartAdded'))
    DECISIONS = {'accept': 'accept', 'accept_session': 'acceptForSession',
                 'decline': 'decline', 'cancel': 'cancel'}

    def __init__(self, executable, cwd, env, on_event, on_request, config_overrides=(), on_created=None):
        if not isinstance(executable, (str, os.PathLike)) or not str(executable):
            raise ValueError('An executable path is required.')
        argv = [os.fspath(executable), 'app-server']
        for value in config_overrides:
            if not isinstance(value, str) or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_.-]*=[^\x00\r\n]+', value):
                raise ValueError('Invalid application configuration override.')
            argv.extend(('-c', value))
        self.cwd = os.fspath(cwd)
        self.on_event = on_event
        self.on_request = on_request
        self._lock = threading.RLock()
        self._close_lock = threading.Lock()
        self._kill_lock = threading.Lock()
        self._closed = False
        self._terminal = False
        self._killed = False
        self._cleanup_complete = False
        self._next_id = 0
        self._pending = {}
        self._approvals = {}
        self._files = collections.OrderedDict()
        self._turn_diffs = collections.OrderedDict()
        self._reasoning = set()
        self._finished = collections.OrderedDict()
        self._interrupts = set()
        self._stderr = b''
        self._writes = queue.Queue(maxsize=128)
        self._events = queue.Queue(maxsize=512)
        self.proc = None
        self._event_thread = threading.Thread(target=self._deliver_events, daemon=True)
        self._event_thread.start()
        try:
            # Import at construction time so assistants.py can import this
            # module without a circular module-initialization dependency.
            from backend.assistants import child_environment
            self.proc = subprocess.Popen(argv, cwd=self.cwd, env=child_environment(env),
                                         stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                         stderr=subprocess.PIPE, start_new_session=True, bufsize=0)
            self._writer = threading.Thread(target=self._write_loop, daemon=True)
            self._reader = threading.Thread(target=self._read_loop, daemon=True)
            self._stderr_reader = threading.Thread(target=self._drain_stderr, daemon=True)
            self._writer.start()
            self._reader.start()
            self._stderr_reader.start()
            threading.Thread(target=self._watch_child, daemon=True).start()
            # Let the owner cancel/remove the child even during the handshake.
            if on_created is not None: on_created(self)
            initialized = self._call('initialize', {'clientInfo': {'name': 'indymat', 'title': 'IndyMAT', 'version': '1'}})
            if not all(isinstance(initialized.get(key), str) for key in ('userAgent', 'codexHome', 'platformFamily', 'platformOs')):
                raise CodexAppServerError('Invalid initialize response.')
            self._send({'method': 'initialized'}, wait=True)
        except Exception as exc:
            self._fatal('Codex app-server initialization failed: ' + str(exc))
            self.close()
            raise

    def _emit(self, event):
        try:
            self._events.put_nowait(event)
        except queue.Full:
            self._fatal('Codex app-server event consumer is too slow.')

    def _deliver_events(self):
        while True:
            event = self._events.get()
            if event is None:
                return
            try:
                self.on_event(event)
            except Exception:
                # A UI consumer failure must not strand transport requests.
                pass

    def _raw(self, value):
        text, truncated = _bounded(json.dumps(value, ensure_ascii=False), self.RAW_LIMIT)
        self._emit({'type': 'raw', 'text': text, 'truncated': truncated})

    def _send(self, message, wait=False, closing=False):
        done = threading.Event()
        packet = {'data': (json.dumps(message, ensure_ascii=False) + '\n').encode('utf-8'), 'done': done, 'error': None}
        with self._lock:
            if self._closed and not closing:
                raise CodexAppServerError('Codex app-server is closed.')
            try:
                self._writes.put_nowait(packet)
            except queue.Full:
                self._fatal('Codex app-server write queue is full.')
                raise CodexAppServerError('Codex app-server write queue is full.')
        if wait:
            if not done.wait(self.WRITE_TIMEOUT):
                self._fatal('Codex app-server write timed out.')
                raise TimeoutError('Codex app-server write timed out.')
            if packet['error']:
                raise packet['error']

    def _write_loop(self):
        while True:
            packet = self._writes.get()
            if packet is None:
                return
            try:
                data = memoryview(packet['data'])
                while data:
                    count = os.write(self.proc.stdin.fileno(), data)
                    if not count:
                        raise OSError('Closed app-server input pipe.')
                    data = data[count:]
            except (OSError, ValueError) as exc:
                packet['error'] = CodexAppServerError(str(exc))
                self._fatal('Codex app-server input failed: ' + str(exc))
            finally:
                packet['done'].set()

    def _call(self, method, params):
        pending = {'done': threading.Event(), 'result': None, 'error': None}
        with self._lock:
            if self._closed:
                raise CodexAppServerError('Codex app-server is closed.')
            self._next_id += 1
            identity = self._next_id
            self._pending[identity] = pending
        try:
            self._send({'id': identity, 'method': method, 'params': params})
            if not pending['done'].wait(self.CALL_TIMEOUT):
                self._fatal('Codex app-server request timed out: ' + method)
                raise TimeoutError('Codex app-server request timed out: ' + method)
            if pending['error']:
                raise pending['error']
            return pending['result']
        finally:
            with self._lock:
                self._pending.pop(identity, None)

    def _read_loop(self):
        try:
            while True:
                line = self.proc.stdout.readline(self.LINE_LIMIT + 1)
                if not line:
                    # The reaper notices even an exited child whose grandchild
                    # keeps this pipe open. EOF before exit is a broken protocol.
                    self._fatal('Codex app-server output closed.')
                    return
                if len(line) > self.LINE_LIMIT or not line.endswith(b'\n'):
                    raise ValueError('Oversized or incomplete protocol line.')
                message = json.loads(line)
                if not isinstance(message, dict):
                    raise ValueError('Protocol message must be an object.')
                if 'method' in message:
                    if not isinstance(message['method'], str) or not isinstance(message.get('params', {}), dict):
                        raise ValueError('Invalid protocol method or params.')
                    if 'id' in message:
                        self._request(message)
                    else:
                        self._notification(message['method'], message.get('params', {}))
                elif 'id' in message and (('result' in message) != ('error' in message)):
                    identity = message['id']
                    if not isinstance(identity, (str, int)) or isinstance(identity, bool):
                        raise ValueError('Invalid response id.')
                    with self._lock:
                        pending = self._pending.get(identity)
                        if pending:
                            if pending['done'].is_set():
                                raise ValueError('Duplicate response id.')
                            if 'result' in message and not isinstance(message['result'], dict):
                                raise ValueError('RPC result must be an object.')
                            if 'error' in message:
                                pending['error'] = CodexAppServerError('Codex RPC error: ' + _bounded(json.dumps(message['error']), self.RAW_LIMIT)[0])
                            else:
                                pending['result'] = message['result']
                            pending['done'].set()
                    if not pending:
                        self._raw(message)
                else:
                    raise ValueError('Invalid protocol envelope.')
        except (OSError, ValueError, TypeError, KeyError, AttributeError, RecursionError, CodexAppServerError) as exc:
            self._fatal('Codex app-server protocol error: ' + str(exc))

    def _drain_stderr(self):
        try:
            while True:
                chunk = self.proc.stderr.read(4096)
                if not chunk:
                    return
                with self._lock:
                    self._stderr = (self._stderr + chunk)[-self.STDERR_LIMIT:]
        except (OSError, ValueError):
            pass

    @property
    def stderr_tail(self):
        """Bounded diagnostics, not automatically inserted into the transcript."""
        with self._lock:
            return self._stderr.decode('utf-8', 'replace')

    def _watch_child(self):
        status = self.proc.wait()
        self._fatal('Codex app-server exited with status ' + str(status) + '.')

    def _fatal(self, text):
        with self._lock:
            if self._closed or self._terminal:
                return
            self._terminal = True
            self._closed = True
            for pending in self._pending.values():
                if not pending['done'].is_set():
                    pending['error'] = CodexAppServerError(text)
                    pending['done'].set()
            # Reserve room for exactly one terminal event, even on overflow.
            if self._events.full():
                self._events.get_nowait()
            self._events.put_nowait({'type': 'error', 'text': text, 'terminal': True, 'state': 'failed'})
        threading.Thread(target=self.close, daemon=True).start()

    def _request(self, message):
        identity, method = message['id'], message['method']
        if not isinstance(identity, (str, int)) or isinstance(identity, bool):
            raise ValueError('Invalid server request id.')
        kinds = {'item/commandExecution/requestApproval': 'command', 'item/fileChange/requestApproval': 'file'}
        if method in kinds:
            params = message.get('params', {})
            with self._lock:
                if identity in self._approvals:
                    raise ValueError('Duplicate server request id.')
                if len(self._approvals) >= 32 or self._closed:
                    self._send({'id': identity, 'result': {'decision': 'cancel'}}, closing=True)
                    return
                self._approvals[identity] = params
                info = self._files.get((params.get('threadId'), params.get('turnId'), params.get('itemId')),
                                       self._turn_diffs.get((params.get('threadId'), params.get('turnId')), {}))
                payload = {**info, 'command': params.get('command'), 'reason': params.get('reason'),
                           'cwd': params.get('cwd') or self.cwd, 'item_id': params.get('itemId'),
                           'thread_id': params.get('threadId'), 'turn_id': params.get('turnId'),
                           'request_id': identity}
                for key in ('approvalId', 'kind', 'grantRoot', 'networkApprovalContext'):
                    if key in params:
                        payload[key] = params[key]
            def decide():
                decision = 'cancel'
                try:
                    with self._lock:
                        closed = self._closed
                    if not closed:
                        decision = self.on_request(kinds[method], payload)
                except Exception:
                    pass
                self._answer(identity, self.DECISIONS.get(decision, 'cancel') if isinstance(decision, str) else 'cancel')
            threading.Thread(target=decide, daemon=True).start()
            return
        # Schema-specific negative responses. Unknown methods use JSON-RPC's
        # method-not-found error, including auth/attestation we cannot supply.
        unsupported = {
            'item/tool/requestUserInput': {'answers': {}},
            'item/permissions/requestApproval': {'permissions': {}, 'scope': 'turn'},
            'item/tool/call': {'success': False, 'contentItems': [{'type': 'inputText', 'text': 'Unsupported client tool.'}]},
            'mcpServer/elicitation/request': {'action': 'decline'},
            'applyPatchApproval': {'decision': {'denied': {'rejection': 'Unsupported legacy approval.'}}},
            'execCommandApproval': {'decision': {'denied': {'rejection': 'Unsupported legacy approval.'}}},
        }
        self._raw({'method': method, 'unsupported': True})
        reply = {'id': identity}
        if method in unsupported:
            reply['result'] = unsupported[method]
        else:
            reply['error'] = {'code': -32601, 'message': 'Unsupported client request: ' + method[:200]}
        self._send(reply)

    def _answer(self, identity, decision):
        with self._lock:
            if identity not in self._approvals:
                return
            self._approvals.pop(identity)
            if self._closed:
                decision = 'cancel'
            try:
                self._send({'id': identity, 'result': {'decision': decision}}, closing=True)
            except (CodexAppServerError, OSError, ValueError):
                pass

    def _file_info(self, changes):
        if not isinstance(changes, list):
            raise ValueError('File changes must be a list.')
        paths = []
        parts = []
        for change in changes[:256]:
            kind = change.get('kind', {})
            if not isinstance(change.get('path'), str) or not isinstance(change.get('diff'), str) or not isinstance(kind, dict) or kind.get('type') not in ('add', 'delete', 'update'):
                raise ValueError('Invalid file change.')
            paths.append({'path': change.get('path', ''), 'kind': kind.get('type') if isinstance(kind, dict) else kind,
                          **({'move_path': kind['move_path']} if isinstance(kind, dict) and kind.get('move_path') else {})})
            parts.append(change.get('diff', ''))
        diff, truncated = _bounded('\n'.join(parts), self.DIFF_LIMIT)
        return {'paths': paths, 'diff': diff, 'truncated': truncated or len(changes) > 256}

    def _notification(self, method, params):
        context = {key: params[value] for key, value in (('thread_id', 'threadId'), ('turn_id', 'turnId'), ('item_id', 'itemId')) if value in params}
        def emit(event):
            self._emit({**context, **event})
        if method in self.BOOKKEEPING:
            return
        if method == 'serverRequest/resolved':
            with self._lock:
                self._approvals.pop(params['requestId'], None)
            return
        if method == 'thread/started':
            emit({'type': 'conversation', 'conversation': self._identity(params, 'thread')})
        elif method == 'turn/started':
            return
        elif method == 'item/agentMessage/delta':
            if not isinstance(params.get('delta'), str):
                raise ValueError('Invalid agent message delta.')
            emit({'type': 'text', 'text': params['delta'], 'delta': True})
        elif method == 'item/reasoning/summaryTextDelta':
            if not isinstance(params.get('delta'), str):
                raise ValueError('Invalid reasoning delta.')
            with self._lock:
                self._reasoning.add((params.get('threadId'), params.get('turnId'), params.get('itemId')))
            emit({'type': 'reasoning', 'text': params['delta'], 'delta': True})
        elif method in ('item/started', 'item/completed', 'item/fileChange/patchUpdated'):
            item = params if method.endswith('patchUpdated') else params['item']
            category = 'fileChange' if method.endswith('patchUpdated') else item.get('type')
            item_id = params.get('itemId', item.get('id'))
            context['item_id'] = item_id
            key = (params.get('threadId'), params.get('turnId'), item_id)
            if category in ('agentMessage', 'userMessage'):
                return  # Completed text is already delivered by delta notifications.
            if category == 'reasoning':
                if method == 'item/completed':
                    with self._lock:
                        streamed = key in self._reasoning
                    if not streamed and item.get('summary'):
                        emit({'type': 'reasoning', 'text': '\n'.join(item['summary'])})
            elif category in ('commandExecution', 'mcpToolCall', 'webSearch'):
                if method != 'item/started':
                    return
                if category == 'mcpToolCall':
                    name = ('indymat · ' if item.get('server') == 'indymat' else '') + item['tool']
                    text = _bounded(json.dumps(item.get('arguments', {}), ensure_ascii=False), self.RAW_LIMIT)[0]
                else:
                    name = 'command_execution' if category == 'commandExecution' else 'web_search'
                    text = item.get('command', item.get('query', ''))
                emit({'type': 'tool', 'name': name, 'text': text, 'command': item.get('command', '')})
            elif category == 'fileChange':
                info = self._file_info(item.get('changes', []))
                with self._lock:
                    self._files[key] = info
                    while len(self._files) > 128:
                        self._files.popitem(last=False)
                emit({'type': 'file', 'text': json.dumps(info['paths'], ensure_ascii=False), **info,
                      'status': item.get('status')})
            else:
                self._raw({'method': method, 'params': params})
        elif method == 'turn/diff/updated':
            if not isinstance(params.get('diff'), str):
                raise ValueError('Invalid turn diff.')
            diff, truncated = _bounded(params['diff'], self.DIFF_LIMIT)
            key = (params.get('threadId'), params.get('turnId'))
            with self._lock:
                paths = [path for item_key, info in self._files.items() if item_key[:2] == key for path in info['paths']][:256]
                info = {'paths': paths, 'diff': diff, 'truncated': truncated}
                self._turn_diffs[key] = info
                while len(self._turn_diffs) > 128:
                    self._turn_diffs.popitem(last=False)
            emit({'type': 'file', 'text': diff, **info})
        elif method == 'error':
            emit({'type': 'error', 'text': params['error']['message'], 'will_retry': params.get('willRetry', False)})
        elif method == 'turn/completed':
            turn = params['turn']
            if turn['status'] not in ('completed', 'interrupted', 'failed', 'inProgress'):
                raise ValueError('Invalid turn status.')
            key = (params.get('threadId'), self._identity(params, 'turn'))
            with self._lock:
                if key in self._finished:
                    return
                self._finished[key] = True
                while len(self._finished) > 256:
                    self._finished.popitem(last=False)
                self._interrupts.discard(key)
                self._reasoning = {value for value in self._reasoning if value[:2] != key}
                self._files = collections.OrderedDict((k, v) for k, v in self._files.items() if k[:2] != key)
                self._turn_diffs.pop(key, None)
            if turn.get('error'):
                emit({'type': 'error', 'text': turn['error']['message']})
            emit({'type': 'turn-end', 'turn_id': turn['id'], 'status': turn['status'],
                  'state': 'stopped' if turn['status'] == 'interrupted' else turn['status']})
        else:
            self._raw({'method': method, 'params': params})

    @staticmethod
    def _identity(result, key):
        identity = result[key]['id']
        if not isinstance(identity, str) or not identity:
            raise ValueError('Invalid ' + key + ' id in response.')
        return identity

    def _id_call(self, method, params, key):
        result = self._call(method, params)
        try:
            return self._identity(result, key)
        except (KeyError, TypeError, ValueError) as exc:
            self._fatal('Codex app-server protocol error: ' + str(exc))
            raise CodexAppServerError('Invalid ' + method + ' response.') from exc

    def list_models(self):
        """Discovery only: at most 200 entries and 20 seconds across all pages."""
        entries = []
        params = {}
        cursors = set()
        deadline = time.monotonic() + 20
        timeout = self.CALL_TIMEOUT
        try:
            for _ in range(20):
                self.CALL_TIMEOUT = min(timeout, max(0, deadline - time.monotonic()))
                if self.CALL_TIMEOUT <= 0: raise TimeoutError('Model discovery timed out.')
                result = self._call('model/list', params)
                data = result.get('data')
                if not isinstance(data, list) or not all(isinstance(item, dict) for item in data):
                    raise CodexAppServerError('Invalid model list.')
                entries.extend(data[:200 - len(entries)])
                cursor = result.get('nextCursor')
                if len(entries) >= 200 or cursor is None: break
                if not isinstance(cursor, str) or not cursor or len(cursor) > 4096 or cursor in cursors:
                    raise CodexAppServerError('Invalid model cursor.')
                cursors.add(cursor)
                params = {'cursor': cursor}
            return [entry for entry in entries if not entry.get('hidden')]
        finally:
            self.CALL_TIMEOUT = timeout

    def start_thread(self, sandbox, approval_policy, developer_instructions, mcp_servers=None):
        if sandbox not in ('read-only', 'workspace-write') or approval_policy not in ('on-request', 'untrusted', 'never'):
            raise ValueError('Invalid sandbox or approval policy.')
        if not isinstance(developer_instructions, str):
            raise ValueError('Developer instructions must be text.')
        params = {'cwd': self.cwd, 'sandbox': sandbox, 'approvalPolicy': approval_policy,
                  'developerInstructions': developer_instructions}
        if mcp_servers is not None:
            if not isinstance(mcp_servers, dict):
                raise ValueError('MCP servers must be a configuration map.')
            servers = {}
            for name, config in mcp_servers.items():
                if not isinstance(name, str) or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_-]{0,63}', name) or not isinstance(config, dict):
                    raise ValueError('Invalid MCP server.')
                if set(config) != {'command', 'args', 'default_tools_approval_mode'} or not isinstance(config['command'], str) or not config['command']:
                    raise ValueError('Invalid MCP server configuration.')
                if not isinstance(config['args'], list) or not all(isinstance(arg, str) for arg in config['args']) or config['default_tools_approval_mode'] != 'approve':
                    raise ValueError('Invalid MCP arguments or approval mode.')
                servers[name] = {**config, 'args': list(config['args'])}
            params['config'] = {'mcp_servers': servers}
        return self._id_call('thread/start', params, 'thread')

    def resume_thread(self, thread_id):
        return self._id_call('thread/resume', {'threadId': thread_id}, 'thread')

    def start_turn(self, thread_id, text):
        if not isinstance(text, str):
            raise ValueError('Turn input must be text.')
        return self._id_call('turn/start', {'threadId': thread_id, 'input': [{'type': 'text', 'text': text}]}, 'turn')

    def steer(self, thread_id, turn_id, text):
        if not isinstance(text, str):
            raise ValueError('Steering input must be text.')
        result = self._call('turn/steer', {'threadId': thread_id, 'expectedTurnId': turn_id,
                                           'input': [{'type': 'text', 'text': text}]})
        if not isinstance(result.get('turnId'), str) or not result['turnId']:
            self._fatal('Codex app-server protocol error: invalid steering response.')
            raise CodexAppServerError('Invalid steering response.')
        return result

    def interrupt(self, thread_id, turn_id):
        key = (thread_id, turn_id)
        with self._lock:
            if self._closed or key in self._interrupts or key in self._finished:
                return
            self._interrupts.add(key)
            for identity, params in list(self._approvals.items()):
                if (params.get('threadId'), params.get('turnId')) == key:
                    self._answer(identity, 'cancel')
        try:
            self._call('turn/interrupt', {'threadId': thread_id, 'turnId': turn_id})
        except CodexAppServerError:
            with self._lock:
                self._interrupts.discard(key)
                finished = key in self._finished
            if not finished:
                raise
        except Exception:
            with self._lock:
                self._interrupts.discard(key)
            raise

    def _kill(self):
        with self._kill_lock:
            if self._killed or self.proc is None:
                return
            def signal_group(sig):
                try:
                    os.killpg(self.proc.pid, sig)
                except ProcessLookupError:
                    pass
                except PermissionError:
                    if self.proc.poll() is None:
                        raise
            try:
                signal_group(signal.SIGTERM)
                try:
                    self.proc.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    pass
                # Also terminate descendants after the leader has exited.
                signal_group(signal.SIGKILL)
                try:
                    self.proc.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    pass
            finally:
                self._killed = True

    def close(self):
        with self._close_lock:
            if self._cleanup_complete:
                return
            with self._lock:
                self._closed = True
                for pending in self._pending.values():
                    if not pending['done'].is_set():
                        pending['error'] = CodexAppServerError('Codex app-server is closed.')
                        pending['done'].set()
                for identity in list(self._approvals):
                    self._answer(identity, 'cancel')
            # A barrier lets cancellation replies reach the child before killing
            # it, without waiting for the human callback to return.
            if not self._killed and self.proc is not None:
                barrier = {'data': b'', 'done': threading.Event(), 'error': None}
                try:
                    self._writes.put_nowait(barrier)
                    barrier['done'].wait(.3)
                except queue.Full:
                    pass
                self._kill()
                for pipe in (self.proc.stdin, self.proc.stdout, self.proc.stderr):
                    try:
                        pipe.close()
                    except (OSError, ValueError):
                        pass
            for name in ('_reader', '_stderr_reader'):
                worker = getattr(self, name, None)
                if worker and worker is not threading.current_thread():
                    worker.join(timeout=.2)
            for channel in (self._writes, self._events):
                try:
                    channel.put_nowait(None)
                except queue.Full:
                    pass
            writer = getattr(self, '_writer', None)
            if writer and writer is not threading.current_thread():
                writer.join(timeout=.2)
            if self._event_thread is not threading.current_thread():
                self._event_thread.join(timeout=.2)
            self._cleanup_complete = True
