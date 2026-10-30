"""backend.kernel_rust.RustKernel against the real indy-kernel binary (the interpreter; StubEngine via INDYMAT_R_STUB). No Octave."""
import base64
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from backend import kernel_rust  # noqa: E402
from backend.kernel_rust import RustKernel, kernel_executable, OctaveFreeServices  # noqa: E402
from backend.figure_artifacts import read_figure_artifact  # noqa: E402
from tests.rust_engine_support import require_rust_kernel  # noqa: E402


binary = require_rust_kernel


class RustKernelTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.executable = binary()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name).resolve()
        (base / 'work').mkdir()
        self.work = base / 'work'
        self.k = RustKernel(ROOT, base / 'jobs', self.work, executable=self.executable)

    def tearDown(self):
        self.k.close()
        self.tmp.cleanup()

    def wait(self, predicate, timeout=10):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            state = self.k.snapshot()
            if predicate(state):
                return state
            time.sleep(.02)
        self.fail(f'condition not reached: {self.k.snapshot()}')

    def run_code(self, code, **kwargs):
        job = self.k.submit(code, 'code', **kwargs)
        return self.wait(lambda s: s['job'] == job and s['status'] in ('idle', 'dead'))

    def names(self, state):
        return [v['name'] for v in state['variables']]

    def test_start_and_x_equals_3_round_trip(self):
        state = self.k.snapshot()
        self.assertEqual(state['status'], 'idle')
        self.assertEqual(state['version'], 'IndyMAT Rust engine (experimental)')
        self.assertEqual(state['engine'], {'name': 'indy', 'stub': False})
        self.assertEqual(state['cwd'], str(self.work))
        state = self.run_code('x = 3')
        self.assertEqual(state['status'], 'idle')
        self.assertIsNone(state['error'])
        self.assertEqual(state['output'], 'x = 3\n')
        self.assertEqual(state['variables'], [{'name': 'x', 'size': '1x1', 'class': 'double', 'bytes': 8,
                                               'complex': False, 'preview': '3', 'global': False}])
        job = self.k.submit(mode='inspect', argument='x')
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertEqual(state['detail'], {'name': 'x', 'class': 'double', 'size': [1, 1], 'rows': [['3']], 'text': '', 'truncated': False})
        job = self.k.workspace_action('variable-read', {'name': 'x', 'path': [], 'slices': [], 'row': 1, 'column': 1,
                                                        'rows': 100, 'columns': 30, 'epoch': self.k.generation})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        page = state['variable_action']
        self.assertEqual((page['action'], page['kind'], page['rows'], page['root_size']), ('read', 'matrix', [['3']], [1, 1]))

    def test_errors_keep_earlier_variables(self):
        state = self.run_code('a = 1; b = nope')
        self.assertEqual(state['error'], "Undefined function or variable 'nope'.")
        self.assertEqual(self.names(state), ['a'])
        state = self.run_code('A = magic(3); B = A*A\'')
        self.assertIsNone(state['error'])
        self.assertIn('101    71    53', state['output'])
        self.assertEqual(self.names(state), ['A', 'B', 'a'])
        state = self.run_code("annotation('line')")
        self.assertEqual(state['error'], 'annotation: graphics not yet available in the Rust engine')
        self.assertEqual(state['figures'], [])

    def figure_data(self, figure):
        return json.loads(read_figure_artifact(self.k.runtime, figure['job'], figure['data_file']))

    def validate(self, data):
        """The UI's own admission gate (frontend/figure_data_utils.cjs validate())."""
        if not shutil.which('node'):
            self.skipTest('node is needed for the figure data gate')
        script = ('const u=require(process.argv[1]);const r=u.validate(JSON.parse(require("fs").readFileSync(0,"utf8")));'
                  'process.stdout.write(JSON.stringify({ok:r.ok,supported:r.supported,reason_code:r.reason_code}))')
        out = subprocess.run(['node', '-e', script, str(ROOT / 'frontend' / 'figure_data_utils.cjs')], input=json.dumps(data),
                             capture_output=True, text=True, check=True).stdout
        return json.loads(out)

    def test_print_writes_files_the_engine_draws(self):
        state = self.run_code("plot(1:10); print('cikti.png', '-dpng'); saveas(gcf, 'kopya.svg'); exportgraphics(gcf, 'sayfa.pdf');")
        self.assertIsNone(state['error'])
        self.assertTrue((self.work / 'cikti.png').read_bytes().startswith(b'\x89PNG'))
        self.assertIn(b'<svg', (self.work / 'kopya.svg').read_bytes()[:200])
        self.assertTrue((self.work / 'sayfa.pdf').read_bytes().startswith(b'%PDF'))

    def test_assistant_figure_image_returns_the_engine_png(self):
        from types import SimpleNamespace
        from backend.assistant_bridge_service import BridgeService
        runtime = Path(self.tmp.name).resolve() / '.matlab-free'
        runtime.mkdir(mode=0o700)
        service = BridgeService(SimpleNamespace(runtime=runtime, base='http://127.0.0.1:9999', token='direct-launch-secret', port=9999, kernel=self.k))
        paths = service.create('a' * 32, 'claude', 'inspect')
        grant = service.authenticate(json.loads(Path(paths['capability']).read_text())['token'])
        self.assertIsNone(self.run_code('plot(1:3)')['error'])
        image = service.call(grant, {'name': 'figure_image', 'arguments': {}})
        self.assertFalse(image['isError'])
        self.assertEqual(image['content'][0]['mimeType'], 'image/png')
        self.assertTrue(base64.b64decode(image['content'][0]['data']).startswith(b'\x89PNG'))

    def test_figures_are_stored_as_figure_artifacts(self):
        state = self.run_code('plot(1:3)')
        self.assertIsNone(state['error'])
        job = state['job']
        self.assertEqual(state['figures'], [{'name': 'Figure 1', 'file': 'figure-1.png', 'number': 1, 'data_file': 'figure-1.json',
                                             'interactive': True, 'decimated': False, 'fallback_reason': '', 'job': job}])
        data = self.figure_data(state['figures'][0])
        # The export identity names this job and ordinal, as __mf_execute__ does for Octave.
        self.assertEqual((data['source']['job'], data['source']['figure']), (job, 1))
        self.assertEqual(self.validate(data), {'ok': True, 'supported': True, 'reason_code': ''})
        # The figure's PNG lies next to its JSON, as with the Octave kernel.
        png = (self.k.runtime / job / 'figure-1.png').read_bytes()
        self.assertTrue(png.startswith(b'\x89PNG\r\n\x1a\n') and png.endswith(b'IEND\xaeB`\x82'))
        self.assertEqual(int.from_bytes(png[16:20], 'big'), 700)  # 560 px at 120 dpi
        # An empty figure is a contract fallback: not interactive, a blank PNG.
        state = self.run_code("figure('Name', 'Bos');")
        self.assertEqual([(f['number'], f['name'], f['interactive']) for f in state['figures']], [(1, 'Figure 1', True), (2, 'Bos', False)])
        empty = state['figures'][1]
        self.assertTrue(empty['fallback_reason'])
        data = self.figure_data(empty)
        self.assertEqual((data['source']['job'], data['source']['figure']), (state['job'], 2))
        self.assertEqual(self.validate(data), {'ok': True, 'supported': False, 'reason_code': 'no_axes'})
        self.assertTrue((self.k.runtime / state['job'] / 'figure-2.png').read_bytes().startswith(b'\x89PNG'))
        # Every eval reports the complete list, also after an error.
        state = self.run_code('figure(1); error("boom")')
        self.assertEqual(state['error'], 'boom')
        self.assertEqual([f['number'] for f in state['figures']], [1, 2])
        self.assertTrue(all(f['job'] == state['job'] for f in state['figures']))
        state = self.run_code('subplot(2,1,1); bar([3 1 2]); title(\'x\'); figure; surf(peaks(20));')
        for figure in state['figures']:
            self.assertTrue(self.validate(self.figure_data(figure))['ok'], figure)
        self.assertEqual(self.run_code('close all')['figures'], [])
        self.run_code('plot(1:3)')
        self.k.reset()
        self.assertEqual(self.k.snapshot()['figures'], [])
        self.assertEqual(self.run_code('x = 1;')['figures'], [])

    def test_input_reply(self):
        job = self.k.submit("a = input('A? ');", 'code')
        self.wait(lambda s: s['waiting_input'])
        with self.assertRaises(ValueError):
            self.k.input('two\nlines')
        self.k.input('2+3')
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIsNone(state['error'])
        self.assertEqual(state['output'], 'A? ')
        self.assertEqual(state['variables'][0]['preview'], '5')
        with self.assertRaises(ValueError):
            self.k.input('6')

    def test_interrupt_twice_keeps_the_session(self):
        self.run_code('x = 3;')
        for code in ('pause(30)', 'for k = 1:1e9, end'):
            job = self.k.submit(code, 'code')
            self.wait(lambda s: s['status'] == 'running' and self.k._eval_id is not None)
            time.sleep(.2)
            self.k.interrupt()
            self.k.interrupt()
            started = time.monotonic()
            state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle', timeout=5)
            self.assertLess(time.monotonic() - started, 2)
            self.assertEqual(state['error'], 'Operation stopped.')
        self.assertEqual(self.names(state), ['k', 'x'])
        self.k.interrupt()  # idle: no effect
        state = self.run_code('disp(x)')
        self.assertIsNone(state['error'])
        self.assertTrue(state['output'].endswith('3\n'))
        # Interrupt while waiting for input.
        job = self.k.submit("y = input('? ')", 'code')
        self.wait(lambda s: s['waiting_input'])
        self.k.interrupt()
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertEqual(state['error'], 'Operation stopped.')
        self.assertFalse(state['waiting_input'])

    def test_time_limit(self):
        state = self.run_code('pause(30)', timeout=0.3)
        self.assertEqual(state['error'], 'The 0.3-second time limit was exceeded.')
        self.assertEqual(state['status'], 'idle')

    def test_reset(self):
        self.run_code('x = 3;')
        generation = self.k.generation
        pid = self.k.session.proc.pid
        self.k.reset()
        state = self.k.snapshot()
        self.assertEqual((state['status'], state['variables']), ('idle', []))
        self.assertGreater(self.k.generation, generation)
        self.assertEqual(self.k.session.proc.pid, pid)  # idle reset reuses the process
        # A reset while running restarts the process.
        self.k.submit('pause(30)', 'code')
        self.wait(lambda s: s['status'] == 'running' and self.k._eval_id is not None)
        self.k.reset()
        self.assertNotEqual(self.k.session.proc.pid, pid)
        state = self.run_code('z = 1')
        self.assertEqual(self.names(state), ['z'])

    def test_workspace_actions(self):
        self.run_code('a = 1; b = 2; c = 3;')
        job = self.k.workspace_action('workspace-rename', {'old_name': 'a', 'new_name': 'q'})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertEqual(self.names(state), ['b', 'c', 'q'])
        self.assertEqual(state['workspace_action'], {'action': 'workspace-rename', 'old_name': 'a', 'new_name': 'q'})
        job = self.k.workspace_action('workspace-clear-names', {'names': ['b', 'q']})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertEqual(self.names(state), ['c'])
        job = self.k.workspace_action('workspace-rename', {'old_name': 'nope', 'new_name': 'r'})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIn('nope', state['error'])

    def test_sparse_workspace_detail_and_mat_round_trip(self):
        self.run_code("S = sparse([1 2],[1 2],[3 4.5]); C = sparse(2,1,1-2i,2,2);")
        state = self.wait(lambda s: s['status'] == 'idle')
        rows = {v['name']: v for v in state['variables']}
        self.assertEqual(rows['S']['preview'], 'sparse [3 0;0 4.5]')
        self.assertEqual(rows['S']['class'], 'double')
        self.assertFalse(rows['S']['complex'])
        self.assertTrue(rows['C']['complex'])
        path = str(self.work / 'sparse.mat')
        job = self.k.workspace_action('workspace-save', {'path': path, 'all': True, 'names': []})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIsNone(state['error'])
        self.assertEqual(state['workspace_action']['names'], ['C', 'S'])
        self.run_code("clear S C; X = load('%s'); ok = issparse(X.S) && issparse(X.C) && nnz(X.S) == 2; disp(ok)" % path)
        state = self.wait(lambda s: s['status'] == 'idle')
        self.assertIn('1', state['output'])

    def test_handles_and_strings_through_save_and_load(self):
        self.run_code("""a = 3; n = @sin; f = @(x) x + a; q = "metin";
            c = {1, @cos}; s = struct('n', 5, 't', {{1, table([1;2])}});""")
        self.wait(lambda s: s['status'] == 'idle')
        path = str(self.work / 'tutamac.mat')
        job = self.k.workspace_action('workspace-save', {'path': path, 'all': True, 'names': []})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIsNone(state['error'])
        self.assertEqual(state['workspace_action']['names'], ['a', 'c', 'f', 'n', 'q', 's'])
        self.assertIn("warning: 's.t{2}' was saved as []", state['output'])
        self.run_code('clear')
        self.wait(lambda s: s['status'] == 'idle')
        job = self.k.workspace_action('workspace-load-inspect', {'path': path})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        action = state['workspace_action']
        classes = {v['name']: v['class'] for v in action['variables']}
        self.assertEqual(classes['f'], 'function_handle')
        self.assertEqual(classes['q'], 'string')
        job = self.k.workspace_action('workspace-load', {'path': path, 'inventory': action['variables'], 'replacements': []})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIsNone(state['error'])
        self.run_code("disp([f(2), n(0), c{2}(0), s.n, isempty(s.t{2})]); disp(q); disp(class(q))")
        state = self.wait(lambda s: s['status'] == 'idle')
        self.assertEqual(state['output'].split('\n')[0].split(), ['5', '0', '1', '5', '1'])
        self.assertIn('metin', state['output'])
        self.assertIn('string', state['output'])

    def test_mat_save_inspect_and_load(self):
        self.run_code("x = magic(3); s.a = 'metin'; f = table([1;2]);")
        path = str(self.work / 'stage.mat')
        job = self.k.workspace_action('workspace-save', {'path': path, 'all': True, 'names': []})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIsNone(state['error'])
        self.assertEqual(state['workspace_action'], {'action': 'workspace-save', 'path': path, 'names': ['s', 'x']})
        self.assertIn("warning: Variable 'f' was not saved", state['output'])
        self.assertTrue(Path(path).read_bytes().startswith(b'MATLAB 5.0 MAT-file'))
        job = self.k.workspace_action('workspace-save', {'path': str(self.work / 'sel.mat'), 'all': False, 'names': ['x']})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertEqual(state['workspace_action']['names'], ['x'])
        job = self.k.workspace_action('workspace-save', {'path': str(self.work / 'no.mat'), 'all': False, 'names': ['nope']})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIn('no longer exists', state['error'])

        self.run_code('clear s; x = 0;')
        job = self.k.workspace_action('workspace-load-inspect', {'path': path})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        action = state['workspace_action']
        self.assertEqual(action['variables'], [{'name': 's', 'size': '1x1', 'class': 'struct', 'bytes': 10},
                                               {'name': 'x', 'size': '3x3', 'class': 'double', 'bytes': 72}])
        self.assertEqual(action['replacements'], ['x'])
        self.assertEqual(self.names(state), ['f', 'x'])
        # A stale confirmation assigns nothing.
        job = self.k.workspace_action('workspace-load', {'path': path, 'inventory': action['variables'], 'replacements': []})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIn('changed since confirmation', state['error'])
        self.assertEqual(self.names(state), ['f', 'x'])
        job = self.k.workspace_action('workspace-load', {'path': path, 'inventory': action['variables'], 'replacements': ['x']})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIsNone(state['error'])
        self.assertEqual(state['workspace_action'], {'action': 'workspace-load', 'path': path, 'names': ['s', 'x'], 'replacements': ['x']})
        self.assertEqual(self.names(state), ['f', 's', 'x'])
        self.assertFalse(self.run_code("assert(isequal(x, magic(3)) && strcmp(s.a, 'metin'))")['error'])
        job = self.k.workspace_action('workspace-load-inspect', {'path': str(self.work / 'missing.mat')})
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIn('not found', state['error'])
        with self.assertRaises(ValueError):
            self.k.workspace_action('workspace-save', {'all': True})

    def test_cd_and_file_run(self):
        sub = self.work / 'sub'
        sub.mkdir()
        script = sub / 'script.m'
        script.write_text('q = 5\nr = not_defined_here\n')
        state = self.run_code(f'cd({self.k.quote(sub)});')
        self.assertEqual(state['cwd'], str(sub))
        job = self.k.submit(mode='file', argument=str(script))
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertEqual(state['error'], "Undefined function or variable 'not_defined_here'.\n  script:2")
        self.assertIn('q = 5', state['output'])

    def test_unsupported_features_are_explicit(self):
        for call, text in ((lambda: self.k.package('load', 'signal'), 'Packages are not supported'),
                           (lambda: self.k.submit(mode='profile', argument='a.m'), 'profiler is not supported'),
                           (lambda: self.k.submit(mode='publish', argument='a.m'), 'Publishing is not supported'),
                           (lambda: self.k.workspace_action('workspace-assign-scalar', {}), 'Editing Workspace values'),
                           (lambda: self.k.workspace_action('variable-write', {}), 'Editing variables'),
                           (lambda: self.k.submit('x=1', source_context={}), 'Source adaptation')):
            with self.subTest(text=text):
                with self.assertRaises(ValueError) as caught:
                    call()
                self.assertIn(text, str(caught.exception))
                self.assertEqual(self.k.snapshot()['status'], 'idle')
        self.assertIsNone(self.k.relocate_file_breakpoints(self.work / 'a.m', self.work / 'b.m'))
        services = OctaveFreeServices()
        self.assertFalse(services.help('sin', str(self.work))['found'])
        with self.assertRaises(ValueError):
            services.check('x = 1')

    def test_unsupported_features_are_listed_for_the_ui(self):
        unsupported = self.k.snapshot()['unsupported']
        for feature in ('debugger', 'breakpoints', 'run_to_cursor'):
            self.assertNotIn(feature, unsupported)
        for feature in ('packages', 'profiler', 'publish',
                        'workspace_edit', 'variable_editor_write', 'figure_rotate', 'help', 'lint'):
            self.assertIn(feature, unsupported)
        # The real engine reads and writes MAT files: the Workspace Save/Load buttons are shown.
        self.assertNotIn('mat_files', unsupported)
        self.k.reset()
        self.assertNotIn('debugger', self.k.snapshot()['unsupported'])
        self.assertIn('profiler', self.k.snapshot()['unsupported'])

    def test_busy_and_close(self):
        self.k.submit('pause(30)', 'code')
        with self.assertRaises(ValueError):
            self.k.submit('x = 1', 'code')
        proc = self.k.session.proc
        self.k.close()
        self.assertIsNotNone(proc.poll())

    def paused(self, line=None):
        return self.wait(lambda s: s['status'] == 'paused' and s['debug'].get('ready') and (line is None or s['debug']['line'] == line))

    def idle(self):
        return self.wait(lambda s: s['status'] == 'idle')

    def test_a_refused_debug_request_does_not_outlive_the_next_step(self):
        path = self.work / 'steps.m'
        path.write_text('a = 1;\nb = 2;\nc = 3;\n')
        self.k.breakpoint(path, 1, True)
        self.idle()
        self.k.submit('', 'file', str(path))
        self.paused(1)
        self.k.debug('inspect', 'no_such_variable')
        self.assertIn('no_such_variable', self.paused(1)['error'])
        self.k.debug('step')
        self.assertIsNone(self.paused(2)['error'])
        self.k.interrupt()
        self.idle()

    def test_a_refused_inspect_keeps_a_pending_run_to_cursor(self):
        path = self.work / 'pending.m'
        path.write_text('a = 1;\nb = 2;\nc = 3;\nd = 4;\ne = 5;\n')
        for line in (1, 3):
            self.k.breakpoint(path, line, True)
            self.idle()
        self.k.submit('', 'file', str(path))
        self.paused(1)
        self.k.run_to_cursor(path, 5)
        self.assertEqual(self.paused(3)['run_to_cursor']['line'], 5)
        self.k.debug('inspect', 'no_such_variable')
        self.assertEqual(self.paused(3)['run_to_cursor']['line'], 5)
        self.k.interrupt()
        self.idle()

    def test_a_refused_run_to_cursor_is_no_longer_pending(self):
        path = self.work / 'refused.m'
        path.write_text('a = 1;\nb = 2;\n')
        other = self.work / 'other.txt.m'
        other.write_text('\n')
        self.k.breakpoint(path, 1, True)
        self.idle()
        self.k.submit('', 'file', str(path))
        self.paused(1)
        self.k.run_to_cursor(other, 1)
        state = self.paused(1)
        self.assertIsNotNone(state['error'])
        self.assertNotIn('run_to_cursor', state)
        self.k.interrupt()
        self.idle()

    def test_a_breakpoint_snapped_onto_another_keeps_the_existing_one(self):
        path = self.work / 'snap.m'
        path.write_text('a = 1;\n\n\nb = 2;\n')
        self.k.breakpoint(path, 4, True, 'a == 2')
        self.idle()
        self.k.breakpoint(path, 2, True)
        state = self.idle()
        self.assertIsNone(state['error'])
        points = state['breakpoints'][0]['breakpoints']
        self.assertEqual([(p['line'], p['condition']) for p in points], [(4, 'a == 2')])
        # The engine agrees: the condition is false, so the file runs through.
        job = self.k.submit('', 'file', str(path))
        self.assertEqual(self.wait(lambda s: s['job'] == job and s['status'] in ('idle', 'paused'))['status'], 'idle')

    def test_a_snapped_breakpoint_onto_a_disabled_one_stays_disabled(self):
        path = self.work / 'snapoff.m'
        path.write_text('a = 1;\n\nb = 2;\n')
        self.k.breakpoint(path, 3, True)
        self.idle()
        self.k.configure_breakpoint(path, 3, 'disable')
        self.idle()
        self.k.breakpoint(path, 2, True)
        points = self.idle()['breakpoints'][0]['breakpoints']
        self.assertEqual([(p['line'], p['enabled']) for p in points], [(3, False)])
        job = self.k.submit('', 'file', str(path))
        self.assertEqual(self.wait(lambda s: s['job'] == job and s['status'] in ('idle', 'paused'))['status'], 'idle')

    def test_breakpoints_reapplied_to_an_edited_file_follow_the_engine(self):
        path = self.work / 'edited.m'
        path.write_text('a = 1;\nb = 2;\nc = 3;\n')
        self.k.breakpoint(path, 2, True)
        self.idle()
        path.write_text('a = 1;\n\nc = 3;\n')
        self.k.session.kill()
        self.k.reset()
        self.assertEqual(self.idle()['breakpoints'][0]['lines'], [3])

    def test_a_relocated_breakpoint_follows_the_engine_too(self):
        source = self.work / 'moved_a.m'
        target = self.work / 'moved_b.m'
        source.write_text('a = 1;\nb = 2;\nc = 3;\n')
        self.k.breakpoint(source, 2, True)
        self.idle()
        source.rename(target)
        target.write_text('a = 1;\n\nc = 3;\n')
        self.k.relocate_file_breakpoints(source, target)
        state = self.idle()
        self.assertEqual(state['breakpoints'][0]['lines'], [3])
        self.assertEqual(state['breakpoint_relocation']['moved'][0]['line'], 3)

    def test_a_busy_relocation_leaves_no_phantom_breakpoint_in_the_engine(self):
        path = self.work / 'phantom.m'
        path.write_text('x = 1;\ny = 2;\n')
        moved = self.work / 'phantom_b.m'
        self.k.breakpoint(path, 2, True)
        self.idle()
        self.k.submit('pause(0.5)', 'code')
        path.rename(moved)
        with self.assertRaises(ValueError):
            self.k.relocate_file_breakpoints(path, moved)
        self.assertEqual(self.k.snapshot()['breakpoints'], [])
        self.idle()
        moved.rename(path)
        job = self.k.submit('', 'file', str(path))
        self.assertEqual(self.wait(lambda s: s['job'] == job and s['status'] in ('idle', 'paused'))['status'], 'idle')

    def test_a_breakpoint_set_after_a_busy_relocation_survives_the_cleanup(self):
        path = self.work / 'again.m'
        path.write_text('x = 1;\ny = 2;\n')
        moved = self.work / 'again_b.m'
        self.k.breakpoint(path, 2, True)
        self.idle()
        self.k.submit('pause(0.3)', 'code')
        path.rename(moved)
        with self.assertRaises(ValueError):
            self.k.relocate_file_breakpoints(path, moved)
        self.idle()
        moved.rename(path)
        self.k.breakpoint(path, 1, True)
        self.idle()
        self.k.submit('', 'file', str(path))
        self.assertEqual(self.paused()['debug']['line'], 1)
        self.k.interrupt()
        self.idle()

    def test_stop_during_a_mat_save_says_so_and_reports_the_real_result(self):
        self.run_code('x = 1:1e6;')
        target = self.work / 'saved.mat'
        job = self.k.workspace_action('workspace-save', {'path': str(target), 'all': True})
        try:
            self.k.interrupt()
        except ValueError as caught:
            self.assertIn('cannot be stopped', str(caught))
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIsNone(state['error'])
        self.assertTrue(target.exists())
        self.assertEqual(state['workspace_action']['action'], 'workspace-save')

    def test_stop_during_a_short_request_is_quiet_and_changes_nothing(self):
        self.run_code('x = 1;')
        job = self.k.submit('', 'inspect', 'x')
        self.k.interrupt()
        state = self.wait(lambda s: s['job'] == job and s['status'] == 'idle')
        self.assertIsNone(state['error'])
        self.assertEqual(state['detail']['name'], 'x')

    def test_an_explicit_binary_must_be_able_to_start(self):
        with tempfile.TemporaryDirectory() as folder:
            plain = Path(folder) / 'not-executable'
            plain.write_text('x')
            for value in ('/no/such/indy-kernel', str(plain), folder):
                with self.subTest(value=value), patch.dict(os.environ, {'INDYMAT_R_KERNEL': value}):
                    with self.assertRaises(RuntimeError) as caught:
                        kernel_executable(ROOT)
                    self.assertIn('INDYMAT_R_KERNEL', str(caught.exception))
            with patch.dict(os.environ, {'INDYMAT_R_KERNEL': self.executable}):
                self.assertEqual(kernel_executable(ROOT), self.executable)

    def test_compat_text_readers_keep_a_written_replacement_character(self):
        (self.work / 'written.txt').write_bytes('a\ufffdb,ü\n'.encode('utf-8'))
        (self.work / 'latin.txt').write_bytes('ü,1\n'.encode('latin-1'))
        state = self.run_code(f"""addpath({self.k.quote(ROOT / 'octave' / 'compat')});
            a = readlines('written.txt'); assert(isequal(double(char(a(1))), [97 65533 98 44 252]));
            b = readcell('written.txt'); assert(isequal(double(b{{1}}), [97 65533 98]) && isequal(double(b{{2}}), 252));
            c = readlines('latin.txt'); assert(isequal(double(char(c(1))), [252 44 49]));
            d = readcell('latin.txt'); assert(isequal(double(d{{1}}), 252));
            assert(isempty(fopen('all')));""")
        self.assertIsNone(state['error'])

    def test_missing_binary_is_reported(self):
        old = os.environ.pop('INDYMAT_R_KERNEL', None)
        try:
            with tempfile.TemporaryDirectory() as root, \
                    patch.object(kernel_rust, '__file__', str(Path(root) / 'backend' / 'kernel_rust.py')):
                with self.assertRaises(RuntimeError) as caught:
                    kernel_executable(root)
                self.assertIn('cargo build', str(caught.exception))
        finally:
            if old is not None:
                os.environ['INDYMAT_R_KERNEL'] = old


