import json
import io
from email.message import Message
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch
from types import SimpleNamespace
from backend.assistants import Assistants
from backend.assistant_models import parse_agy_models, codex_models
from backend.codex_app_server import CodexAppServer, CodexAppServerError
from backend.files import Workspace

FIXTURE = str(Path(__file__).parent / 'fixtures' / 'assistant_cli.py')


class AssistantModelTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.folder = Path(self.tmp.name).resolve()
        self.manager = Assistants(Workspace(self.folder, self.folder), 'private-launch-secret')
        self.env = patch.dict(os.environ, {'INDYMAT_ASSISTANT_' + name: FIXTURE for name in ('CLAUDE', 'CODEX', 'AGY')})
        self.env.start()

    def tearDown(self):
        self.manager.close()
        self.env.stop()
        self.tmp.cleanup()

    def wait(self, identity, after=0):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            result = self.manager.events(identity, after)
            if not result['running']: return result
            time.sleep(.02)
        self.fail('Fixture turn timed out')

    def config(self, provider, after=0, **choices):
        result = self.manager.start({'provider': provider, 'mode': 'read-only', 'prompt': 'config', **choices})
        events = self.wait(result['session'], after)['events']
        self.assertEqual(events[-1]['state'], 'completed')
        config = next(json.loads(event['text']) for event in events if event['type'] == 'text' and event['text'].startswith('{'))
        self.assertNotIn('private-launch-secret', json.dumps(config))
        return result['session'], config

    def test_static_claude_and_listed_codex_agy(self):
        claude = self.manager.models('claude')
        self.assertEqual([item['id'] for item in claude['models']], ['', 'fable', 'opus', 'sonnet', 'haiku'])
        self.assertEqual(claude['models'][0]['efforts'], ['low', 'medium', 'high', 'xhigh', 'max'])
        codex = self.manager.models('codex')
        self.assertEqual([item['id'] for item in codex['models']], ['', 'gpt-6.1-sol', 'gpt-6-astra'])
        self.assertEqual(codex['models'][0]['default_effort'], 'medium')
        self.assertTrue(codex['models'][1]['default'])
        agy = self.manager.models('agy')
        self.assertFalse(agy['efforts_separate'])
        self.assertEqual(agy['models'][1]['id'], 'gemini-3.8-flash-high')
        self.assertEqual(agy['models'][1]['label'], 'Gemini 3.8 Flash (High)')
        self.assertFalse(self.manager.model_clients)
        self.assertFalse(self.manager.probes)

    def test_defensive_parsers(self):
        output = 'Fetching available models...\na\tA\na\tDuplicate\n--bad\tB\nb c\tC\nd\t\ne\tbad\x00label\nf\tF\textra\n'
        self.assertEqual(parse_agy_models(output), [{'id': 'a', 'label': 'A', 'efforts': []}])
        self.assertEqual(len(parse_agy_models('\n'.join(f'm{i}\tModel {i}' for i in range(210)))), 200)
        self.assertEqual(codex_models([{'id': 'bad id', 'displayName': 'X'}, {'id': 'secret', 'hidden': True}, {'id': [], 'displayName': 'X'}]), [])

    def test_cache_copies_refresh_and_failure(self):
        catalog = self.manager.model_catalog
        with patch.object(catalog, '_agy', return_value=[{'id': 'a', 'label': 'A', 'efforts': []}]) as discover:
            value = self.manager.models('agy')
            value['models'].clear()
            self.assertEqual(len(self.manager.models('agy')['models']), 2)
            self.assertEqual(discover.call_count, 1)
            timestamp, cached = catalog.cache['agy']
            catalog.cache['agy'] = (timestamp - 601, cached)
            self.manager.models('agy')
            self.assertEqual(discover.call_count, 2)
        catalog.cache.clear()
        with patch.object(catalog, '_codex', side_effect=TimeoutError('secret stderr')) as discover:
            fallback = self.manager.models('codex')
            self.assertEqual(len(fallback['models']), 1)
            self.assertIn('note', fallback)
            self.assertNotIn('secret stderr', fallback['note'])
            self.manager.models('codex')
            self.assertEqual(discover.call_count, 1)

    def test_agy_timeout_and_output_bound_cleanup(self):
        catalog = self.manager.model_catalog
        class Pipe:
            def read(self, size): return b'x' * size
            def close(self): pass
        class Proc:
            stdout = Pipe()
            returncode = 0
            def wait(self, timeout): return 0
        with patch('backend.assistant_models.subprocess.Popen', return_value=Proc()) as launch, patch.object(self.manager, '_kill') as kill:
            self.assertEqual(len(self.manager.models('agy')['models']), 1)
            self.assertIn('note', self.manager.models('agy'))
            self.assertEqual(launch.call_args.args[0], [FIXTURE, 'models'])
            self.assertNotIn('private-launch-secret', json.dumps(launch.call_args.kwargs['env']))
            self.assertTrue(kill.called)
        catalog.cache.clear()
        with patch('backend.assistant_models.subprocess.Popen', side_effect=subprocess.TimeoutExpired('models', 20)):
            self.assertIn('note', self.manager.models('agy'))

    def test_codex_discovery_initialization_failure_releases_child(self):
        real = CodexAppServer
        children = []
        def hanging(*args, **kwargs):
            created = kwargs['on_created']
            def record(child):
                children.append(child)
                created(child)
            kwargs['on_created'] = record
            return real(*args, **kwargs, config_overrides=('fake_hang_initialize=true',))
        with patch.object(real, 'CALL_TIMEOUT', .05), patch('backend.assistant_models.CodexAppServer', side_effect=hanging):
            result = self.manager.models('codex')
        self.assertEqual(len(result['models']), 1)
        self.assertIn('note', result)
        self.assertTrue(children)
        self.assertIsNotNone(children[0].proc.poll())
        self.assertFalse(self.manager.model_clients)

    def test_reject_malformed_and_unlisted_choices_before_launch(self):
        for provider in ('claude', 'codex', 'agy'):
            for value in ('unknown', '--model', 'a b', 'a\nb', 'a\n', 'x' * 129, None, 1, [], {}):
                for key in ('model', 'effort'):
                    with self.subTest(provider=provider, key=key, value=value), self.assertRaises(ValueError):
                        self.manager.start({'provider': provider, 'mode': 'read-only', 'prompt': 'hello', key: value})
        self.assertFalse(self.manager.sessions)
        for provider in ('bogus', '', None, []):
            with self.assertRaises(ValueError): self.manager.models(provider)
        with self.assertRaises(ValueError):
            self.manager.start({'provider': 'codex', 'prompt': 'hello', 'model': 'hidden-model'})
        with self.assertRaises(ValueError):
            self.manager.start({'provider': 'claude', 'prompt': 'hello', 'model': 'sonnet', 'effort': 'ultra'})

    def test_cli_argv_resume_fixed_choices_and_defaults(self):
        for provider, model, effort in [('claude', 'sonnet', 'high'), ('agy', 'gemini-3.8-flash-high', '')]:
            identity, config = self.config(provider, model=model, effort=effort)
            self.assertEqual(config['model'], model)
            self.assertEqual(config['effort'], effort)
            after = self.manager.events(identity)['after']
            _, resumed = self.config(provider, after, conversation=identity)
            self.assertEqual(resumed['model'], model)
            self.assertEqual(resumed['effort'], effort)
            self.assertIn('--resume' if provider == 'claude' else '--conversation', resumed['argv'])
            for changes in ({'model': ''}, {'effort': ''} if effort else {'model': 'claude-opus-5-5-high'}):
                with self.assertRaisesRegex(ValueError, 'Start a new conversation'):
                    self.manager.start({'provider': provider, 'prompt': 'hello', 'conversation': identity, **changes})
            _, default = self.config(provider, model='', effort='')
            self.assertNotIn('--model', default['argv'])
            self.assertNotIn('--effort', default['argv'])

    def test_codex_thread_model_and_per_turn_effort(self):
        identity, config = self.config('codex', model='gpt-6-astra', effort='high')
        self.assertEqual(config['thread']['model'], 'gpt-6-astra')
        self.assertEqual(config['effort'], 'high')
        after = self.manager.events(identity)['after']
        _, second = self.config('codex', after, conversation=identity, effort='low')
        self.assertEqual(second['thread']['model'], 'gpt-6-astra')
        self.assertEqual(second['effort'], 'low')
        after = self.manager.events(identity)['after']
        _, third = self.config('codex', after, conversation=identity)
        self.assertEqual(third['effort'], 'low')
        with self.assertRaisesRegex(ValueError, 'Start a new conversation'):
            self.manager.start({'provider': 'codex', 'prompt': 'hello', 'conversation': identity, 'model': 'gpt-6.1-sol'})
        _, default = self.config('codex')
        self.assertNotIn('model', default['thread'])
        self.assertEqual(default['effort'], '')

    def test_codex_pagination_bound_hidden_and_cursor_cycle(self):
        client = CodexAppServer.__new__(CodexAppServer)
        with patch.object(client, '_call', side_effect=[{'data': [{'id': 'a'}, {'id': 'hidden', 'hidden': True}], 'nextCursor': 'next'}, {'data': [{'id': 'b'}], 'nextCursor': None}]) as call:
            self.assertEqual(client.list_models(), [{'id': 'a'}, {'id': 'b'}])
            self.assertEqual(call.call_args_list[0].args, ('model/list', {}))
            self.assertEqual(call.call_args_list[1].args, ('model/list', {'cursor': 'next'}))
        with patch.object(client, '_call', return_value={'data': [{'id': str(i)} for i in range(250)], 'nextCursor': 'next'}) as call:
            self.assertEqual(len(client.list_models()), 200)
            self.assertEqual(call.call_count, 1)
        with patch.object(client, '_call', return_value={'data': [], 'nextCursor': 'same'}):
            with self.assertRaises(CodexAppServerError): client.list_models()
        with patch.object(client, '_call', return_value={'data': None}):
            with self.assertRaises(CodexAppServerError): client.list_models()

    def test_model_route_authentication_and_start_without_sockets(self):
        from app import Handler
        app = SimpleNamespace(port=8769, token='private-launch-secret', assistants=self.manager, file_lock=self.manager.lock)
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
        for bad in ({'Host': 'evil.invalid'}, {'Origin': 'https://evil.invalid'}, {'X-MF-Token': 'bad'}, {'Sec-Fetch-Site': 'cross-site'}):
            self.assertEqual(request('models?provider=codex', **bad)[0], 403)
        self.assertEqual(request('models?provider=codex')[0], 200)
        self.assertEqual(request('models?provider=bad')[0], 400)
        status, result = request('start', {'provider': 'codex', 'prompt': 'hello', 'model': 'gpt-6-astra', 'effort': 'high'})
        self.assertEqual(status, 202)
        self.assertEqual(self.wait(result['session'])['events'][-1]['state'], 'completed')
        self.assertEqual(request('start', {'provider': 'codex', 'prompt': 'hello', 'model': '--flag'})[0], 400)


if __name__ == '__main__': unittest.main()
