"""Offline transport, approval and lifecycle tests; no model/account/network."""
import json
import os
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from backend.codex_app_server import CodexAppServer, CodexAppServerError

FIXTURE = Path(__file__).parent / 'fixtures' / 'codex_app_server_fake.py'


class CodexAppServerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.events = []
        self.condition = threading.Condition()

    def record(self, event):
        with self.condition:
            self.events.append(event)
            self.condition.notify_all()

    def client(self, request=None, overrides=(), env=None):
        client = CodexAppServer(str(FIXTURE), self.directory.name, os.environ if env is None else env,
                                self.record, request or (lambda kind, payload: 'decline'), overrides)
        self.addCleanup(client.close)
        return client

    def wait_event(self, predicate, timeout=4):
        deadline = time.monotonic() + timeout
        with self.condition:
            while True:
                for event in self.events:
                    if predicate(event):
                        return event
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    self.fail('Expected event missing: ' + repr(self.events))
                self.condition.wait(remaining)

    def thread(self, client):
        return client.start_thread('read-only', 'on-request', 'IndyMAT context')

    def turn_end(self, turn):
        return self.wait_event(lambda event: event['type'] == 'turn-end' and event['turn_id'] == turn)

    def test_initialize_thread_resume_turn_and_configuration(self):
        client = self.client(overrides=('model="gpt-6-luna"',), env={**os.environ, 'INDYMAT_TOKEN': 'private', 'XDG_SECRET': '/x/.matlab-free/y'})
        servers = {'indymat': {'command': '/usr/bin/python3', 'args': ['bridge.py', 'private-capability'], 'default_tools_approval_mode': 'approve'}}
        thread = client.start_thread('workspace-write', 'untrusted', 'Real developer instructions', servers)
        self.assertEqual(client.resume_thread(thread), thread)
        turn = client.start_turn(thread, 'config')
        self.turn_end(turn)
        value = json.loads(self.wait_event(lambda event: event['type'] == 'text' and event['turn_id'] == turn)['text'])
        self.assertEqual(value['thread'], {'cwd': self.directory.name, 'sandbox': 'workspace-write', 'approvalPolicy': 'untrusted',
                                         'developerInstructions': 'Real developer instructions', 'config': {'mcp_servers': servers}})
        self.assertEqual(value['argv'], ['app-server', '-c', 'model="gpt-6-luna"'])
        self.assertNotIn('INDYMAT_TOKEN', value['env'])
        self.assertNotIn('XDG_SECRET', value['env'])
        self.assertEqual(self.wait_event(lambda event: event['type'] == 'conversation')['conversation'], thread)
        self.assertEqual(client.proc.pid, os.getpgid(client.proc.pid))
        self.assertEqual(sum(event['type'] == 'text' for event in self.events), 1)

    def test_user_text_is_only_protocol_data(self):
        client = self.client()
        thread = self.thread(client)
        turn = client.start_turn(thread, '--dangerously-bypass-approvals-and-sandbox; $(touch injected)\n')
        self.turn_end(turn)
        self.assertEqual(client.proc.args, [str(FIXTURE), 'app-server'])
        self.assertFalse((Path(self.directory.name) / 'injected').exists())

    def test_steer_and_idempotent_interrupt(self):
        client = self.client()
        thread = self.thread(client)
        turn = client.start_turn(thread, 'hold')
        self.assertEqual(client.steer(thread, turn, 'new direction'), {'turnId': turn})
        self.wait_event(lambda event: event['type'] == 'text' and event['text'] == 'new direction')
        client.interrupt(thread, turn)
        self.assertEqual(self.turn_end(turn)['state'], 'stopped')
        client.interrupt(thread, turn)
        client.close()
        client.interrupt(thread, turn)
        client.close()
        self.assertEqual(sum(event['type'] == 'turn-end' for event in self.events), 1)

    def test_all_decisions_for_command_and_file(self):
        for kind in ('command', 'file'):
            for choice, expected in CodexAppServer.DECISIONS.items():
                with self.subTest(kind=kind, choice=choice):
                    seen = []
                    client = self.client(lambda name, payload: (seen.append((name, payload)), choice)[1])
                    thread = self.thread(client)
                    turn = client.start_turn(thread, 'approval-' + kind)
                    self.turn_end(turn)
                    response = json.loads(self.wait_event(lambda event: event['type'] == 'text' and event['turn_id'] == turn and event['thread_id'] == thread and event['text'].startswith('{"id": "server-'))['text'])
                    self.assertEqual(response['result'], {'decision': expected})
                    self.assertEqual(seen[0][0], kind)
                    self.assertEqual(Path(seen[0][1]['cwd']).resolve(), Path(self.directory.name).resolve())
                    self.assertEqual(seen[0][1]['item_id'], 'approval')
                    self.assertEqual(seen[0][1]['reason'], 'Write a file.')
                    if kind == 'file':
                        self.assertEqual(seen[0][1]['paths'], [{'path': 'answer.m', 'kind': 'add'}])
                        self.assertIn('+answer=42;', seen[0][1]['diff'])
                    else:
                        self.assertEqual(seen[0][1]['command'], 'touch answer.m')
                    client.close()
                    # Fixture ids restart in each process; do not reuse old events.
                    with self.condition:
                        self.events.clear()

    def test_callback_exception_and_invalid_decision_cancel(self):
        def failed(kind, payload):
            raise RuntimeError('UI failed')
        for callback in (failed, lambda kind, payload: 'unexpected', lambda kind, payload: {}):
            client = self.client(callback)
            thread = self.thread(client)
            turn = client.start_turn(thread, 'approval-command')
            self.turn_end(turn)
            response = json.loads(self.wait_event(lambda event: event['type'] == 'text')['text'])
            self.assertEqual(response['result']['decision'], 'cancel')
            client.close()
            self.events.clear()

    def test_blocking_approval_keeps_reader_and_calls_live(self):
        entered, release = threading.Event(), threading.Event()
        self.addCleanup(release.set)
        def decide(kind, payload):
            entered.set()
            release.wait(5)
            return 'accept'
        client = self.client(decide)
        thread = self.thread(client)
        turn = client.start_turn(thread, 'approval-command')
        self.assertTrue(entered.wait(2))
        other_thread = self.thread(client)
        other_turn = client.start_turn(other_thread, 'events')
        self.turn_end(other_turn)
        self.assertEqual(client.resume_thread(other_thread), other_thread)
        self.assertFalse(any(event['type'] == 'turn-end' and event['turn_id'] == turn for event in self.events))
        release.set()
        self.turn_end(turn)

    def test_close_pending_approval_answers_cancel_without_waiting_for_user(self):
        entered, release = threading.Event(), threading.Event()
        self.addCleanup(release.set)
        client = self.client(lambda kind, payload: (entered.set(), release.wait(5), 'accept')[-1])
        thread = self.thread(client)
        client.start_turn(thread, 'approval-command')
        self.assertTrue(entered.wait(2))
        sent = []
        original = client._send
        def spy(message, **kwargs):
            sent.append(message)
            return original(message, **kwargs)
        client._send = spy
        started = time.monotonic()
        client.close()
        self.assertLess(time.monotonic() - started, 2)
        self.assertTrue(any(value.get('result') == {'decision': 'cancel'} for value in sent))
        release.set()
        self.assertFalse(any(value.get('result') == {'decision': 'accept'} for value in sent))
        self.assertIsNotNone(client.proc.poll())
        with self.assertRaises(CodexAppServerError):
            client.start_turn(thread, 'again')

    def test_interrupt_cancels_pending_approval(self):
        entered, release = threading.Event(), threading.Event()
        self.addCleanup(release.set)
        client = self.client(lambda kind, payload: (entered.set(), release.wait(5), 'accept')[-1])
        thread = self.thread(client)
        turn = client.start_turn(thread, 'approval-command')
        self.assertTrue(entered.wait(2))
        client.interrupt(thread, turn)
        self.assertFalse(client._approvals)
        response = json.loads(self.wait_event(lambda event: event['type'] == 'text' and event['turn_id'] == turn)['text'])
        self.assertEqual(response['result']['decision'], 'cancel')
        release.set()

    def test_normalized_events_and_no_completed_duplicates(self):
        client = self.client()
        turn = client.start_turn(self.thread(client), 'events')
        self.turn_end(turn)
        texts = [event['text'] for event in self.events if event['type'] == 'text']
        thoughts = [event['text'] for event in self.events if event['type'] == 'reasoning']
        tools = [event for event in self.events if event['type'] == 'tool']
        self.assertEqual(texts, ['done'])
        self.assertEqual(thoughts, ['Think first.', 'Streamed thought.'])
        self.assertEqual([event['name'] for event in tools], ['command_execution', 'indymat · session_status', 'web_search'])
        self.assertEqual(tools[0]['command'], 'pwd')
        self.assertEqual(tools[2]['text'], 'Octave')
        self.assertFalse(any(event['type'] == 'raw' for event in self.events))
        self.assertTrue(self.wait_event(lambda event: event['type'] == 'error')['will_retry'])

    def test_file_changes_unified_diff_and_utf8_byte_bound(self):
        client = self.client()
        for prompt in ('file', 'big-diff'):
            turn = client.start_turn(self.thread(client), prompt)
            self.turn_end(turn)
            events = [event for event in self.events if event['type'] == 'file' and event['turn_id'] == turn]
            self.assertEqual(len(events), 3)
            self.assertEqual(events[0]['paths'], [{'path': 'test.m', 'kind': 'update', 'move_path': 'renamed.m'}])
            for event in events:
                self.assertIn('@@ -1 +1 @@', event['diff'])
                self.assertLessEqual(len(event['diff'].encode('utf-8')), 64 * 1024)
                self.assertEqual(event['truncated'], prompt == 'big-diff')
                self.assertNotIn('\ufffd', event['diff'])
            self.assertEqual(events[-1]['paths'], [{'path': 'test.m', 'kind': 'update'}])
            self.assertFalse(client._files)
            self.assertFalse(client._turn_diffs)

    def test_unknown_notification_is_one_bounded_raw_event(self):
        client = self.client()
        self.turn_end(client.start_turn(self.thread(client), 'unknown'))
        values = [event for event in self.events if event['type'] == 'raw']
        self.assertEqual(len(values), 1)
        self.assertTrue(values[0]['truncated'])
        self.assertLessEqual(len(values[0]['text'].encode('utf-8')), client.RAW_LIMIT)

    def test_unsupported_server_requests_are_answered(self):
        expected = {
            'item/tool/requestUserInput': {'answers': {}},
            'item/permissions/requestApproval': {'permissions': {}, 'scope': 'turn'},
            'item/tool/call': {'success': False, 'contentItems': [{'type': 'inputText', 'text': 'Unsupported client tool.'}]},
            'mcpServer/elicitation/request': {'action': 'decline'},
            'applyPatchApproval': {'decision': {'denied': {'rejection': 'Unsupported legacy approval.'}}},
            'execCommandApproval': {'decision': {'denied': {'rejection': 'Unsupported legacy approval.'}}},
            'future/serverRequest': None,
            'account/chatgptAuthTokens/refresh': None,
            'attestation/generate': None,
        }
        client = self.client()
        thread = self.thread(client)
        for method, result in expected.items():
            with self.subTest(method=method):
                turn = client.start_turn(thread, 'unsupported:' + method)
                self.turn_end(turn)
                event = self.wait_event(lambda event: event['type'] == 'text' and event['turn_id'] == turn)
                response = json.loads(event['text'])
                if result is None:
                    self.assertEqual(response['error']['code'], -32601)
                else:
                    self.assertEqual(response['result'], result)

    def test_rpc_timeout_is_terminal_and_fails_concurrent_calls(self):
        client = self.client()
        thread = self.thread(client)
        errors = []
        def request():
            try:
                client.resume_thread('hang')
            except Exception as exc:
                errors.append(exc)
        with patch.object(client, 'CALL_TIMEOUT', .2):
            worker = threading.Thread(target=request)
            worker.start()
            with self.assertRaises((TimeoutError, CodexAppServerError)):
                client.start_turn(thread, 'hang-call')
            worker.join(2)
        self.assertFalse(worker.is_alive())
        self.assertEqual(len(errors), 1)
        self.wait_event(lambda event: event.get('terminal'))
        client.close()
        self.assertEqual(sum(event.get('terminal', False) for event in self.events), 1)

    def test_close_fails_pending_calls(self):
        client = self.client()
        errors = []
        def request():
            try:
                client.resume_thread('hang')
            except CodexAppServerError as exc:
                errors.append(str(exc))
        worker = threading.Thread(target=request)
        worker.start()
        deadline = time.monotonic() + 2
        while not client._pending and time.monotonic() < deadline:
            time.sleep(.01)
        client.close()
        worker.join(2)
        self.assertFalse(worker.is_alive())
        self.assertEqual(len(errors), 1)

    def test_crash_malformed_and_oversized_protocol_terminate_once(self):
        for prompt in ('crash', 'malformed', 'oversized', 'bad-delta'):
            with self.subTest(prompt=prompt):
                client = self.client()
                client.start_turn(self.thread(client), prompt)
                self.wait_event(lambda event: event.get('terminal'))
                client.close()
                self.assertIsNotNone(client.proc.poll())
                self.assertEqual(sum(event.get('terminal', False) for event in self.events), 1)
                self.events.clear()

    def test_invalid_result_is_terminal_but_rpc_error_is_recoverable(self):
        client = self.client()
        with self.assertRaises(CodexAppServerError):
            client.resume_thread('rpc-error')
        self.assertEqual(client.resume_thread('thread-existing'), 'thread-existing')
        with self.assertRaises(CodexAppServerError):
            client.resume_thread('bad-result')
        self.wait_event(lambda event: event.get('terminal'))

    def test_failed_turn_is_distinct_from_dead_server(self):
        client = self.client()
        turn = client.start_turn(self.thread(client), 'failed')
        self.assertEqual(self.turn_end(turn)['state'], 'failed')
        self.assertEqual(self.wait_event(lambda event: event['type'] == 'error')['text'], 'Model failed.')
        self.assertFalse(any(event.get('terminal') for event in self.events))
        self.turn_end(client.start_turn(self.thread(client), 'normal'))

    def test_stderr_is_drained_and_bounded(self):
        client = self.client()
        self.turn_end(client.start_turn(self.thread(client), 'stderr'))
        deadline = time.monotonic() + 2
        while len(client.stderr_tail) < client.STDERR_LIMIT and time.monotonic() < deadline:
            time.sleep(.01)
        self.assertEqual(len(client.stderr_tail.encode('utf-8')), client.STDERR_LIMIT)
        self.assertEqual(client.stderr_tail, 'z' * client.STDERR_LIMIT)

    @staticmethod
    def alive(pid):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        # Linux can retain an orphan zombie briefly; it cannot execute or hold pipes.
        status = Path('/proc') / str(pid) / 'stat'
        if status.exists() and status.read_text().split(') ', 1)[1].startswith('Z'):
            return False
        return True

    def test_close_and_crash_kill_grandchildren_holding_pipes(self):
        for prompt in ('grandchild', 'crash-grandchild'):
            with self.subTest(prompt=prompt):
                client = self.client()
                turn = client.start_turn(self.thread(client), prompt)
                child = int(self.wait_event(lambda event: event['type'] == 'text' and event['turn_id'] == turn)['text'])
                if prompt == 'crash-grandchild':
                    self.wait_event(lambda event: event.get('terminal'))
                client.close()
                deadline = time.monotonic() + 3
                while self.alive(child) and time.monotonic() < deadline:
                    time.sleep(.02)
                self.assertFalse(self.alive(child))
                self.assertIsNotNone(client.proc.poll())
                self.events.clear()

    def test_initialization_timeout_and_protocol_failure_reap_child(self):
        for override in ('fake_hang_initialize=true', 'fake_bad_initialize=true'):
            with self.subTest(override=override):
                with patch.object(CodexAppServer, 'CALL_TIMEOUT', .2):
                    with self.assertRaises((TimeoutError, CodexAppServerError)):
                        self.client(overrides=(override,))
                self.assertEqual(sum(event.get('terminal', False) for event in self.events), 1)
                self.events.clear()

    def test_event_callback_can_make_rpc_and_errors_do_not_block_transport(self):
        ready = threading.Event()
        holder = {}
        def callback(event):
            if event['type'] == 'text':
                holder['resumed'] = holder['client'].resume_thread('thread-from-callback')
                ready.set()
                raise RuntimeError('Consumer failed')
            self.record(event)
        client = CodexAppServer(str(FIXTURE), self.directory.name, os.environ, callback, lambda kind, payload: 'decline')
        self.addCleanup(client.close)
        holder['client'] = client
        turn = client.start_turn(self.thread(client), 'normal')
        self.assertTrue(ready.wait(2))
        self.turn_end(turn)
        self.assertEqual(holder['resumed'], 'thread-from-callback')
        self.assertEqual(client.resume_thread('still-live'), 'still-live')

    def test_invalid_application_configuration_is_rejected(self):
        with self.assertRaises(ValueError):
            self.client(overrides=('--bypass',))
        client = self.client()
        for sandbox, policy in (('danger-full-access', 'on-request'), ('read-only', 'bad')):
            with self.assertRaises(ValueError):
                client.start_thread(sandbox, policy, '')
        with self.assertRaises(ValueError):
            client.start_thread('read-only', 'on-request', '', {'bad.name': {}})
        with self.assertRaises(ValueError):
            client.start_turn('thread', {'prompt': 'invalid'})
        with self.assertRaises(ValueError):
            client.steer('thread', 'turn', [])


if __name__ == '__main__':
    unittest.main()