class StubKernelTest(RustKernelTest.__base__):
    """INDYMAT_R_STUB=1 keeps the stub engine selectable; it stays marked as a stub."""

    def test_stub_by_environment(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {'INDYMAT_R_STUB': '1'}):
            k = RustKernel(ROOT, Path(tmp) / 'jobs', Path(tmp), executable=binary())
            try:
                state = k.snapshot()
                self.assertTrue(state['engine']['stub'])
                self.assertIn('stub', state['version'])
                job = k.submit('x = 3', 'code')
                end = time.monotonic() + 10
                while time.monotonic() < end and not (k.snapshot()['job'] == job and k.snapshot()['status'] == 'idle'):
                    time.sleep(.02)
                self.assertIn('stub engine', k.snapshot()['output'])
            finally:
                k.close()


HUNG_ENGINE = """#!{python}
import json, sys
print(json.dumps({{'type': 'ready', 'protocol': 1, 'engine': {{'name': 'hung'}}, 'cwd': '.'}}), flush=True)
for line in sys.stdin:
    message = json.loads(line)
    if message['op'] != 'inspect':
        print(json.dumps({{'type': 'response', 'id': message['id'], 'ok': True, 'result': {{}}}}), flush=True)
"""


class WedgedEngineTest(unittest.TestCase):
    def test_a_request_that_gets_no_answer_ends_the_session_once(self):
        with tempfile.TemporaryDirectory() as folder, \
                patch.object(kernel_rust._Session.request, '__defaults__', (.3,)):
            base = Path(folder).resolve()
            (base / 'work').mkdir()
            engine = base / 'hung'
            engine.write_text(HUNG_ENGINE.format(python=sys.executable))
            engine.chmod(0o755)
            k = RustKernel(ROOT, base / 'jobs', base / 'work', executable=str(engine))
            try:
                proc = k.session.proc
                started = time.monotonic()
                k.submit('', 'inspect', 'x')
                end = time.monotonic() + 5
                while k.snapshot()['status'] == 'running' and time.monotonic() < end:
                    time.sleep(.01)
                state = k.snapshot()
                self.assertEqual(state['status'], 'dead')
                self.assertIn('did not answer', state['error'])
                self.assertLess(time.monotonic() - started, .55, 'no second wait on the same wedged child')
                self.assertIsNotNone(proc.poll())
            finally:
                k.close()


