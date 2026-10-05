import json
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import patch
from backend.assistants import Assistants, command, child_environment, PARSERS, BUFFER_LIMIT, EVENT_LIMIT
from backend.files import Workspace

FIXTURE = str(Path(__file__).parent / 'fixtures' / 'assistant_cli.py')
ENV = {'INDYMAT_ASSISTANT_' + name: FIXTURE for name in ('CLAUDE', 'CODEX', 'AGY')}

class AssistantTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.folder = Path(self.tmp.name).resolve()
        self.workspace = Workspace(self.folder, self.folder)
        self.manager = Assistants(self.workspace, 'test-launch-secret')
        self.env = patch.dict(os.environ, ENV)
        self.env.start()

    def tearDown(self):
        self.manager.close()
        self.env.stop()
        self.tmp.cleanup()

    def wait(self, identity):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            events = self.manager.events(identity)
            if not events['running']: return events
            time.sleep(.02)
        self.fail('Assistant fixture did not finish')

    def start(self, provider='claude', prompt='hello', **extra):
        return self.manager.start({'provider': provider, 'prompt': prompt, **extra})

    def test_context_note_opens_a_conversation_and_is_not_repeated(self):
        for provider in ('claude', 'agy'):
            first = self.start(provider, 'echo-context')
            text = ''.join(e.get('text', '') for e in self.wait(first['session'])['events'] if e['type'] == 'text')
            self.assertIn('You are running inside IndyMAT', text, provider)
            self.assertIn('read-only', text, provider)
            self.assertIn('End of IndyMAT context.\n\necho-context', text, provider)
            self.assertNotIn('test-launch-secret', text)
            after = self.manager.events(first['session'])['after']
            self.start(provider, 'echo-context', conversation=first['session'])
            again = ''.join(e.get('text', '') for e in self.wait(first['session'])['events'] if e['type'] == 'text' and e['id'] > after)
            self.assertNotIn('You are running inside IndyMAT', again, provider)
            self.assertIn('echo-context', again, provider)

    def test_argv(self):
        for provider in PARSERS:
            for mode in ('read-only', 'edit'):
                argv = command(provider, mode, FIXTURE, self.folder)
                self.assertEqual(argv[0], FIXTURE)
                self.assertFalse(any('danger' in word or 'bypass' in word or 'skip-permission' in word for word in argv))
                flag = '--sandbox' if provider == 'codex' else '--permission-mode' if provider == 'claude' else '--mode'
                self.assertEqual(argv[argv.index(flag)+1], {'codex': ['read-only', 'workspace-write'], 'claude': ['plan', 'acceptEdits'], 'agy': ['plan', 'accept-edits']}[provider][mode == 'edit'])
                resumed = command(provider, mode, FIXTURE, self.folder, 'fixture-id')
                self.assertIn('fixture-id', resumed)
        for bad in ('--dangerously-bypass-approvals-and-sandbox', 'a b', '', 'foo\nbar'):
            with self.assertRaises(ValueError): command('claude', 'edit', FIXTURE, self.folder, bad)

    def test_prompt_only_stdin_and_validation(self):
        from backend import assistants
        real = assistants.subprocess.Popen
        launches = []
        def capture(argv, **kwargs):
            launches.append((argv, kwargs))
            return real(argv, **kwargs)
        with patch.object(assistants.subprocess, 'Popen', side_effect=capture):
            identity = self.start(prompt='--dangerously-skip-permissions $(touch bad) test-launch-secret')['session']
            self.wait(identity)
        self.assertNotIn('test-launch-secret', str(launches))
        self.assertNotIn('$(touch bad)', str(launches))
        self.assertIsInstance(launches[0][0], list)
        self.assertNotIn('shell', launches[0][1])
        for request in ({'provider': 'codex', 'prompt': 'a', 'argv': ['--bad']}, {'provider': 'codex', 'prompt': 'a', 'mode': 'danger-full-access'}, {'provider': ['codex'], 'prompt': 'a'}, {'provider': 'codex', 'prompt': 'x'*128001}):
            with self.assertRaises(ValueError): self.manager.start(request)
        with self.assertRaises(ValueError): self.start(conversation='--resume')

    def test_environment(self):
        result = child_environment({'PATH': '/bin', 'HOME': '/Users/test', 'MF_TOKEN': 'secret', 'LAUNCH_TOKEN': 'secret', 'XDG_CACHE_HOME': '/Users/test/.matlab-free', 'LANG': 'secret', 'CLAUDECODE': '1', 'OTHER': 'ignored', 'CODEX_HOME': '/Users/test/.codex', 'CLAUDE_CONFIG_DIR': '/Users/test/.matlab-free/config'}, 'secret')
        self.assertEqual(result, {'PATH': '/bin:/Users/test/.local/bin:/opt/homebrew/bin:/usr/local/bin', 'HOME': '/Users/test', 'CODEX_HOME': '/Users/test/.codex'})

    def test_parsers(self):
        for provider, parser in PARSERS.items():
            self.assertEqual(parser({'type': 'new.event', 'data': 'retained'})[0]['type'], 'raw', provider)
            self.assertEqual(parser({'type': 'error', 'message': 'fail'})[0]['type'], 'error')
        self.assertEqual(PARSERS['claude']({'type': 'assistant', 'message': {'content': [{'type': 'text', 'text': 'answer'}, {'type': 'tool_use', 'name': 'Read', 'input': {'path': 'a.m'}}, {'type': 'new.block'}]}})[0]['text'], 'answer')
        self.assertEqual(PARSERS['claude']({'type': 'stream_event', 'event': {'delta': {'type': 'text_delta', 'text': 'a'}}})[0]['text'], 'a')
        self.assertEqual(PARSERS['agy']({'type': 'text', 'text': 'answer'})[0]['text'], 'answer')

    def test_end_to_end_and_resume(self):
        for provider in PARSERS:
            identity = self.start(provider)['session']
            result = self.wait(identity)
            events = result['events']
            self.assertIn('Fixture answer', ''.join(e.get('text', '') for e in events if e['type'] == 'text'))
            self.assertTrue(any(e['type'] == 'tool' for e in events))
            self.assertTrue(any(e['type'] == 'reasoning' for e in events))
            self.assertTrue(any(e['type'] == 'raw' and e['text'] == '{unfinished' for e in events))
            self.assertEqual(events[-1]['state'], 'completed')
            self.assertTrue(events[-1]['conversation'])
            previous = result['after']
            self.start(provider, conversation=identity)
            self.wait(identity)
            newer = self.manager.events(identity, previous)
            self.assertTrue(all(e['id'] > previous for e in newer['events']))
            self.assertEqual(newer['events'][-1]['state'], 'completed')
            with self.assertRaises(ValueError): self.start(provider, mode='edit', conversation=identity)

    def test_stop_and_concurrency(self):
        identities = [self.start(prompt='wait')['session'] for _ in range(3)]
        with self.assertRaises(ValueError): self.start(prompt='wait')
        with self.assertRaises(ValueError): self.start(prompt='wait', conversation=identities[0])
        for identity in identities:
            self.assertEqual(self.manager.stop(identity), {'ok': True})
            self.manager.stop(identity)
            events = self.wait(identity)['events']
            self.assertEqual(events[-1]['state'], 'stopped')
            self.assertEqual(sum(e['type'] == 'turn-end' for e in events), 1)
            self.manager.stop(identity)

    def test_bounds_and_failure(self):
        identity = self.start(prompt='flood')['session']
        events = self.wait(identity)
        self.assertTrue(events['truncated'])
        self.assertLessEqual(len(events['events']), BUFFER_LIMIT + 1)
        self.assertEqual(events['events'][0]['type'], 'notice')
        identity = self.start(prompt='long')['session']
        events = self.wait(identity)['events']
        self.assertTrue(any(e['type'] == 'notice' for e in events))
        self.assertTrue(all(len(e.get('text', '')) <= EVENT_LIMIT for e in events))
        identity = self.start(prompt='fail')['session']
        self.assertEqual(self.wait(identity)['events'][-1]['state'], 'failed')
        self.manager.MAX_SESSIONS = len(self.manager.sessions)
        with self.assertRaises(ValueError): self.start()

    def test_context_and_boundaries(self):
        path = self.folder/'sample.m'
        path.write_text('disk')
        context = {'path': 'sample.m', 'dirty': True, 'content': 'unsaved private', 'selection': 'unsaved selection'}
        prompt = self.manager._prompt('question', context, self.folder)
        self.assertIn('sample.m', prompt)
        self.assertNotIn('unsaved private', prompt)
        self.assertNotIn('unsaved selection', prompt)
        prompt = self.manager._prompt('question', {**context, 'include_unsaved': True}, self.folder)
        self.assertIn('unsaved private', prompt)
        self.assertIn('unsaved selection', prompt)
        for path in ('../escape.m', '/etc/passwd', '.matlab-free/launch.json'):
            with self.assertRaises((ValueError, PermissionError)):
                self.manager._prompt('question', {'path': path}, self.folder)
        with tempfile.TemporaryDirectory() as external:
            (self.folder/'alias').symlink_to(external, target_is_directory=True)
            with self.assertRaises(PermissionError): self.manager._prompt('question', {'path': 'alias/a.m'}, self.folder)
        self.assertNotIn('test-launch-secret', self.manager._prompt('test-launch-secret', {}, self.folder))

    def test_providers_and_shutdown(self):
        self.assertTrue(all(item['available'] for item in self.manager.providers()))
        with patch.dict(os.environ, {'INDYMAT_ASSISTANT_AGY': '/no/such/program'}):
            self.assertFalse(next(item for item in self.manager.providers() if item['id'] == 'agy')['available'])
        identity = self.start(prompt='wait')['session']
        proc = self.manager.sessions[identity]['proc']
        self.manager.close()
        self.wait(identity)
        self.assertIsNotNone(proc.poll())
        with self.assertRaises(ValueError): self.start()



    def test_stop_kills_descendants(self):
        identity = self.start(prompt='descendant')['session']
        deadline = time.monotonic() + 2
        child = None
        while time.monotonic() < deadline:
            for event in self.manager.events(identity)['events']:
                if event['type'] == 'raw' and 'fixture.child' in event['text']:
                    child = json.loads(event['text'])['pid']
            if child: break
            time.sleep(.01)
        self.assertIsNotNone(child)
        heartbeat = self.folder / 'fixture-child-heartbeat'
        while not heartbeat.exists() and time.monotonic() < deadline: time.sleep(.01)
        self.assertTrue(heartbeat.exists())
        self.manager.stop(identity)
        # The child inherits stdout: EOF/turn-end also proves that its pipe was
        # closed. A surviving child would keep this turn running indefinitely.
        self.wait(identity)
        first = heartbeat.read_text()
        time.sleep(.1)
        self.assertEqual(heartbeat.read_text(), first, 'Descendant survived process-group stop')

    def test_incremental_stream(self):
        identity = self.start('claude', 'stream')['session']
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            result = self.manager.events(identity)
            if any(e.get('text') == 'First fragment' for e in result['events']): break
            time.sleep(.01)
        self.assertTrue(result['running'], 'first fragment is visible before turn end')
        cursor = result['after']
        self.wait(identity)
        self.assertTrue(all(e['id'] > cursor for e in self.manager.events(identity, cursor)['events']))

    def test_http_routes_without_sockets(self):
        # The lane sandbox forbids socket.bind, so exercise the actual Handler's
        # authentication and dispatch in memory; the .cjs tests cover real HTTP.
        import io
        from email.message import Message
        from types import SimpleNamespace
        from app import Handler
        app = SimpleNamespace(port=8769, token='test-launch-secret', assistants=self.manager,
                              file_lock=self.manager.lock, kernel=SimpleNamespace(lock=self.manager.lock))
        def request(endpoint, body=None, **headers):
            handler = Handler.__new__(Handler)
            handler.server = SimpleNamespace(app=app)
            handler.path = '/api/assistant/' + endpoint
            handler.headers = Message()
            values = {'Host': '127.0.0.1:8769', 'X-MF-Token': app.token, **headers}
            if body is not None:
                data = json.dumps(body).encode()
                values['Content-Length'] = str(len(data))
                handler.rfile = io.BytesIO(data)
            for key, value in values.items(): handler.headers[key] = value
            results = []
            handler.send = lambda status, data, *args, **kwargs: results.append((status, data))
            handler.handle_request(body is not None)
            return results[0]
        routes = [('providers', None), ('start', {'provider': 'codex', 'prompt': 'hello'}),
                  ('events?session=nope', None), ('stop', {'session': 'nope'})]
        for endpoint, body in routes:
            for bad in ({'Host': 'evil.invalid'}, {'Origin': 'https://evil.invalid'},
                        {'X-MF-Token': 'bad'}, {'Sec-Fetch-Site': 'cross-site'}):
                self.assertEqual(request(endpoint, body, **bad)[0], 403)
        self.assertEqual(request('providers')[0], 200)
        status, data = request('start', {'provider': 'codex', 'prompt': 'hello'})
        self.assertEqual(status, 202)
        self.wait(data['session'])
        status, result = request('events?session=' + data['session'])
        self.assertEqual(status, 200)
        self.assertEqual(result['events'][-1]['state'], 'completed')
        self.assertEqual(request('stop', {'session': data['session']})[0], 200)
        self.assertEqual(request('start', {'provider': 'codex', 'prompt': 'a', 'flags': []})[0], 400)
        self.assertEqual(request('start', {'provider': 'codex', 'prompt': 'a', 'context': {'path': '../a.m'}})[0], 403)

