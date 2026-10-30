"""The engine setting in app.py: GNU Octave unless the user chose otherwise; one engine at a time."""
import json
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import app as application
from app import App, ENGINES, PublishKernel, PublishRustKernel
from backend.assistants import environment_note
from backend.kernel import Kernel
from backend.kernel_rust import RustKernel


class FakeKernel:
    def __init__(self, name, generation, log):
        self.name, self.generation, self.log = name, generation, log
        self.lock = threading.RLock()
    def close(self):
        self.log.append('close ' + self.name)
        self.generation += 1


def make_app(folder, engine='octave', problems=None):
    target = App.__new__(App)
    target.runtime = Path(folder)
    target.engine_file = target.runtime / 'engine.json'
    target.engine_lock = threading.Lock()
    target.engine_epoch = 0
    target.engine_notice = ''
    target.engine = engine
    target.log = []
    target.problems = problems or {}
    target.fail_next = None
    target.octave_services = SimpleNamespace(close=lambda: target.log.append('close services'))
    target.kernel = FakeKernel(engine, 3, target.log)
    target._engine_problem = lambda name: target.problems.get(name, '')
    def open_engine(name):
        target.log.append('open ' + name)
        if target.fail_next == name:
            target.fail_next = None
            raise RuntimeError('no start')
        target.kernel = FakeKernel(name, target.engine_epoch + 1, target.log)
    target._open_engine = open_engine
    return target