if __name__ == '__main__':
    unittest.main()


class RustWorkspaceMatFlowTest(unittest.TestCase):
    """The app's Workspace Save / Save Selection / Load (backend.workspace_actions) on the Rust engine."""

    @classmethod
    def setUpClass(cls):
        cls.executable = binary()

    def setUp(self):
        from types import SimpleNamespace
        import threading
        from app import PublishRustKernel, workspace_variable_names
        from backend.files import Workspace
        from backend.workspace_actions import WorkspaceActions
        self.validate = workspace_variable_names
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name).resolve()
        self.work = self.root / 'work'
        self.work.mkdir()
        self.app = SimpleNamespace(workspace=Workspace(self.work, self.root), file_lock=threading.Lock(),
                                   finish_publish=lambda k: None, discard_publish_stages=lambda *args: None)
        self.app.workspace_actions = WorkspaceActions(self.app)
        self.app.kernel = PublishRustKernel(ROOT, self.root / 'runtime' / 'jobs', self.work, publish_app=self.app,
                                            executable=self.executable)

    def tearDown(self):
        self.app.kernel.close()
        self.tmp.cleanup()

    def settle(self, job):
        end = time.monotonic() + 30
        while time.monotonic() < end:
            state = self.app.kernel.snapshot()
            if state['job'] == job and state['status'] in ('idle', 'dead'):
                return state
            time.sleep(.02)
        self.fail(self.app.kernel.snapshot())

    def code(self, source):
        return self.settle(self.app.kernel.submit(source, 'code'))

    def mat(self, request):
        return self.settle(self.app.workspace_actions.submit(request, self.validate))

    def test_save_all_selection_overwrite_inspect_and_load(self):
        from backend.workspace_actions import MatOverwriteConflict
        self.assertNotIn('mat_files', self.app.kernel.snapshot()['unsupported'])
        self.assertFalse(self.code("x = magic(4); t = 'İzmir'; c = {1, 'iki'}; z = 1 - 2i;")['error'])
        state = self.mat({'action': 'save', 'path': 'all.mat', 'overwrite': False, 'all': True})
        self.assertIsNone(state['error'])
        self.assertEqual(state['workspace_action']['path'], str(self.work / 'all.mat'))
        self.assertTrue((self.work / 'all.mat').read_bytes().startswith(b'MATLAB 5.0 MAT-file'))
        state = self.mat({'action': 'save', 'path': 'sel.mat', 'overwrite': False, 'all': False, 'names': ['x', 't']})
        self.assertEqual(state['workspace_action']['names'], ['x', 't'])
        # Overwriting needs the approval the conflict carries.
        with self.assertRaises(MatOverwriteConflict) as caught:
            self.app.workspace_actions.submit({'action': 'save', 'path': 'sel.mat', 'overwrite': False, 'all': True}, self.validate)
        details = caught.exception.details
        state = self.mat({'action': 'save', 'path': 'sel.mat', 'overwrite': True, 'confirm_overwrite': True, 'all': True,
                          'approval': details['approval'], 'expected_hash': details['expected_hash']})
        self.assertIsNone(state['error'])
        self.assertEqual(sorted(state['workspace_action']['names']), ['c', 't', 'x', 'z'])

        self.assertFalse(self.code('clear all; x = 0;')['error'])
        state = self.mat({'action': 'load-inspect', 'path': 'all.mat'})
        action = state['workspace_action']
        self.assertEqual([v['name'] for v in action['variables']], ['c', 't', 'x', 'z'])
        self.assertEqual(action['replacements'], ['x'])
        self.assertTrue(action['hash'] and action['inspection'])
        state = self.mat({'action': 'load', 'path': action['path'], 'hash': action['hash'], 'inspection': action['inspection'],
                          'replacements': action['replacements'], 'confirm': True})
        self.assertIsNone(state['error'])
        self.assertEqual(sorted(v['name'] for v in state['variables']), ['c', 't', 'x', 'z'])
        self.assertFalse(self.code("assert(isequal(x, magic(4)) && strcmp(t, 'İzmir') && isequal(c, {1, 'iki'}) && z == 1 - 2i)")['error'])

    @unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli (behaviour oracle) is not installed')
    def test_octave_loads_the_saved_workspace(self):
        self.assertFalse(self.code("x = magic(4); t = 'İzmir'; s.a = int8([1 -2]); c = {1, 'iki'};")['error'])
        self.assertIsNone(self.mat({'action': 'save', 'path': 'oct.mat', 'overwrite': False, 'all': True})['error'])
        check = ("S = load('oct.mat'); assert(isequal(S.x, magic(4)) && strcmp(S.t, 'İzmir') && "
                 "isequal(S.s.a, int8([1 -2])) && isequal(S.c, {1, 'iki'})); disp('octave-ok')")
        out = subprocess.run(['octave-cli', '-q', '--eval', check], cwd=self.work, capture_output=True, text=True, timeout=60)
        self.assertIn('octave-ok', out.stdout, out.stderr)