class MeasuredStreams(unittest.TestCase):
    """Event shapes recorded from the real programs on 5 Oct 2026 (identifiers shortened)."""
    def test_antigravity_measured_events(self):
        from backend.assistants import parse_agy, command
        events = [
            {'event': 'init', 'conversation_id': 'c1', 'init': {'cwd': '/x', 'tools': ['run_command'], 'permission_mode': 'request-review'}},
            {'event': 'step_update', 'step_update': {'conversation_id': 'c1', 'step_index': 0, 'state': 'DONE', 'step_type': 'user_input'}},
            {'event': 'step_update', 'step_update': {'conversation_id': 'c1', 'step_index': 1, 'state': 'ACTIVE', 'step_type': 'agent_response', 'text_delta': 'Hi'}},
            {'event': 'step_update', 'step_update': {'conversation_id': 'c1', 'step_index': 2, 'state': 'ACTIVE', 'step_type': 'tool', 'tool_name': 'run_command', 'tool_info': {'name': 'run_command', 'parameters': {'CommandLine': 'ls -la'}}}},
            {'event': 'step_update', 'step_update': {'conversation_id': 'c1', 'step_index': 2, 'state': 'DONE', 'step_type': 'tool', 'tool_name': 'run_command'}},
            {'event': 'result', 'result': {'conversation_id': 'c1', 'status': 'SUCCESS', 'response': '', 'denied_actions': [{'action': 'command', 'display_name': 'RunCommand'}]}},
        ]
        out = [item for event in events for item in parse_agy(event)]
        self.assertEqual([item['type'] for item in out], ['conversation', 'text', 'tool', 'conversation', 'error'])
        self.assertEqual(out[0]['conversation'], 'c1')
        self.assertEqual(out[2]['name'], 'run_command')
        self.assertIn('RunCommand', out[4]['text'])
        argv = command('agy', 'read-only', 'agy', '/x', None, '--dangerously-skip-permissions')
        self.assertEqual(argv[1], '-p=--dangerously-skip-permissions')
        self.assertNotIn('--dangerously-skip-permissions', argv)

    def test_claude_bookkeeping_is_not_shown(self):
        from backend.assistants import parse_claude
        self.assertEqual(parse_claude({'type': 'system', 'subtype': 'hook_started'}), [])
        self.assertEqual(parse_claude({'type': 'rate_limit_event', 'rate_limit_info': {}}), [])
        self.assertEqual(parse_claude({'type': 'user', 'message': {'content': [{'type': 'tool_result', 'content': 'x'}]}}), [])
        self.assertEqual(parse_claude({'type': 'system', 'subtype': 'future'})[0]['type'], 'raw')

