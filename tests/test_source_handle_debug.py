"""Saved-entry handle calls must retain original physical debugger identity."""
import hashlib
import tempfile
import time
import unittest
from pathlib import Path

from backend.files import Workspace
from backend.kernel import Kernel
from backend.source_adapter import AdapterProfile, PackageSupport
from backend.source_files import adapt_file_source, read_file_source

ROOT = Path(__file__).resolve().parents[1]
PROFILE = AdapterProfile(True, package=PackageSupport(True, '/verified/string.m', 'fixture', ''))


class HandleDebugTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='adapter-handle-', dir=ROOT)
        self.base = Path(self.temp.name)
        self.work = self.base / 'work'
        self.work.mkdir()
        self.path = self.work / 'entry.m'
        self.workspace = Workspace(self.work, allowed_root=self.base)
        self.kernel = None

    def tearDown(self):
        if self.kernel:
            self.kernel.close()
            for entry in self.kernel.runtime.glob('*/entry'):
                entry.chmod(0o700)
        self.temp.cleanup()

    def source(self, text):
        data = text.encode('utf-8')
        self.path.write_bytes(data)
        return read_file_source(self.workspace, str(self.path), hashlib.sha256(data).hexdigest())

    def wait(self, predicate):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            state = self.kernel.snapshot()
            if predicate(state):
                return state
            time.sleep(.025)
        self.fail(str(self.kernel.snapshot()))

    def assert_saved_pause(self, call):
        self.kernel = Kernel(ROOT, self.base / 'runtime', self.work, executable='octave-cli')
        self.wait(lambda s: s['status'] in ('idle', 'dead'))
        source = self.source('r="x"; ' + call + ' after_pause=1;')
        self.kernel.submit(mode='file', argument=source.path, source_file=source)
        paused = self.wait(lambda s: s['status'] == 'paused' and (s.get('debug') or {}).get('ready'))
        self.assertEqual(paused['debug']['file'], source.path)
        self.assertTrue(any(f.get('file') == source.path for f in paused['debug']['stack']))
        self.assertEqual(paused['source_adapter']['status'], 'fallback')
        self.assertEqual(paused['source_adapter']['diagnostics'][0]['code'], 'file-debug')
        self.assertEqual(paused['source_adapter']['replacements'], [])
        self.assertEqual(self.path.read_text(), source.document)
        self.assertEqual(next(v['class'] for v in paused['variables'] if v['name'] == 'r'), 'char')
        epoch = self.kernel.generation
        self.kernel.interrupt()
        self.wait(lambda s: s['status'] in ('idle', 'dead'))
        self.assertEqual(self.kernel.generation, epoch)

    def test_ordinary_eval_control(self):
        self.assert_saved_pause("eval('keyboard');")

    def test_handle_eval_original_pause(self):
        self.assert_saved_pause("(@eval)('keyboard');")

    def test_handle_evalin_original_pause(self):
        self.assert_saved_pause("(@evalin)('base','keyboard');")

    def test_nested_handle_eval_original_pause(self):
        self.assert_saved_pause("( ( @eval ) ) ( 'keyboard' );")

    def test_handle_literal_detection_whole_unit(self):
        for name, args in [('eval', "'keyboard'"), ('evalin', "'base','keyboard'"),
                           ('feval', "'keyboard'"), ('builtin', "'keyboard'"),
                           ('str2func', "'keyboard'")]:
            for handle in ['(@' + name + ')', '( ( @' + name + ' ) )',
                           '( ...\n @' + name + ' ...\n )']:
                with self.subTest(handle=handle):
                    source = self.source('r="x"; ' + handle + ' (' + args + ');')
                    result = adapt_file_source(source, PROFILE)
                    self.assertEqual(result.status, 'fallback')
                    self.assertEqual(result.diagnostics[0].code, 'file-debug')
                    self.assertEqual(result.generated_text, source.document)
                    self.assertEqual(result.replacements, ())

    def test_non_debug_literals_still_adapt(self):
        for code in ['r="keyboard"; % (@eval)(\'keyboard\');\n',
                     'r="x"; (@disp)(\'keyboard\');',
                     'r="x"; (@eval)(\'a=1;\'); s="keyboard";',
                     'r="x"; (@eval)(\'keyboard_suffix=1;\');',
                     'r="x"; h=(@eval); s="keyboard";']:
            with self.subTest(code=code):
                self.assertEqual(adapt_file_source(self.source(code), PROFILE).status, 'adapted')


if __name__ == '__main__':
    unittest.main()
