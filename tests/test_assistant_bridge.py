"""Real stdio pipes, capability HTTP boundary and persistent Octave integration."""
import base64
import http.client
import json
import os
from pathlib import Path
import selectors
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
import tomllib
import unittest
from unittest.mock import patch

import app as application
from backend.assistant_bridge_service import CODE_BYTES, IMAGE_BYTES, INSPECT_TOOLS, RUN_TOOLS, TEXT_LIMIT, VARIABLE_LIMIT
from backend.assistants import command, environment_note

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = str(ROOT / 'tests/fixtures/assistant_cli.py')


class BridgeArguments(unittest.TestCase):
    def test_modes_escaping_resume_and_notes(self):
        bridge = {key: '/private/a "quoted" folder/ö\\file\n' + key for key in ('python', 'script', 'capability', 'config')}
        for provider in ('claude',):
            for mode in ('read-only', 'edit'):
                for level in ('none', 'inspect', 'run'):
                    for conversation in (None, 'fixture-thread'):
                        argv = command(provider, mode, 'agent', '/tmp/work', conversation, session_access=level, bridge=bridge)
                        self.assertNotIn('token', argv)
                        if level == 'none':
                            self.assertNotIn('--mcp-config', argv)
                            self.assertNotIn('-c', argv)
                            continue
                        if provider == 'claude':
                            self.assertEqual(argv[argv.index('--permission-mode') + 1], 'default' if mode == 'read-only' else 'acceptEdits')
                            self.assertEqual(argv[argv.index('--mcp-config') + 1], bridge['config'])
                            allowed = argv[argv.index('--allowedTools') + 1].split(',')
                            tools = RUN_TOOLS if level == 'run' else INSPECT_TOOLS
                            self.assertEqual(allowed, (['Read', 'Glob', 'Grep'] if mode == 'read-only' else []) + ['mcp__indymat__' + name for name in tools])
        with self.assertRaises(ValueError): command('agy', 'edit', 'agent', '/tmp', session_access='run', bridge=bridge)
        self.assertIn('Code cannot be run', environment_note('read-only', '/tmp', '/', session_access='inspect'))
        self.assertIn("user's LIVE", environment_note('edit', '/tmp', '/', session_access='run'))
        self.assertIn('cannot reach', environment_note('read-only', '/tmp', '/'))