class EngineSelection(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.folder = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def saved(self):
        return json.loads((self.folder / 'engine.json').read_text())['engine']

    def test_octave_is_the_default(self):
        self.assertEqual(ENGINES[0], 'octave')
        target = make_app(self.folder)
        self.assertEqual(App._saved_engine(target), 'octave')
        self.assertFalse((self.folder / 'engine.json').exists(), 'nothing is written until the user chooses')
        for broken in ('not json', '[]', '{"engine":"matlab"}', '{"engine":1}'):
            (self.folder / 'engine.json').write_text(broken)
            self.assertEqual(App._saved_engine(target), 'octave', broken)

    def test_choice_is_saved_and_read_back(self):
        target = make_app(self.folder)
        App._save_engine(target, 'rust')
        self.assertEqual(App._saved_engine(target), 'rust')
        self.assertEqual(list(self.folder.iterdir()), [self.folder / 'engine.json'])

    def test_switch_closes_before_it_opens_and_raises_the_epoch(self):
        target = make_app(self.folder)
        before = target.kernel.generation
        info = App.switch_engine(target, 'rust')
        self.assertEqual(target.log, ['close octave', 'close services', 'open rust'])
        self.assertEqual(info['engine'], 'rust')
        self.assertEqual(target.engine, 'rust')
        self.assertEqual(self.saved(), 'rust')
        self.assertGreater(target.kernel.generation, before)

    def test_switch_to_the_active_engine_restarts_nothing(self):
        target = make_app(self.folder)
        App.switch_engine(target, 'octave')
        self.assertEqual(target.log, [])

    def test_missing_binary_is_refused_and_the_session_stays(self):
        message = 'The Rust engine is not built yet. Build it with: cd motor && cargo build --release'
        target = make_app(self.folder, problems={'rust': message})
        with self.assertRaises(ValueError) as caught:
            App.switch_engine(target, 'rust')
        self.assertEqual(str(caught.exception), message)
        self.assertEqual(target.log, [])
        self.assertEqual(target.engine, 'octave')
        self.assertFalse((self.folder / 'engine.json').exists())

    def test_failed_start_restores_the_previous_engine(self):
        target = make_app(self.folder)
        target.fail_next = 'rust'
        with self.assertRaises(ValueError) as caught:
            App.switch_engine(target, 'rust')
        self.assertIn('no start', str(caught.exception))
        self.assertEqual(target.log, ['close octave', 'close services', 'open rust', 'open octave'])
        self.assertEqual(target.engine, 'octave')
        self.assertFalse((self.folder / 'engine.json').exists())

    def test_unknown_engine(self):
        with self.assertRaises(ValueError):
            App.switch_engine(make_app(self.folder), 'matlab')

    def test_engine_problem_reports_the_build_command(self):
        target = App.__new__(App)
        with patch.object(application, 'kernel_executable', side_effect=RuntimeError('cd motor && cargo build --release')):
            self.assertIn('cargo build --release', App._engine_problem(target, 'rust'))
        with patch.object(application, 'kernel_executable', return_value='/bin/true'):
            self.assertEqual(App._engine_problem(target, 'rust'), '')

    def test_epoch_floor_is_neutral_until_a_switch(self):
        for cls, base in ((PublishKernel, Kernel), (PublishRustKernel, RustKernel)):
            started = []
            def fake_start(kernel):
                kernel.generation += 1
                started.append(kernel.generation)
            with patch.object(base, 'start', fake_start):
                probe = cls.__new__(cls)
                probe.generation = 0
                probe.publish_app = SimpleNamespace()
                probe.start()
                probe.publish_app = SimpleNamespace(engine_epoch=0)
                probe.start()
                probe.publish_app = SimpleNamespace(engine_epoch=9)
                probe.start()
            self.assertEqual(started, [1, 2, 10], cls.__name__)

    def test_assistant_note_names_the_active_engine(self):
        octave = environment_note('ask', '/h/w', '/h', '11.3.0', ['signal'])
        rust = environment_note('ask', '/h/w', '/h', 'IndyMAT Rust engine (experimental)', [], rust_engine=True)
        self.assertIn('The engine is GNU Octave 11.3.0, not MATLAB. Loaded packages: signal.', octave)
        self.assertNotIn('The engine is GNU Octave', rust)
        self.assertNotIn('live Octave session', rust)
        self.assertIn('experimental Rust engine', rust)
        for access in ('inspect', 'run'):
            with self.subTest(access=access):
                live = environment_note('ask', '/h/w', '/h', '', [], session_access=access, rust_engine=True)
                self.assertNotIn('Octave session', live)
        self.assertIn("user's LIVE session", environment_note('ask', '/h/w', '/h', '', [], session_access='run', rust_engine=True))
        self.assertIn("user's LIVE Octave session", environment_note('ask', '/h/w', '/h', '', [], session_access='run'))

    def test_a_kernel_being_replaced_refuses_reset_instead_of_starting_an_orphan(self):
        for cls in (PublishKernel, PublishRustKernel):
            probe = cls.__new__(cls)
            probe.lock = threading.RLock()
            probe.closed = True
            with self.assertRaises(ValueError):
                cls.reset(probe)

    def test_the_switching_flag_covers_the_switch_and_a_failed_one(self):
        target = make_app(self.folder)
        seen = []
        open_engine = target._open_engine
        target._open_engine = lambda name: (seen.append(target.engine_switching), open_engine(name))
        App.switch_engine(target, 'rust')
        self.assertEqual(seen, [True])
        self.assertFalse(target.engine_switching)
        target.fail_next = 'octave'
        with self.assertRaises(ValueError):
            App.switch_engine(target, 'octave')
        self.assertFalse(target.engine_switching)

    def test_a_failed_service_start_closes_the_octave_it_started_and_publishes_nothing(self):
        target = App.__new__(App)
        target.runtime = self.folder
        target.workspace = SimpleNamespace(root=self.folder)
        target.kernel, target.linter, target.octave_services = 'old kernel', 'old linter', 'old services'
        started = FakeKernel('octave', 1, [])
        started.executable = 'octave'
        with patch.object(application, 'PublishKernel', return_value=started), \
                patch.object(application, 'OctaveLinter', return_value='new linter'), \
                patch.object(application, 'OctaveServices', side_effect=OSError('no services')):
            with self.assertRaises(OSError):
                App._open_engine(target, 'octave')
        self.assertEqual(started.log, ['close octave'])
        self.assertEqual((target.kernel, target.linter, target.octave_services), ('old kernel', 'old linter', 'old services'))


if __name__ == '__main__':
    unittest.main()