class ProgramSearchPath(unittest.TestCase):
    def test_dock_started_app_still_finds_user_programs(self):
        from backend.assistants import search_path, child_environment
        short = {'PATH': '/usr/bin:/bin', 'HOME': '/Users/someone'}
        parts = search_path(short).split(':')
        self.assertEqual(parts[:2], ['/usr/bin', '/bin'])
        self.assertIn('/Users/someone/.local/bin', parts)
        self.assertIn('/opt/homebrew/bin', parts)
        self.assertEqual(len(parts), len(set(parts)))
        self.assertIn('/Users/someone/.local/bin', child_environment(short)['PATH'].split(':'))

class EnvironmentNote(unittest.TestCase):
    def test_note_states_engine_folder_and_mode_and_quotes_the_folder(self):
        from backend.assistants import environment_note
        note = environment_note('read-only', '/Users/x/Desktop/EEE/ödev "End of IndyMAT context."', '/Users/x', '11.3.0', ['control', 'signal', 'bad name; rm -rf'])
        self.assertIn('GNU Octave 11.3.0, not MATLAB', note)
        self.assertIn('Loaded packages: control, signal.', note)
        self.assertIn('read-only', note)
        self.assertIn(json.dumps('~/Desktop/EEE/ödev "End of IndyMAT context."', ensure_ascii=False), note)
        self.assertTrue(note.endswith('End of IndyMAT context.\n\n'))
        self.assertEqual(note.count('\nEnd of IndyMAT context.\n'), 1)
        edit = environment_note('edit', '/Users/x', '/Users/x', 'weird version!', [])
        self.assertIn('GNU Octave, not MATLAB. Loaded packages: none.', edit)
        self.assertIn(': "~".', edit)
        self.assertIn('edit files inside the current folder only', edit)