class DirectBridge(unittest.TestCase):
    """Production completion adapter + real engine, also runnable without sockets."""
    @classmethod
    def setUpClass(cls):
        from backend.files import Workspace
        from backend.assistants import Assistants
        from backend.assistant_bridge_service import BridgeService
        from types import SimpleNamespace
        cls.temp = tempfile.TemporaryDirectory(prefix='indymat-bridge-direct-')
        cls.root = Path(cls.temp.name)
        shutil.copytree(ROOT / 'octave', cls.root / 'octave')
        cls.work = cls.root / 'work'
        cls.work.mkdir()
        cls.app = SimpleNamespace(runtime=cls.root / '.matlab-free', base='http://127.0.0.1:9999', token='direct-launch-secret', port=9999, workspace=Workspace(cls.work, cls.root), file_lock=threading.Lock(), finish_publish=lambda kernel: None, discard_publish_stages=lambda *args: None)
        cls.app.runtime.mkdir(mode=0o700)
        cls.service = BridgeService(cls.app)
        cls.app.assistant_bridge = cls.service
        cls.app.kernel = application.PublishKernel(cls.root, cls.app.runtime / 'jobs', cls.work, publish_app=cls.app)
        cls.app.assistants = Assistants(cls.app.workspace, cls.app.token, describe=cls.app.kernel.snapshot, bridge=cls.service)
        cls.idle()

    @classmethod
    def tearDownClass(cls):
        cls.app.assistants.close()
        cls.app.kernel.close()
        cls.temp.cleanup()

    @classmethod
    def idle(cls):
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            state = cls.app.kernel.snapshot()
            if state['status'] in ('idle', 'dead'): return state
            time.sleep(.025)
        raise AssertionError('Engine timeout')

    def grant(self, level='run'):
        import uuid
        identity = uuid.uuid4().hex
        paths = self.service.create(identity, 'claude', level)
        token = json.loads(Path(paths['capability']).read_text())['token']
        self.addCleanup(self.service.revoke, identity)
        return self.service.authenticate(token), paths

    def setUp(self):
        self.idle()

    def call(self, grant, tool, **arguments):
        return self.service.call(grant, {'name': tool, 'arguments': arguments})

    def payload(self, value): return json.loads(value['content'][0]['text'])

    def test_real_job_typed_read_completion_race_and_unchanged_session(self):
        grant, _ = self.grant()
        epoch = self.app.kernel.generation
        result = self.call(grant, 'run_code', code='direct_live=[17 29]; direct_keep=@sin; disp(direct_live);')
        self.assertFalse(result['isError'], result)
        job = self.payload(result)['job']
        self.assertIn('17', self.payload(result)['output'])
        value = self.call(grant, 'variable_value', name='direct_live')
        self.assertFalse(value['isError'], value)
        self.assertEqual(self.payload(value)['value']['rows'], [['17', '29']])
        self.assertTrue(self.call(grant, 'variable_value', name='direct_live;clear all')['isError'])
        self.assertEqual(self.payload(self.call(grant, 'job_result', job=job))['job'], job)
        other, _ = self.grant()
        self.assertTrue(self.call(other, 'job_result', job=job)['isError'])
        self.assertEqual(self.app.kernel.generation, epoch)
        self.app.kernel.submit('assert(direct_keep(pi/2)==1);')
        self.assertFalse(self.idle()['error'])
        # Completion is captured at the next submission even without a poll.
        original = self.service._wait
        with patch.object(self.service, '_wait', side_effect=lambda g, j, seconds=10: original(g, j, .01)):
            pending = self.payload(self.call(grant, 'run_code', code='pause(.2); direct_later=71;'))
        time.sleep(.4)
        self.app.kernel.submit('assert(direct_later==71);')
        self.idle()
        self.assertEqual(self.payload(self.call(grant, 'job_result', job=pending['job']))['status'], 'idle')
        visible = self.service.visible_jobs(self.app.kernel)
        self.assertTrue(any(entry['code'].startswith('direct_live=') and entry['provider'] == 'claude' for entry in visible))

    def test_inspect_busy_paused_waiting_output_code_and_variable_bounds(self):
        grant, _ = self.grant()
        inspect, _ = self.grant('inspect')
        self.assertTrue(self.call(inspect, 'run_code', code='bad=1;')['isError'])
        self.assertTrue(self.call(grant, 'run_code', code='z' * (CODE_BYTES + 1))['isError'])
        self.assertTrue(self.call(grant, 'run_code', code='🙂' * CODE_BYTES)['isError'])
        self.app.kernel.submit('pause(.3);')
        self.assertTrue(self.call(grant, 'run_code', code='bad=1;')['isError'])
        self.assertTrue(self.call(inspect, 'variable_value', name='direct_live')['isError'])
        self.idle()
        for updates in ({'status': 'paused'}, {'waiting_input': True}):
            with self.app.kernel.lock:
                saved = self.app.kernel.state.copy()
                self.app.kernel.state.update(updates)
            try: self.assertTrue(self.call(grant, 'run_code', code='bad=1;')['isError'])
            finally:
                with self.app.kernel.lock: self.app.kernel.state = saved
        result = self.call(grant, 'run_code', code="fprintf('%s',repmat('q',1,30000));")
        self.assertIn('truncated', result['content'][0]['text'])
        packed = self.payload(result)
        self.assertIn('job', packed)
        self.assertIn('error', packed)
        self.assertIn('figures', packed)
        self.assertLess(len(result['content'][0]['text']), TEXT_LIMIT + 100)
        with self.app.kernel.lock:
            original = self.app.kernel.state['variables']
            self.app.kernel.state['variables'] = [{'name': 'v' + str(i), 'size': [1, 1], 'class': 'double'} for i in range(VARIABLE_LIMIT + 1)]
        try: self.assertIn('truncated', self.call(inspect, 'workspace_variables')['content'][0]['text'])
        finally:
            with self.app.kernel.lock: self.app.kernel.state['variables'] = original
        self.assertTrue(self.call(grant, 'run_code', code="error('expected bridge failure');")['isError'])

    def test_figure_read_and_bounded_image(self):
        import uuid
        grant, _ = self.grant()
        # Explicit PNG fixture tests the byte/auth boundary; RealBridge below
        # exercises Octave's real plot/export path under the graphical build.
        job = uuid.uuid4().hex
        folder = self.app.kernel.runtime / job
        folder.mkdir()
        file = folder / 'figure-1.png'
        data = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jL1sAAAAASUVORK5CYII=')
        file.write_bytes(data)
        with self.app.kernel.lock:
            original = self.app.kernel.state['figures']
            self.app.kernel.state['figures'] = [{'job': job, 'file': file.name, 'number': 1, 'name': 'PNG fixture'}]
        try:
            image = self.call(grant, 'figure_image')['content'][0]
            self.assertEqual(image['mimeType'], 'image/png')
            self.assertEqual(base64.b64decode(image['data']), data)
            file.write_bytes(b'q' * (IMAGE_BYTES + 1))
            self.assertIn('size limit', self.call(grant, 'figure_image')['content'][0]['text'])
            self.assertTrue(self.call(grant, 'figure_image', index=True)['isError'])
        finally:
            with self.app.kernel.lock: self.app.kernel.state['figures'] = original
            shutil.rmtree(folder)

    def test_handler_boundary_modes_revocation_and_shutdown(self):
        import email.message
        import io
        grant, paths = self.grant('inspect')
        token = grant['token']
        for path in (paths['capability'], paths['config']): self.assertEqual(stat.S_IMODE(Path(path).stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(Path(paths['config']).parent.stat().st_mode), 0o700)
        def route(path, bridge_token='', browser_token='', origin=None, host=None, body=None):
            handler = object.__new__(application.Handler)
            handler.server = type('Server', (), {'app': self.app})()
            handler.path = path
            handler.headers = email.message.Message()
            handler.headers['Host'] = host or '127.0.0.1:9999'
            handler.headers['X-IndyMAT-Bridge'] = bridge_token
            handler.headers['X-MF-Token'] = browser_token
            if origin is not None: handler.headers['Origin'] = origin
            content = json.dumps(body or {}).encode()
            handler.headers['Content-Length'] = str(len(content))
            handler.rfile = io.BytesIO(content)
            responses = []
            handler.send = lambda status, data, *args: responses.append((status, data))
            handler.handle_request(body is not None)
            return responses[0]
        self.assertEqual(route('/api/bridge/tools', token, body={})[0], 200)
        self.assertEqual(route('/api/bridge/tools', self.app.token, self.app.token, body={})[0], 403)
        self.assertEqual(route('/api/bridge/tools', token, origin='', body={})[0], 403)
        self.assertEqual(route('/api/bridge/tools', token, origin=self.app.base, body={})[0], 403)
        self.assertEqual(route('/api/bridge/tools', token, host='evil.invalid', body={})[0], 403)
        self.assertEqual(route('/api/bridge/call', token, body={'name': 'run_code', 'arguments': {'code': 'bad=1;'}})[1]['isError'], True)
        for path in ('/api/state', '/api/files', '/api/execute', '/api/variable'):
            self.assertEqual(route(path, token, token, body={})[0], 403)
        self.service.revoke(grant['identity'])
        self.assertEqual(route('/api/bridge/tools', token, body={})[0], 403)
        self.assertFalse(Path(paths['capability']).exists())
        self.assertTrue(self.call(grant, 'session_status')['isError'])
        # Real manager owns grant lifetime even when a CLI turn has ended.
        with patch.dict(os.environ, {'INDYMAT_ASSISTANT_CODEX': FIXTURE}):
            identity = self.app.assistants.start({'provider': 'codex', 'session_access': 'inspect', 'prompt': 'hello'})['session']
        paths = self.app.assistants.sessions[identity]['bridge']
        credential = json.loads(Path(paths['capability']).read_text())
        self.app.assistants.close()
        self.assertFalse(Path(paths['capability']).exists())
        self.assertFalse(Path(paths['config']).exists())
        with self.assertRaises(PermissionError): self.service.authenticate(credential['token'])
        # Keep the class's manager usable for other alphabetically later tests.
        from backend.assistants import Assistants
        self.app.assistants = Assistants(self.app.workspace, self.app.token, describe=self.app.kernel.snapshot, bridge=self.service)

    def test_stop_retained_results_and_epoch_rejection(self):
        grant, _ = self.grant()
        epoch = self.app.kernel.generation
        original = self.service._wait
        with patch.object(self.service, '_wait', side_effect=lambda g, j, seconds=10: original(g, j, .05)):
            pending = self.payload(self.call(grant, 'run_code', code='pause(5); direct_should_not_finish=1;'))
        self.app.kernel.interrupt()
        self.app.kernel.interrupt()
        self.idle()
        result = self.call(grant, 'job_result', job=pending['job'])
        self.assertTrue(result['isError'], result)
        self.assertIn('stopped', self.payload(result)['error'])
        self.assertEqual(self.app.kernel.generation, epoch)
        with self.app.kernel.lock: self.app.kernel.generation += 1
        try:
            stale = self.call(grant, 'job_result', job=pending['job'])
            self.assertTrue(stale['isError'])
            self.assertIn('session changed', stale['content'][0]['text'])
        finally:
            with self.app.kernel.lock: self.app.kernel.generation = epoch

    def test_stdio_survives_bad_input_and_refused_http(self):
        grant, paths = self.grant()
        proc = subprocess.Popen([sys.executable, str(ROOT / 'backend/assistant_bridge.py'), paths['capability']], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        messages = [
            'malformed', '[]',
            json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {'protocolVersion': '2025-03-26'}}),
            json.dumps({'jsonrpc': '2.0', 'method': 'notifications/initialized'}),
            json.dumps({'jsonrpc': '2.0', 'id': 2, 'method': 'ping'}),
            json.dumps({'jsonrpc': '2.0', 'id': 3, 'method': 'missing'}),
            json.dumps({'jsonrpc': '2.0', 'id': 4, 'method': 'tools/call', 'params': {'name': 'session_status'}}),
            json.dumps({'jsonrpc': '2.0', 'id': 1.5, 'method': 'ping'}),
            '{"jsonrpc":"2.0","id":NaN,"method":"ping"}',
            'x' * 150_010,
            json.dumps({'jsonrpc': '2.0', 'id': 6, 'method': 'ping'}),
        ]
        output, errors = proc.communicate(('\n'.join(messages) + '\n').encode(), timeout=20)
        replies = [json.loads(line) for line in output.splitlines()]
        self.assertEqual(proc.returncode, 0, errors)
        self.assertEqual([r.get('error', {}).get('code') for r in replies[:2]], [-32700, -32600])
        self.assertEqual(replies[2]['result']['protocolVersion'], '2025-03-26')
        self.assertEqual(replies[3]['result'], {})
        self.assertEqual(replies[4]['error']['code'], -32601)
        self.assertTrue(replies[5]['result']['isError'])
        self.assertEqual(replies[6]['id'], 1.5)
        self.assertEqual(replies[7]['error']['code'], -32700)
        self.assertEqual(replies[8]['error']['code'], -32700)
        self.assertEqual(replies[9]['result'], {})


