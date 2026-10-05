"""Editor adaptation through real Kernel/Octave, with immutable job staging."""
import json
import tempfile
import time
import unittest
import threading
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from backend.kernel import Kernel
from backend.files import Workspace
from backend.source_adapter import PackageSupport
from backend.source_jobs import validate_source_context
from app import Handler

ROOT = Path(__file__).resolve().parents[1]


class SourceAdapterJobTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.work = Path(self.temp.name) / 'work'
        self.work.mkdir()
        self.kernel = Kernel(ROOT, Path(self.temp.name) / 'runtime', self.work, executable='octave-cli')
        self.wait()

    def tearDown(self):
        self.kernel.close()
        self.temp.cleanup()

    def wait(self, predicate=None):
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            state = self.kernel.snapshot()
            if (predicate(state) if predicate else state['status'] in ('idle', 'dead')):
                return state
            time.sleep(.025)
        self.fail(str(self.kernel.snapshot()))

    def context(self, document, span=None, origin='editor-selection', cursor=0):
        return {'document': document, 'span': span or [0, len(document.encode('utf-16-le')) // 2],
                'path': str(self.work / 'dirty.m'), 'revision': 'submitted-revision',
                'profile': 'matlab', 'origin': origin, 'cursor': cursor}

    def run_editor(self, document, span=None):
        context = self.context(document, span)
        code = document.encode('utf-16-le')[context['span'][0]*2:context['span'][1]*2].decode('utf-16-le')
        job = self.kernel.submit(code, source_context=context)
        state = self.wait()
        self.assertEqual(state['job'], job)
        self.assertEqual(state['status'], 'idle', state)
        return state

    def native(self, code):
        self.kernel.submit(code)
        return self.wait()

    def test_native_default_and_saved_file_are_identical(self):
        code = 'r = "ab" + "cd";'
        state = self.native(code)
        self.assertFalse(state['error'], state)
        self.assertNotIn('source_adapter', state)
        self.assertEqual((self.kernel.runtime/state['job']/'code.m').read_text(), code)
        self.assertEqual(next(v['class'] for v in state['variables'] if v['name'] == 'r'), 'double')
        file = self.work/'saved.m'
        file.write_text('r="native";')
        self.kernel.submit(mode='file', argument=str(file))
        state = self.wait()
        self.assertNotIn('source_adapter', state)
        self.assertEqual(next(v['class'] for v in state['variables'] if v['name'] == 'r'), 'char')
        self.assertEqual(file.read_text(), 'r="native";')

    def test_string_addition_flags_original_metadata_and_no_file_writes(self):
        code = 'r = "ab" + "cd"; total=sum([1 2;3 4],"all");'
        state = self.run_editor(code)
        self.assertFalse(state['error'], state)
        self.assertEqual(state['source_adapter']['status'], 'adapted')
        self.assertEqual(next(v['class'] for v in state['variables'] if v['name'] == 'r'), 'string')
        self.assertFalse(self.native("assert(strcmp(char(r),'abcd')); assert(total==10);")['error'])
        metadata = json.loads((self.kernel.runtime/state['job']/'source.json').read_text())
        self.assertEqual(metadata['original_text'], code)
        self.assertIn("(@string)('ab')", metadata['adapted_text'])
        self.assertTrue(metadata['source_map']['pieces'])
        self.assertEqual(metadata['epoch'], self.kernel.generation)
        self.assertFalse((self.work/'dirty.m').exists())

    def test_quoted_command_falls_back_whole_unit(self):
        code = 'r="x"; disp "hello"'
        state = self.run_editor(code)
        self.assertEqual(state['source_adapter']['status'], 'fallback')
        self.assertEqual(state['source_adapter']['diagnostics'][0]['code'], 'quoted-command')
        self.assertEqual((self.kernel.runtime/state['job']/'code.m').read_text(), code)

    def test_missing_or_unpatched_constructor_falls_back(self):
        with patch.object(self.kernel, '_source_package', return_value=PackageSupport(reason='Unpatched constructor.')):
            state = self.run_editor('r="x";')
        self.assertEqual(state['source_adapter']['status'], 'fallback')
        self.assertEqual(state['source_adapter']['diagnostics'][0]['code'], 'package-unverified')
        self.assertFalse(self.native('pkg unload statistics; pkg unload datatypes;')['error'])
        state = self.run_editor('r="x";')
        self.assertEqual(state['source_adapter']['status'], 'fallback')

    def test_live_shadowed_constructor_and_variable_shadowing(self):
        self.native('string=17; kept=@(x)x+1;')
        state = self.run_editor('r="a"+"b";')
        self.assertEqual(state['source_adapter']['status'], 'adapted')
        self.assertFalse(self.native("assert(string==17); assert(kept(2)==3); assert(strcmp(char(r),'ab'));")['error'])
        (self.work/'string.m').write_text('function y=string(x)\ny=x;\nendfunction\n')
        self.native('rehash;')
        state = self.run_editor('r="x";')
        self.assertEqual(state['source_adapter']['status'], 'fallback')

    def test_midline_unicode_location_and_failed_job_never_retries(self):
        self.native('attempts=0;')
        prefix = '% 😀\nignored=1; '
        code = 'attempts=attempts+1; r="ş"; missing_adapter_value;'
        document = prefix + code
        start = len(prefix.encode('utf-16-le')) // 2
        state = self.run_editor(document, [start, len(document.encode('utf-16-le')) // 2])
        self.assertEqual(state['source_adapter']['status'], 'adapted')
        self.assertIn('missing_adapter_value', state['error'])
        location = state['source_error_locations'][0]
        # Octave reports the byte-column of the undefined name; the adapter
        # removes constructor expansion and converts Unicode to editor UTF-16.
        expected = len(('ignored=1; '+code[:code.index('missing_adapter_value')]).encode('utf-16-le')) // 2 + 1
        self.assertEqual((location['line'], location['column']), (2, expected))
        self.assertNotEqual(state['error'], state['raw_error'])
        self.assertFalse(self.native('assert(attempts==1);')['error'])

    def assert_later_line_error(self, document, name, span=None):
        state = self.run_editor(document, span)
        self.assertEqual(state['source_adapter']['status'], 'adapted')
        self.assertIn(name, state['raw_error'])
        metadata = json.loads((self.kernel.runtime/state['job']/'source.json').read_text())
        generated_line = next(line for line in metadata['adapted_text'].splitlines() if name in line)
        byte_column = len(generated_line[:generated_line.index(name)].encode('utf-8')) + 1
        # Measured evalin calibration: these later top-level statements are
        # reported as line 1, with the correct generated physical byte column.
        self.assertIn(f'near line 1, column {byte_column}', state['raw_error'])
        before = document[:document.index(name)]
        expected_line = before.count('\n') + 1
        expected_column = len(before.rsplit('\n', 1)[-1].encode('utf-16-le')) // 2 + 1
        location, = state['source_error_locations']
        self.assertEqual((location['line'], location['column']), (expected_line, expected_column))
        self.assertEqual(location['start_utf16'], len(before.encode('utf-16-le')) // 2)
        self.assertFalse(location['adapted'])
        self.assertEqual((location['job'], location['epoch']), (state['job'], self.kernel.generation))
        return state

    def test_two_line_ui_error_after_literal(self):
        code = 'attempts=0;\nattempts=attempts+1; r="x"; missing_adapter_ui;'
        state = self.assert_later_line_error(code, 'missing_adapter_ui')
        self.assertEqual(state['source_error_locations'][0]['column'], 29)
        self.assertFalse(self.native('assert(attempts==1);')['error'])

    def test_second_line_error_before_literal(self):
        self.assert_later_line_error('a=1;\nmissing_before; r="x";', 'missing_before')

    def test_third_line_error_after_two_literals(self):
        self.assert_later_line_error('a=1;\nb=2;\nr="a"; s="b"; missing_two;', 'missing_two')

    def test_midline_span_error_on_second_execution_line(self):
        prefix = '% not submitted\nignored=1; '
        document = prefix + 'r="first";\ns="second"; missing_midline;'
        start = len(prefix.encode('utf-16-le')) // 2
        self.assert_later_line_error(document, 'missing_midline',
                                    [start, len(document.encode('utf-16-le')) // 2])

    def test_later_line_unicode_before_error(self):
        self.assert_later_line_error('a=1;\nr="ş😀"; missing_unicode;', 'missing_unicode')

    def test_ambiguous_evalin_lines_do_not_create_a_source_link(self):
        code = 'a=1;\nif false; missing_repeated; end;\nif true;  missing_repeated; end; r="x";'
        state = self.run_editor(code)
        self.assertEqual(state['source_adapter']['status'], 'adapted')
        self.assertIn('missing_repeated', state['raw_error'])
        self.assertEqual(state['source_error_locations'], [])
        self.assertEqual(state['error'], state['raw_error'])

    def test_later_parse_error_without_location_never_guesses(self):
        code = 'a=1;\nr="x"; z=;'
        state = self.run_editor(code)
        # Unlike the single-line parse fixture, evalin supplies no source line
        # or caret for this later parse. A bare message cannot authorize a link.
        self.assertEqual(state['raw_error'], 'syntax error\n')
        self.assertEqual(state['error'], state['raw_error'])
        self.assertEqual(state['source_error_locations'], [])

    def test_snapshot_retained_while_buffer_changes(self):
        code = 'r="old"; pause(.2);'
        context = self.context(code)
        job = self.kernel.submit(code, source_context=context)
        context['document'] = 'r="edited";'
        state = self.wait()
        metadata = json.loads((self.kernel.runtime/job/'source.json').read_text())
        self.assertEqual(metadata['source_context']['document'], code)
        self.assertFalse(state['error'], state)

    def test_parse_caret_maps_to_original_source_and_keeps_raw_error(self):
        code = 'r="x"; z=;'
        state = self.run_editor(code)
        self.assertEqual(state['source_adapter']['status'], 'adapted')
        self.assertIn('>>> '+code, state['error'])
        self.assertIn("(@string)('x')", state['raw_error'])
        self.assertEqual(state['source_error_locations'][0]['line'], 1)
        self.assertEqual(state['source_error_locations'][0]['column'], len(code))

    def test_preparation_failure_leaves_kernel_idle(self):
        previous = self.kernel.snapshot()['job']
        folders = set(self.kernel.runtime.iterdir())
        original_write = Path.write_text
        def fail_source(path, *args, **kwargs):
            if path.name == 'source.json': raise OSError('preparation fixture')
            return original_write(path, *args, **kwargs)
        with patch.object(Path, 'write_text', fail_source), self.assertRaises(ValueError):
            self.kernel.submit('r="x";', source_context=self.context('r="x";'))
        self.assertEqual(self.kernel.snapshot()['job'], previous)
        self.assertEqual(self.kernel.snapshot()['status'], 'idle')
        self.assertEqual(set(self.kernel.runtime.iterdir()), folders)

    def test_stop_and_input_guards_preserve_job_and_epoch(self):
        code = 'r="x"; v=input(\'Number: \'); assert(v==17);'
        job = self.kernel.submit(code, source_context=self.context(code))
        self.wait(lambda s: s['waiting_input'])
        self.kernel.input('17')
        self.assertFalse(self.wait()['error'])
        code = 'r="x"; while true; end'
        epoch = self.kernel.generation
        job = self.kernel.submit(code, source_context=self.context(code))
        with self.assertRaises(ValueError): self.kernel.input('clear')
        with self.assertRaises(ValueError): self.kernel.submit('z=1;')
        self.kernel.interrupt()
        self.kernel.interrupt()
        state = self.wait()
        self.assertEqual(state['job'], job)
        self.assertEqual(self.kernel.generation, epoch)
        self.assertFalse(self.native('assert(isa(r,\'string\'));')['error'])

    def test_debugger_remains_native_and_stop_is_idempotent(self):
        file = self.work/'adapter_debug.m'
        file.write_text('function r=adapter_debug()\nx="native";\nr=class(x);\nendfunction\n')
        self.kernel.configure_breakpoint(file, 3, 'set')
        self.wait()
        code = 'r="adapted"; answer=adapter_debug();'
        job = self.kernel.submit(code, source_context=self.context(code))
        paused = self.wait(lambda s: s['status']=='paused' and s.get('debug',{}).get('ready'))
        with self.assertRaises(ValueError): self.kernel.submit(code, source_context=self.context(code))
        self.kernel.debug('eval', 'assert(ischar(x));')
        self.wait(lambda s: s['status']=='paused' and s['debug'].get('ready') and s['debug']['serial'] != paused['debug']['serial'])
        self.kernel.interrupt()
        self.kernel.interrupt()
        self.assertEqual(self.wait()['job'], job)

    def test_app_validates_snapshot_identity_utf16_and_section_boundaries(self):
        workspace = Workspace(self.work, allowed_root=self.work)
        document = '%% one\nr="a";\n%% two\nr="b";\n'
        context = self.context(document, [0, document.index('\n%% two')], 'editor-section', 9)
        code = document[:context['span'][1]]
        accepted = validate_source_context(context, code, 'code', workspace)
        self.assertEqual(accepted['span'], context['span'])
        for bad in [{**context, 'profile':'native-octave'}, {**context,'origin':'command-window'},
                    {**context,'span':[1,3]}, {**context,'generated_text':'injected'},
                    {**context,'document':'😀', 'span':[1,2]}]:
            with self.assertRaises((ValueError, PermissionError)):
                validate_source_context(bad, code, 'code', workspace)
        with self.assertRaises(ValueError): validate_source_context(context, code, 'file', workspace)

    def test_http_handler_uses_real_kernel_and_preserves_native_response(self):
        workspace = Workspace(self.work, allowed_root=self.work)
        handler = Handler.__new__(Handler)
        responses = []
        handler.server = SimpleNamespace(app=SimpleNamespace(kernel=self.kernel, workspace=workspace, file_lock=threading.RLock()))
        handler.send = lambda status, value: responses.append((status, value))
        code = 'r="ab"+"cd";'
        handler.post('/api/execute', {'code': code, 'mode':'code', 'argument':'', 'history':False})
        self.assertEqual(responses[-1][0], 202)
        self.assertEqual(set(responses[-1][1]), {'job'})
        self.assertNotIn('source_adapter', self.wait())
        handler.post('/api/execute', {'code': code, 'mode':'code', 'argument':'', 'history':False,
                                     'adapt_editor_literals':True, 'source_context':self.context(code)})
        reply = responses[-1][1]
        self.assertEqual(reply['source_adapter']['job'], reply['job'])
        self.assertEqual(reply['source_adapter']['status'], 'adapted')
        self.assertFalse(self.wait()['error'])

    def test_private_source_snapshot_cannot_use_normal_file_routes(self):
        state = self.run_editor('r="x";')
        workspace = Workspace(self.work, allowed_root=Path(self.temp.name))
        workspace.source_private_roots = (self.kernel.runtime,)
        with self.assertRaises(PermissionError):
            workspace.path(str(self.kernel.runtime/state['job']/'source.json'))
        with self.assertRaises(PermissionError):
            workspace.folder(self.kernel.runtime/state['job'])


if __name__ == '__main__': unittest.main()
