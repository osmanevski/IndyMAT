"""Approval policy tests without a model, browser, sockets or user's Octave session."""
import collections
import io
import json
import os
from pathlib import Path
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from email.message import Message
from app import Handler
from backend.files import Workspace
from backend.assistants import Assistants, command, environment_note
from backend.assistant_bridge_service import BridgeService, INSPECT_TOOLS, RUN_TOOLS
from backend.assistant_approvals import preview, decision_key, DETAIL_BYTES, MAX_PENDING


class PreviewTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.folder = Path(self.temp.name).resolve()
        self.path = self.folder / 'x.m'
        self.path.write_text('x = 1;\nx = 1;\ny = 2;\n')
        self.addCleanup(self.temp.cleanup)

    def diff(self, tool, **value):
        return preview(tool, {'file_path': 'x.m', **value}, self.folder)[1]

    def test_write_edit_multiedit_and_no_final_newline(self):
        detail = self.diff('Write', content='new = 3;\n')
        self.assertIn('-y = 2;', detail['text'])
        self.assertIn('+new = 3;', detail['text'])
        detail = self.diff('Edit', old_string='x = 1;', new_string='x = 4;', replace_all=True)
        self.assertEqual(detail['text'].count('+x = 4;'), 2)
        detail = self.diff('MultiEdit', edits=[{'old_string': 'y = 2;', 'new_string': 'y = 8;'}, {'old_string': 'y = 8;', 'new_string': 'z = 9;'}])
        self.assertIn('+z = 9;', detail['text'])
        detail = self.diff('Write', content='no newline')
        self.assertIn('+no newline\n\\ No newline', detail['text'])
        self.assertEqual(self.path.read_text(), 'x = 1;\nx = 1;\ny = 2;\n')

    def test_unappliable_ambiguous_and_invalid_edits(self):
        for value in ({'old_string': 'missing', 'new_string': 'new'}, {'old_string': 'x = 1;', 'new_string': 'new'}, {'old_string': '', 'new_string': 'new'}, {'old_string': 'y = 2;', 'new_string': 'new', 'replace_all': 'yes'}):
            detail = self.diff('Edit', **value)
            self.assertIn('could not be applied', detail['text'])
            self.assertIn('Old text:', detail['text'])
            self.assertIn('New text:', detail['text'])
        self.assertIn('could not be applied', self.diff('MultiEdit', edits=[])['text'])
        self.path.write_text('a' * 100000)
        self.assertIn('could not be applied', self.diff('Edit', old_string='a', new_string='a' * 100000, replace_all=True)['text'])

    def test_bounds_missing_binary_private_outside_and_symlink(self):
        detail = self.diff('Write', content='ö\n' * 40000)
        self.assertLessEqual(len(detail['text'].encode()), DETAIL_BYTES)
        self.assertTrue(detail['truncated'])
        self.path.write_bytes(b'\xff\x00')
        self.assertIn('could not be applied', self.diff('Edit', old_string='x', new_string='y')['text'])
        self.path.write_bytes(b'x' * 1_100_000)
        self.assertIn('could not be applied', self.diff('Write', content='y')['text'])
        shown, detail = preview('Write', {'file_path': 'new.m', 'content': 'new\n'}, self.folder)
        self.assertEqual(shown, 'new.m')
        self.assertIn('+new', detail['text'])
        private = self.folder / '.matlab-free'; private.mkdir()
        (private / 'secret').write_text('private-secret')
        shown, detail = preview('Write', {'file_path': str(private / 'secret'), 'content': 'change'}, self.folder)
        self.assertEqual(shown, str(private / 'secret'))
        self.assertTrue(detail['warning'])
        self.assertNotIn('private-secret', detail['text'])
        (self.folder / 'link').symlink_to('/etc/hosts')
        shown, detail = preview('Write', {'file_path': 'link', 'content': 'change'}, self.folder)
        self.assertTrue(Path(shown).is_absolute())
        self.assertTrue(detail['warning'])
        command_detail = preview('Bash', {'command': 'echo hello', 'description': 'Say hello'}, self.folder)[1]
        self.assertEqual(command_detail['text'], 'Say hello\necho hello')
        self.assertTrue(preview('Other', {'x': 'ö' * 100000}, self.folder)[1]['truncated'])

    def test_precise_keys(self):
        self.assertEqual(decision_key('Edit', {'file_path': './x.m'}, self.folder), decision_key('Edit', {'file_path': str(self.path)}, self.folder))
        self.assertNotEqual(decision_key('Edit', {'file_path': 'x.m'}, self.folder), decision_key('Write', {'file_path': 'x.m'}, self.folder))
        self.assertNotEqual(decision_key('Bash', {'command': 'ls'}, self.folder), decision_key('Bash', {'command': 'ls '}, self.folder))
        self.assertEqual(decision_key('Other', {'x': 1}, self.folder), ('Other',))
        self.assertIsNone(decision_key('Edit', {}, self.folder))


class ServiceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.folder = Path(self.temp.name).resolve()
        self.workspace = Workspace(self.folder, self.folder)
        self.manager = Assistants(self.workspace, 'launch-private')
        self.app = SimpleNamespace(assistants=self.manager, runtime=self.folder / '.matlab-free', base='http://127.0.0.1:12345', port=12345, token='launch-private', kernel=SimpleNamespace(lock=threading.RLock()))
        self.app.runtime.mkdir()
        self.bridge = BridgeService(self.app)
        self.app.assistant_bridge = self.bridge
        self.manager.bridge = self.bridge
        self.service = self.bridge.approvals
        self.addCleanup(self.temp.cleanup)
        self.addCleanup(self.manager.close)
        self.manager._kill = lambda proc: None
        self.new_session('one')

    def new_session(self, identity):
        self.manager.sessions[identity] = {'provider': 'claude', 'mode': 'ask', 'folder': str(self.folder), 'session_access': 'none', 'turn': 'turn-' + identity, 'running': True, 'stopped': False, 'proc': object(), 'events': collections.deque(maxlen=512), 'seq': 0, 'bytes': 0, 'conversation': 'fixture'}
        self.bridge.create(identity, 'claude', 'none', approvals=True)
        return self.bridge.grants[identity]

    def pending(self, identity='one'):
        return [event for event in self.manager.events(identity)['events'] if event['type'] == 'approval']

    def start_request(self, tool='Write', value=None, identity='one', use='use-1'):
        value = {'file_path': 'x.m', 'content': 'new\n'} if value is None else value
        output = []
        def call():
            output.append(self.bridge.call(self.bridge.grants[identity], {'name': 'approve', 'arguments': {'tool_name': tool, 'input': value, 'tool_use_id': use}}))
        thread = threading.Thread(target=call)
        thread.start()
        def cleanup():
            self.service.cancel(identity, 'test cleanup')
            thread.join(2)
        self.addCleanup(cleanup)
        deadline = time.monotonic() + .2
        while not output and not any(event['id'] not in {item['id'] for item in getattr(self, 'seen', [])} for event in self.pending(identity)) and time.monotonic() < deadline: time.sleep(.005)
        self.seen = self.pending(identity)
        return thread, output

    def decide(self, event, decision='allow', identity='one', **extra):
        return self.service.decide({'session': identity, 'id': event['id'], 'decision': decision, **extra})

    @staticmethod
    def answer(output): return json.loads(output[0]['content'][0]['text'])

    def test_block_allow_exact_input_events_and_idempotent_route(self):
        value = {'file_path': 'x.m', 'content': 'ö' * 30000}
        thread, output = self.start_request(value=value)
        self.assertTrue(thread.is_alive()); self.assertFalse(output)
        event = self.pending()[0]
        self.assertIn('detail', event)
        handler = Handler.__new__(Handler); handler.server = SimpleNamespace(app=self.app)
        responses = []; handler.send = lambda status, body: responses.append((status, body))
        for decision in ('allow', 'deny'):
            handler.post('/api/assistant/approve', {'session': 'one', 'id': event['id'], 'decision': decision})
        self.assertEqual(responses[0], responses[1])
        thread.join(2)
        self.assertFalse(thread.is_alive())
        self.assertEqual(self.answer(output), {'behavior': 'allow', 'updatedInput': value})
        events = self.manager.events('one')['events']
        self.assertEqual(sum(item['type'] == 'approval-resolved' for item in events), 1)
        self.assertEqual(events[-1]['id'], event['id'])
        self.assertEqual(self.manager.events('one', events[0]['seq'])['events'], events[1:])

    def test_conversation_rules_for_files_bash_and_other_tools(self):
        for tool, first, repeat, changed in (
            ('Write', {'file_path': 'x.m', 'content': 'one'}, {'file_path': './x.m', 'content': 'two'}, {'file_path': 'y.m', 'content': 'two'}),
            ('Bash', {'command': 'echo one'}, {'command': 'echo one', 'description': 'different'}, {'command': 'echo two'}),
            ('Other', {'a': 1}, {'a': 2}, None),
        ):
            thread, output = self.start_request(tool, first, use=tool)
            self.decide(self.pending()[-1], 'allow-conversation'); thread.join(2)
            thread, output = self.start_request(tool, repeat, use=tool + '-repeat')
            thread.join(2); self.assertEqual(self.answer(output)['behavior'], 'allow')
            if changed:
                thread, output = self.start_request(tool, changed, use=tool + '-changed')
                self.assertTrue(thread.is_alive())
                self.decide(self.pending()[-1], 'deny', message='Please use a smaller change')
                thread.join(2); self.assertEqual(self.answer(output)['message'], 'Please use a smaller change')
        self.new_session('two')
        thread, output = self.start_request('Write', {'file_path': 'x.m', 'content': 'three'}, 'two', 'new-conversation')
        self.assertTrue(thread.is_alive())
        self.decide(self.pending('two')[-1], 'deny', identity='two'); thread.join(2)

    def test_stop_remove_close_and_expiry_deny_and_release(self):
        for action in ('stop', 'remove', 'close', 'expire'):
            with self.subTest(action=action):
                identity = action
                self.new_session(identity)
                with patch('backend.assistant_approvals.WAIT_SECONDS', .03 if action == 'expire' else 3600):
                    thread, output = self.start_request(identity=identity, use=action)
                    if action != 'expire': getattr(self.manager, action)(identity) if action != 'close' else self.manager.close()
                    thread.join(2)
                self.assertFalse(thread.is_alive())
                self.assertEqual(self.answer(output)['behavior'], 'deny')
                if action == 'expire': self.assertIn('one hour', self.answer(output)['message'])
                self.manager.closed = False

    def test_capabilities_none_level_and_invalid_cross_conversation_decisions(self):
        one = self.bridge.grants['one']
        two = self.new_session('two')
        self.assertEqual([tool['name'] for tool in self.bridge.tools(one)['tools']], ['approve'])
        self.assertTrue(self.bridge.call(one, {'name': 'session_status'})['isError'])
        thread, output = self.start_request()
        event = self.pending()[0]
        with self.assertRaises(ValueError): self.decide(event, identity='two')
        wrong = self.bridge.call(two, {'name': 'approve', 'arguments': {'session': 'one', 'tool_name': 'Write', 'input': {}, 'tool_use_id': 'steal'}})
        self.assertTrue(wrong['isError']); self.assertFalse(output)
        for token in ('launch-private', 'invalid', ''):
            with self.assertRaises(PermissionError): self.bridge.authenticate(token)
        token = one['token']
        self.assertIs(self.bridge.authenticate(token), one)
        self.bridge.revoke('one'); thread.join(2)
        self.assertEqual(self.answer(output)['behavior'], 'deny')
        with self.assertRaises(PermissionError): self.bridge.authenticate(token)
        inspect = self.bridge.create('inspect', 'claude', 'inspect')
        self.assertTrue(self.bridge.call(self.bridge.grants['inspect'], {'name': 'approve', 'arguments': {}})['isError'])
        self.assertEqual({tool['name'] for tool in self.bridge.tools(self.bridge.grants['inspect'])['tools']}, set(INSPECT_TOOLS))
        self.bridge.revoke('inspect')

    def test_pending_bound_and_validation(self):
        running = [self.start_request(use=str(index)) for index in range(MAX_PENDING)]
        thread, output = self.start_request(use='overflow'); thread.join(2)
        self.assertEqual(self.answer(output)['behavior'], 'deny')
        self.assertEqual(len(self.pending()), MAX_PENDING)
        for request in ({}, {'session': 'one', 'id': [], 'decision': 'allow'}, {'session': 'one', 'id': 'a', 'decision': 'always'}, {'session': 'one', 'id': 'a', 'decision': 'deny', 'message': 'ö' * 600}):
            with self.assertRaises(ValueError): self.service.decide(request)
        self.manager.stop('one')
        for thread, output in running:
            thread.join(2); self.assertEqual(self.answer(output)['behavior'], 'deny')

    def test_route_guards_and_duplicate_waiter_bound(self):
        thread, output = self.start_request()
        event = self.pending()[0]
        payload = {'session': 'one', 'id': event['id'], 'decision': 'allow'}
        for extra in ({'Host': 'evil.invalid'}, {'Origin': 'https://evil.invalid'}, {'X-MF-Token': 'invalid'}, {'Sec-Fetch-Site': 'cross-site'}):
            handler = Handler.__new__(Handler)
            handler.server = SimpleNamespace(app=self.app)
            handler.path = '/api/assistant/approve'
            handler.headers = Message()
            for key, value in {'Host': '127.0.0.1:12345', 'X-MF-Token': 'launch-private', 'Content-Length': str(len(json.dumps(payload))), **extra}.items(): handler.headers[key] = value
            handler.rfile = io.BytesIO(json.dumps(payload).encode())
            responses = []
            handler.send = lambda status, body, *args, **kwargs: responses.append((status, body))
            handler.handle_request(True)
            self.assertEqual(responses[0][0], 403)
            self.assertFalse(output)
        duplicates = [self.start_request() for _ in range(MAX_PENDING - 1)]
        fifth, denied = self.start_request()
        fifth.join(2)
        self.assertEqual(self.answer(denied)['behavior'], 'deny')
        self.assertEqual(len(self.pending()), 1)
        self.decide(event, 'deny')
        for waiter, result in [(thread, output), *duplicates]:
            waiter.join(2)
            self.assertEqual(self.answer(result)['behavior'], 'deny')

    def test_allow_once_reprompts_and_stale_turn_decision_denies(self):
        thread, output = self.start_request(use='first')
        self.decide(self.pending()[-1], 'allow'); thread.join(2)
        second, result = self.start_request(use='second')
        self.assertTrue(second.is_alive())
        event = self.pending()[-1]
        self.manager.sessions['one']['turn'] = 'new-turn'
        self.assertEqual(self.decide(event, 'allow')['decision'], 'deny')
        second.join(2)
        self.assertEqual(self.answer(result)['behavior'], 'deny')

    def test_prepared_previews_decisions_bounds_and_cancel(self):
        def begin(kind, payload):
            output = []
            worker = threading.Thread(target=lambda: output.append(self.service.request_prepared('one', 'turn-one', kind, payload)))
            worker.start()
            self.addCleanup(lambda: self.service.cancel('one', 'cleanup'))
            deadline = time.monotonic() + 2
            while not self.service.pending('one') and not output and time.monotonic() < deadline: time.sleep(.005)
            return worker, output
        worker, output = begin('command', {'request_id': 'codex-shell', 'command': "/bin/zsh -lc 'touch x.m'", 'reason': 'launch-private reason'})
        entry = self.service.pending('one')[0]
        self.assertEqual(entry['tool'], 'Shell')
        self.assertEqual(entry['summary'], 'touch x.m')
        self.assertIn('/bin/zsh', entry['detail']['text'])
        self.assertNotIn('launch-private', entry['detail']['text'])
        self.decide(entry, 'allow-conversation')
        worker.join(2)
        self.assertEqual(output, ['accept_session'])
        self.assertEqual(self.service.request_prepared('one', 'turn-one', 'command', {'request_id': 'again', 'command': "/bin/zsh -lc 'touch x.m'"}), 'accept_session')
        worker, output = begin('file', {'request_id': 'codex-file', 'paths': [{'path': 'x.m'}], 'diff': '+new\n' + 'ö' * 100000})
        entry = self.service.pending('one')[0]
        self.assertEqual(entry['tool'], 'Edit')
        self.assertTrue(entry['detail']['truncated'])
        self.assertLessEqual(len(entry['detail']['text'].encode()), DETAIL_BYTES)
        self.service.cancel('one', 'stopped')
        worker.join(2)
        self.assertEqual(output, ['cancel'])
        self.assertFalse(self.service.waiters)
        inside, outside = str(self.folder / 'sub' / 'y.m'), '/etc/hosts'
        worker, output = begin('file', {'request_id': 'codex-paths', 'paths': [{'path': inside}, {'path': outside}], 'diff': '+new\n'})
        entry = self.service.pending('one')[0]
        self.assertEqual(entry['summary'], 'sub/y.m, /etc/hosts')
        self.assertNotIn(str(self.folder), entry['detail']['text'])
        self.service.cancel('one', 'stopped')
        worker.join(2)
        for kind in ('nope', None):
            with self.assertRaises(ValueError): self.service.request_prepared('one', 'turn-one', kind, {})

    def test_prepared_pending_limit_and_expiry(self):
        workers = []
        results = []
        for index in range(MAX_PENDING):
            worker = threading.Thread(target=lambda number=index: results.append(self.service.request_prepared('one', 'turn-one', 'command', {'request_id': number, 'command': 'command ' + str(number)})))
            worker.start()
            workers.append(worker)
        deadline = time.monotonic() + 2
        while len(self.service.pending('one')) < MAX_PENDING and time.monotonic() < deadline: time.sleep(.005)
        self.assertEqual(len(self.service.pending('one')), MAX_PENDING)
        self.assertEqual(self.service.request_prepared('one', 'turn-one', 'command', {'request_id': 'excess', 'command': 'one more'}), 'decline')
        self.service.cancel('one', 'cancel')
        for worker in workers: worker.join(2)
        self.assertEqual(results, ['cancel'] * MAX_PENDING)
        with patch('backend.assistant_approvals.WAIT_SECONDS', .01):
            self.assertEqual(self.service.request_prepared('one', 'turn-one', 'file', {'request_id': 'expires', 'paths': [{'path': 'x.m'}], 'diff': '+x'}), 'decline')
        self.assertFalse(self.service.pending('one'))
        self.assertFalse(self.service.waiters)

    def test_argv_and_context(self):
        bridge = {'config': '/private/config'}
        for level, names in (('none', ()), ('inspect', INSPECT_TOOLS), ('run', RUN_TOOLS)):
            argv = command('claude', 'ask', 'claude', self.folder, session_access=level, bridge=bridge)
            self.assertEqual(argv[argv.index('--permission-mode') + 1], 'default')
            self.assertEqual(argv[argv.index('--permission-prompt-tool') + 1], 'mcp__indymat__approve')
            allowed = argv[argv.index('--allowedTools') + 1].split(',') if '--allowedTools' in argv else []
            self.assertEqual(allowed, ['mcp__indymat__' + name for name in names])
            self.assertNotIn('mcp__indymat__approve', allowed)
        for provider in ('codex', 'agy'):
            with self.assertRaises(ValueError): command(provider, 'ask', provider, self.folder, bridge=bridge)
        note = environment_note('ask', self.folder, self.folder)
        self.assertIn('You may propose any change', note)
        self.assertIn('explain in one line', note)


if __name__ == '__main__': unittest.main()