class RealBridge(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='indymat-bridge-')
        cls.root = Path(cls.temp.name)
        shutil.copytree(ROOT / 'octave', cls.root / 'octave')
        cls.work = cls.root / 'work'
        cls.work.mkdir()
        cls.root_patch = patch.object(application, 'ROOT', cls.root)
        cls.env_patch = patch.dict(os.environ, {'INDYMAT_ASSISTANT_' + name: FIXTURE for name in ('CLAUDE', 'CODEX', 'AGY')})
        cls.root_patch.start()
        cls.env_patch.start()
        try:
            cls.app = application.App(cls.work, 0, False)
            cls.server_thread = threading.Thread(target=cls.app.server.serve_forever, daemon=True)
            cls.server_thread.start()
            cls.idle()
        except Exception:
            cls.root_patch.stop()
            cls.env_patch.stop()
            cls.temp.cleanup()
            raise

    @classmethod
    def tearDownClass(cls):
        cls.app.server.shutdown()
        cls.app.close()
        cls.root_patch.stop()
        cls.env_patch.stop()
        cls.temp.cleanup()

    @classmethod
    def idle(cls):
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            state = cls.app.kernel.snapshot()
            if state['status'] == 'idle': return state
            if state['status'] == 'dead': raise AssertionError(state)
            time.sleep(.025)
        raise AssertionError('Octave not idle')

    def request(self, route, token='', body=None, headers=None):
        connection = http.client.HTTPConnection('127.0.0.1', self.app.port, timeout=20)
        try:
            connection.request('POST' if body is not None else 'GET', route, json.dumps(body) if body is not None else None, {'X-IndyMAT-Bridge': token, **(headers or {})})
            response = connection.getresponse()
            return response.status, json.loads(response.read())
        finally: connection.close()

    def conversation(self, level='run', provider='codex', prompt='hello'):
        started = self.app.assistants.start({'provider': provider, 'session_access': level, 'prompt': prompt})
        identity = started['session']
        config = self.app.assistants.sessions[identity].get('bridge')
        self.addCleanup(lambda: self.app.assistants.remove(identity) if identity in self.app.assistants.sessions else None)
        deadline = time.monotonic() + 25
        while self.app.assistants.events(identity)['running'] and time.monotonic() < deadline: time.sleep(.025)
        self.assertFalse(self.app.assistants.events(identity)['running'])
        return identity, config

    def pipe(self, config):
        proc = subprocess.Popen([sys.executable, str(ROOT / 'backend/assistant_bridge.py'), config['capability']], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        def cleanup():
            proc.stdin.close()
            try: proc.wait(timeout=3)
            except subprocess.TimeoutExpired: proc.kill(); proc.wait()
            proc.stdout.close()
            proc.stderr.close()
        self.addCleanup(cleanup)
        return proc

    def rpc(self, proc, method, params=None, identity=1):
        proc.stdin.write((json.dumps({'jsonrpc': '2.0', 'id': identity, 'method': method, 'params': params or {}}) + '\n').encode())
        proc.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(proc.stdout, selectors.EVENT_READ)
            self.assertTrue(selector.select(20), 'Bridge response timeout')
        response = json.loads(proc.stdout.readline())
        self.assertEqual(response['jsonrpc'], '2.0')
        self.assertEqual(response['id'], identity)
        return response

    def call(self, proc, name, arguments=None):
        return self.rpc(proc, 'tools/call', {'name': name, 'arguments': arguments or {}})['result']

    def text(self, result): return result['content'][0]['text']

    def test_protocol_live_values_jobs_and_console(self):
        identity, config = self.conversation()
        proc = self.pipe(config)
        self.assertEqual(self.rpc(proc, 'initialize', {'protocolVersion': '2025-03-26'})['result']['protocolVersion'], '2025-03-26')
        proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        proc.stdin.flush()
        self.assertEqual(self.rpc(proc, 'ping')['result'], {})
        self.assertEqual(self.rpc(proc, 'unknown')['error']['code'], -32601)
        proc.stdin.write(b'bad json\n[]\n{"jsonrpc":"2.0","id":9,"method":"ping","params":[]}\n')
        proc.stdin.flush()
        for code in (-32700, -32600, -32602): self.assertEqual(json.loads(proc.stdout.readline())['error']['code'], code)
        listed = self.rpc(proc, 'tools/list')['result']['tools']
        self.assertEqual(tuple(tool['name'] for tool in listed), RUN_TOOLS)
        # Measured with the real claude and codex programs (5 Oct 2026): calls carry _meta bookkeeping.
        with_meta = self.rpc(proc, 'tools/call', {'name': 'session_status', 'arguments': {}, '_meta': {'progressToken': 7, 'claudecode/toolUseId': 'x'}})['result']
        self.assertFalse(with_meta.get('isError'), with_meta)
        self.assertEqual(len(self.rpc(proc, 'tools/list', {'cursor': None, '_meta': {}})['result']['tools']), len(RUN_TOOLS))
        status = json.loads(self.text(self.call(proc, 'session_status')))
        self.assertEqual(status['access_level'], 'run')
        self.assertTrue(status['octave_version'])
        home = self.app.workspace.home
        work = self.work.resolve()  # macOS: the temporary folder lives behind the /var -> /private/var link
        expected = '~/' + str(work.relative_to(home)) if work.is_relative_to(home) else str(work)
        self.assertEqual(status['current_folder'], expected)
        code = 'bridge_live = [17 29]; disp(bridge_live);'
        epoch = self.app.kernel.generation
        result = self.call(proc, 'run_code', {'code': code})
        self.assertFalse(result['isError'], result)
        job = json.loads(self.text(result))['job']
        self.assertIn('17', self.text(result))
        self.assertIn('bridge_live', self.text(self.call(proc, 'workspace_variables')))
        value = self.call(proc, 'variable_value', {'name': 'bridge_live'})
        self.assertFalse(value['isError'], value)
        self.assertEqual(json.loads(self.text(value))['value']['rows'], [['17', '29']])
        self.assertTrue(self.call(proc, 'variable_value', {'name': 'bridge_live;clear all'})['isError'])
        # An intervening typed read must not replace the original run's outcome.
        self.assertIn('29', self.text(self.call(proc, 'job_result', {'job': job})))
        status, state = self.request('/api/state', headers={'X-MF-Token': self.app.token})
        self.assertEqual(status, 200)
        visible = next(entry for entry in state['assistant_jobs'] if entry['job'] == job)
        self.assertEqual(visible['code'], code)
        self.assertEqual(visible['provider'], 'codex')
        self.assertEqual(visible['epoch'], epoch)
        self.assertEqual(self.app.kernel.generation, epoch)

    def test_refusals_limits_and_ownership(self):
        identity, config = self.conversation('inspect')
        proc = self.pipe(config)
        self.assertEqual(tuple(tool['name'] for tool in self.rpc(proc, 'tools/list')['result']['tools']), INSPECT_TOOLS)
        self.assertTrue(self.call(proc, 'run_code', {'code': 'inspect_escape=1;'})['isError'])
        self.assertTrue(self.call(proc, 'variable_value', {'name': 'x', 'extra': 1})['isError'])
        _, run_config = self.conversation()
        run = self.pipe(run_config)
        self.assertTrue(self.call(run, 'run_code', {'code': 'x' * (CODE_BYTES + 1)})['isError'])
        kernel = self.app.kernel
        kernel.submit('pause(1);')
        self.assertTrue(self.call(run, 'run_code', {'code': 'busy_escape=1;'})['isError'])
        self.assertTrue(self.call(proc, 'variable_value', {'name': 'bridge_live'})['isError'])
        self.idle()
        with kernel.lock:
            saved = kernel.state.copy()
            kernel.state['status'] = 'paused'
        try: self.assertTrue(self.call(run, 'run_code', {'code': 'paused_escape=1;'})['isError'])
        finally:
            with kernel.lock: kernel.state = saved
        # Boundary policy for waiting input without writing to stdin.
        with kernel.lock:
            saved = kernel.state.copy()
            kernel.state['waiting_input'] = True
        try: self.assertTrue(self.call(run, 'run_code', {'code': 'input_escape=1;'})['isError'])
        finally:
            with kernel.lock: kernel.state = saved
        # Bounded transport output is signalled, never an unbounded display call.
        result = self.call(run, 'run_code', {'code': "fprintf('%s',repmat('q',1,30000));"})
        self.assertIn('truncated', self.text(result))
        self.assertLess(len(self.text(result)), TEXT_LIMIT + 100)
        with kernel.lock:
            original = kernel.state['variables']
            kernel.state['variables'] = [{'name': 'v' + str(i), 'size': [1, 1], 'class': 'double'} for i in range(VARIABLE_LIMIT + 1)]
        try: self.assertIn('truncated', self.text(self.call(proc, 'workspace_variables')))
        finally:
            with kernel.lock: kernel.state['variables'] = original

    def test_security_permissions_revocation_and_none(self):
        identity, config = self.conversation('inspect')
        credential = json.loads(Path(config['capability']).read_text())
        token = credential['token']
        self.assertNotEqual(token, self.app.token)
        self.assertEqual(stat.S_IMODE(Path(config['capability']).stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(Path(config['config']).stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(Path(config['capability']).parent.stat().st_mode), 0o700)
        self.assertEqual(self.request('/api/bridge/tools', token, {})[0], 200)
        for headers in ({'Origin': self.app.base}, {'Origin': ''}, {'Origin': 'null'}, {'Host': 'evil.invalid'}, {'Sec-Fetch-Site': 'cross-site'}):
            self.assertEqual(self.request('/api/bridge/tools', token, {}, headers)[0], 403)
        self.assertEqual(self.request('/api/bridge/tools', self.app.token, {}, {'X-MF-Token': self.app.token})[0], 403)
        self.assertEqual(self.request('/api/bridge/tools', 'ö', {})[0], 403)
        for route in ('/api/state', '/api/files', '/api/execute', '/api/variable', '/api/assistant/start'):
            self.assertEqual(self.request(route, token, {} if route not in ('/api/state', '/api/files') else None, {'X-MF-Token': token})[0], 403)
        proc = self.pipe(config)
        self.app.assistants.remove(identity)
        self.assertFalse(Path(config['capability']).exists())
        self.assertFalse(Path(config['config']).exists())
        self.assertEqual(self.request('/api/bridge/tools', token, {})[0], 403)
        self.assertTrue(self.call(proc, 'session_status')['isError'])
        _, none = self.conversation('none')
        self.assertIsNone(none)

    def test_fake_agents_use_bridge_end_to_end_and_resume(self):
        for provider in ('claude', 'codex'):
            identity, config = self.conversation('run', provider, 'bridge-run')
            events = self.app.assistants.events(identity)['events']
            self.assertTrue(any(e['type'] == 'text' and '7741' in e['text'] for e in events), events)
            self.assertTrue(any(e['type'] == 'tool' and e['name'] == 'indymat · run_code' for e in events))
            self.assertTrue(any(v['name'] == 'bridge_fixture_' + provider for v in self.idle()['variables']))
            after = self.app.assistants.events(identity)['after']
            self.app.assistants.start({'provider': provider, 'session_access': 'run', 'conversation': identity, 'prompt': 'bridge-run'})
            deadline = time.monotonic() + 25
            while self.app.assistants.events(identity)['running'] and time.monotonic() < deadline: time.sleep(.025)
            self.assertTrue(any(e['type'] == 'text' and '7741' in e['text'] for e in self.app.assistants.events(identity)['events'] if e['id'] > after))
            with self.assertRaises(ValueError): self.app.assistants.start({'provider': provider, 'session_access': 'inspect', 'conversation': identity, 'prompt': 'hello'})

    def test_figures_and_timeout_then_job_result(self):
        _, config = self.conversation()
        proc = self.pipe(config)
        self.assertFalse(self.call(proc, 'run_code', {'code': 'figure(); plot(1:3);'})['isError'])
        image = self.call(proc, 'figure_image')
        self.assertEqual(image['content'][0]['mimeType'], 'image/png')
        self.assertTrue(base64.b64decode(image['content'][0]['data']).startswith(b'\x89PNG'))
        self.assertTrue(self.call(proc, 'figure_image', {'index': 999})['isError'])
        figure = self.app.kernel.snapshot()['figures'][0]
        file = self.app.kernel.runtime / figure['job'] / figure['file']
        data = file.read_bytes()
        try:
            file.write_bytes(b'x' * (IMAGE_BYTES + 1))
            self.assertIn('size limit', self.text(self.call(proc, 'figure_image')))
        finally: file.write_bytes(data)
        # Patch the wait at the service boundary rather than waiting ten seconds.
        service = self.app.assistant_bridge
        original = service._wait
        with patch.object(service, '_wait', side_effect=lambda grant, job, seconds=10: original(grant, job, .05)):
            result = self.call(proc, 'run_code', {'code': 'pause(.5); bridge_later=88;'})
        pending = json.loads(self.text(result))
        self.assertIn('still running', pending['message'])
        self.idle()
        self.assertEqual(json.loads(self.text(self.call(proc, 'job_result', {'job': pending['job']})))['status'], 'idle')
        self.app.kernel.submit('bridge_next = 1;')
        self.idle()
        self.assertEqual(json.loads(self.text(self.call(proc, 'job_result', {'job': pending['job']})))['status'], 'idle')


if __name__ == '__main__': unittest.main()
