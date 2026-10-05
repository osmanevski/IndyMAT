"""Panel integration against the offline app-server, without sockets or accounts."""
import json
import os
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from backend.assistants import Assistants
from backend.files import Workspace
from backend.assistant_bridge_service import BridgeService
from types import SimpleNamespace

FIXTURE = str(Path(__file__).parent / 'fixtures/codex_app_server_fake.py')


class CodexPanel(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.folder = Path(self.tmp.name).resolve()
        self.manager = Assistants(Workspace(self.folder, self.folder), 'private-launch-token', describe=lambda: {'version': '11.3.0'})
        self.env = patch.dict(os.environ, {'INDYMAT_ASSISTANT_CODEX': FIXTURE})
        self.env.start()

    def tearDown(self):
        self.manager.close()
        self.env.stop()
        self.tmp.cleanup()

    def start(self, prompt='hello', **extra):
        return self.manager.start({'provider': 'codex', 'prompt': prompt, **extra})

    def poll(self, identity, predicate=lambda value: not value['running']):
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline:
            value = self.manager.events(identity)
            if predicate(value): return value
            time.sleep(.01)
        self.fail('Timed out waiting for fixture: ' + str(self.manager.events(identity)))

    def configuration(self, identity):
        value = self.poll(identity)
        return json.loads(next(item['text'] for item in value['events'] if item['type'] == 'text'))

    def test_modes_context_environment_and_reuse(self):
        for mode, sandbox, policy in [('read-only', 'read-only', 'never'), ('edit', 'workspace-write', 'never'), ('ask', 'read-only', 'on-request')]:
            started = self.start('config', mode=mode)
            identity = started['session']
            config = self.configuration(identity)
            self.assertEqual(config['thread']['sandbox'], sandbox)
            self.assertEqual(config['thread']['approvalPolicy'], policy)
            self.assertIn('GNU Octave 11.3.0', config['thread']['developerInstructions'])
            self.assertEqual(config['argv'], ['app-server'])
            self.assertNotIn('private-launch-token', json.dumps(config))
            session = self.manager.sessions[identity]
            client, thread = session['codex_client'], session['conversation']
            target = self.folder / 'sample.m'
            target.write_text('x=1;')
            after = self.manager.events(identity)['after']
            second = self.start('echo-context', conversation=identity, ide={'active': str(target), 'dirty': True, 'open': []})
            self.assertNotEqual(second['turn'], started['turn'])
            events = self.poll(identity)['events']
            text = ''.join(item.get('text', '') for item in events if item['type'] == 'text' and item['seq'] > after)
            self.assertIn('Active editor file', text)
            self.assertIn('unsaved changes', text)
            self.assertNotIn('You are running inside IndyMAT', text)
            self.assertIs(session['codex_client'], client)
            self.assertEqual(session['conversation'], thread)
            self.manager.remove(identity)
            self.assertIsNotNone(client.proc.poll())

    def test_early_events_bounds_limits_and_attachment_privacy(self):
        identity = self.start('early')['session']
        value = self.poll(identity)
        self.assertEqual(value['events'][-1]['state'], 'completed')
        self.assertTrue(any(item['type'] == 'text' for item in value['events']))
        for prompt in ('flood', 'long', 'fail'):
            started = self.start(prompt)
            value = self.poll(started['session'])
            self.assertLessEqual(len(value['events']), 513)
            self.assertTrue(all(len(item.get('text', '')) <= 16000 for item in value['events']))
            if prompt == 'flood': self.assertTrue(value['truncated'])
            if prompt == 'long': self.assertTrue(any(item['type'] == 'notice' for item in value['events']))
            if prompt == 'fail': self.assertEqual(value['events'][-1]['state'], 'failed')
        identity = self.start('config')['session']
        self.assertEqual(self.configuration(identity)['input'], 'config')
        target = self.folder / 'x.m'
        target.write_text('disk')
        identity = self.start('echo-context', context={'path': 'x.m', 'dirty': True, 'content': 'private unsaved'})['session']
        text = ''.join(item.get('text', '') for item in self.poll(identity)['events'] if item['type'] == 'text')
        self.assertNotIn('private unsaved', text)
        identity = self.start('echo-context', context={'path': 'x.m', 'dirty': True, 'include_unsaved': True, 'content': 'explicit draft'})['session']
        text = ''.join(item.get('text', '') for item in self.poll(identity)['events'] if item['type'] == 'text')
        self.assertIn('explicit draft', text)
        self.manager.MAX_SESSIONS = len(self.manager.sessions)
        with self.assertRaises(ValueError): self.start()

    def test_concurrency_limit_keeps_steering_available(self):
        identities = [self.start('hold')['session'] for _ in range(self.manager.MAX_RUNNING)]
        for identity in identities: self.poll(identity, lambda value: bool(self.manager.sessions[identity].get('codex_turn')))
        with self.assertRaises(ValueError): self.start('hold')
        result = self.start('direction', conversation=identities[0])
        self.assertTrue(result['steered'])
        for identity in identities:
            self.manager.stop(identity)
            self.assertEqual(self.poll(identity)['events'][-1]['state'], 'stopped')

    def test_removal_during_initialize_releases_child(self):
        from backend.codex_app_server import CodexAppServer
        real = CodexAppServer
        def delayed(*args, **kwargs):
            return real(*args, **kwargs, config_overrides=('fake_hang_initialize=true',))
        with patch('backend.assistant_codex.SelectedCodexAppServer', side_effect=delayed):
            identity = self.start()['session']
            self.poll(identity, lambda value: bool(self.manager.sessions[identity].get('codex_client')))
            client = self.manager.sessions[identity]['codex_client']
            self.manager.stop(identity)
            self.assertEqual(self.poll(identity)['events'][-1]['state'], 'stopped')
            self.assertIsNotNone(client.proc.poll())
            self.manager.remove(identity)
            self.assertNotIn(identity, self.manager.sessions)
            identity = self.start()['session']
            self.poll(identity, lambda value: bool(self.manager.sessions[identity].get('codex_client')))
            client = self.manager.sessions[identity]['codex_client']
            self.manager.remove(identity)
            self.assertIsNotNone(client.proc.poll())
            self.assertNotIn(identity, self.manager.sessions)

    def test_default_ask_and_all_approval_decisions(self):
        for kind in ('command', 'file'):
            for decision, response in [('allow', 'accept'), ('allow-conversation', 'acceptForSession'), ('deny', 'decline')]:
                identity = self.start('approval-' + kind)['session']
                pending = self.poll(identity, lambda value: bool(value['approvals']))['approvals'][0]
                self.assertEqual(self.manager.sessions[identity]['mode'], 'ask')
                self.assertEqual(pending['tool'], 'Shell' if kind == 'command' else 'Edit')
                self.assertIn('Write a file.', pending['detail']['text'])
                if kind == 'file': self.assertIn('+answer=42;', pending['detail']['text'])
                self.manager.approvals.decide({'session': identity, 'id': pending['id'], 'decision': decision})
                value = self.poll(identity)
                self.assertTrue(any(item['type'] == 'text' and response in item['text'] for item in value['events']))
                self.assertTrue(any(item['type'] == 'approval-resolved' and item['decision'] == decision for item in value['events']))
                self.assertEqual(value['events'][-1]['state'], 'completed')

    def test_stop_pending_and_steer_interrupt(self):
        identity = self.start('approval-command')['session']
        self.poll(identity, lambda value: bool(value['approvals']))
        self.manager.stop(identity)
        self.manager.stop(identity)
        result = self.poll(identity)
        self.assertEqual(result['events'][-1]['state'], 'stopped')
        self.assertFalse(result['approvals'])
        deadline = time.monotonic() + 2
        while self.manager.approvals.waiters and time.monotonic() < deadline: time.sleep(.01)
        self.assertFalse(self.manager.approvals.waiters)
        started = self.start('hold', conversation=identity)
        self.poll(identity, lambda value: bool(self.manager.sessions[identity].get('codex_turn')))
        steered = self.start('new direction', conversation=identity)
        self.assertTrue(steered['steered'])
        self.assertEqual(steered['turn'], started['turn'])
        self.poll(identity, lambda value: any(item.get('text') == 'new direction' for item in value['events']))
        self.manager.stop(identity)
        result = self.poll(identity)
        self.assertEqual(result['events'][-1]['state'], 'stopped')
        self.assertEqual(sum(item['type'] == 'turn-end' and item['turn'] == started['turn'] for item in result['events']), 1)

    def test_retry_terminal_resume_and_failed_resume(self):
        identity = self.start('events')['session']
        self.assertEqual(self.poll(identity)['events'][-1]['state'], 'completed')
        session = self.manager.sessions[identity]
        client, thread = session['codex_client'], session['conversation']
        self.start('crash', conversation=identity)
        self.assertEqual(self.poll(identity)['events'][-1]['state'], 'failed')
        deadline = time.monotonic() + 2
        while session.get('codex_client') and time.monotonic() < deadline: time.sleep(.01)
        self.start('hello', conversation=identity)
        self.poll(identity)
        self.assertIsNot(session['codex_client'], client)
        self.assertEqual(session['conversation'], thread)
        self.assertIsNotNone(client.proc.poll())
        for bad_thread in ('rpc-error', 'bad-result'):
            self.manager._codex_discard(identity, session, session['codex_generation'])
            session['conversation'] = bad_thread
            self.start('config', conversation=identity)
            value = self.poll(identity)
            self.assertTrue(any(item['type'] == 'notice' and 'could not resume' in item['text'] for item in value['events']))
            self.assertEqual(value['events'][-1]['state'], 'completed')

    def test_removal_close_pending_and_idle_clients(self):
        identity = self.start('approval-file')['session']
        self.poll(identity, lambda value: bool(value['approvals']))
        client = self.manager.sessions[identity]['codex_client']
        self.manager.remove(identity)
        self.assertNotIn(identity, self.manager.sessions)
        self.assertIsNotNone(client.proc.poll())
        identity = self.start()['session']
        self.poll(identity)
        client = self.manager.sessions[identity]['codex_client']
        self.manager.close()
        self.assertIsNotNone(client.proc.poll())

    def test_bridge_thread_config_without_claude_approval_tool(self):
        app = SimpleNamespace(runtime=self.folder / '.matlab-free', base='http://127.0.0.1:1234/', token='private-launch-token', assistants=self.manager)
        app.runtime.mkdir()
        bridge = BridgeService(app)
        self.manager.bridge = bridge
        identity = self.start('config', session_access='inspect')['session']
        value = self.configuration(identity)
        paths = self.manager.sessions[identity]['bridge']
        expected = {'command': paths['python'], 'args': [paths['script'], paths['capability']], 'default_tools_approval_mode': 'approve'}
        self.assertEqual(value['thread']['config']['mcp_servers'], {'indymat': expected})
        grant = bridge.grants[identity]
        self.assertFalse(grant['approvals'])
        self.assertNotIn('approve', [tool['name'] for tool in bridge.tools(grant)['tools']])
        self.manager.remove(identity)
        self.assertFalse(Path(paths['capability']).exists())


if __name__ == '__main__': unittest.main()