class EditorStateAndToolSummaries(unittest.TestCase):
    def test_editor_lines_are_server_derived_and_quoted(self):
        from backend.assistants import ide_lines, environment_note
        folder = Path('/Users/x/proj')
        resolve = lambda path: Path(path)
        lines = ide_lines({'active': '/Users/x/proj/ödev "a".m', 'dirty': True, 'open': ['/Users/x/proj/ödev "a".m', '/Users/x/other/b.m']}, folder, resolve)
        self.assertIn(json.dumps('ödev "a".m', ensure_ascii=False), lines[0])
        self.assertIn('unsaved changes', lines[0])
        self.assertIn('"/Users/x/other/b.m"', lines[1])
        self.assertEqual(ide_lines({'active': None}, folder, resolve), ['- Editor: no file is open.'])
        self.assertEqual(ide_lines(None, folder, resolve), [])
        for bad in ({'active': 5}, {'active': 'a', 'open': 'x'}, {'active': 'a', 'extra': 1}, {'active': '/Users/x/proj/.matlab-free/launch.json'}):
            with self.assertRaises((ValueError, PermissionError)): ide_lines(bad, folder, resolve)
        later = environment_note('edit', folder, '/Users/x', editor=lines, first=False)
        self.assertNotIn('You are running inside IndyMAT', later)
        self.assertIn('Active editor file', later)
        self.assertTrue(later.endswith('End of IndyMAT context.\n\n'))
        self.assertEqual(environment_note('edit', folder, '/Users/x', first=False), '')
        self.assertIn('small targeted edits', environment_note('edit', folder, '/Users/x'))
        self.assertNotIn('small targeted edits', environment_note('read-only', folder, '/Users/x'))

    def test_tool_arguments_show_edits_not_file_contents(self):
        from backend.assistants import compact_input, parse_claude
        big = 'line\n' * 500
        self.assertEqual(compact_input('Write', {'file_path': '/a/b.m', 'content': big}), {'file_path': '/a/b.m', 'content_lines': 500})
        self.assertEqual(compact_input('Edit', {'file_path': '/a/b.m', 'old_string': 'x = 1;', 'new_string': '% set\nx = 1;'}), {'added': 2, 'removed': 1, 'file_path': '/a/b.m'})
        self.assertEqual(compact_input('Bash', {'command': 'ls'}), {'command': 'ls'})
        self.assertEqual(compact_input('Write', {'file_path': '/a/b.m', 'content': 'x = 1;\n'}), {'file_path': '/a/b.m', 'content_lines': 1})
        event = parse_claude({'type': 'assistant', 'message': {'content': [{'type': 'tool_use', 'name': 'Write', 'input': {'file_path': '/a/b.m', 'content': big}}]}})[0]
        self.assertLess(len(event['text']), 200)

    def test_editor_state_reaches_every_turn(self):
        tests = AssistantTests('test_argv'); tests.setUp()
        try:
            target = tests.folder / 'acik.m'; target.write_text('x = 1;\n')
            first = tests.manager.start({'provider': 'claude', 'prompt': 'echo-context', 'ide': {'active': str(target), 'dirty': False, 'open': [str(target)]}})
            text = ''.join(e.get('text', '') for e in tests.wait(first['session'])['events'] if e['type'] == 'text')
            self.assertIn('Active editor file', text); self.assertIn('"acik.m"', text)
            after = tests.manager.events(first['session'])['after']
            tests.manager.start({'provider': 'claude', 'prompt': 'echo-context', 'conversation': first['session'], 'ide': {'active': str(target), 'dirty': True, 'open': []}})
            again = ''.join(e.get('text', '') for e in tests.wait(first['session'])['events'] if e['type'] == 'text' and e['id'] > after)
            self.assertIn('"acik.m"', again); self.assertIn('unsaved changes', again); self.assertNotIn('You are running inside IndyMAT', again)
        finally: tests.tearDown()

if __name__ == '__main__': unittest.main()
