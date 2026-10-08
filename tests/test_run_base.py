"""Native file/profile execution keeps user names and Octave run semantics."""
import tempfile
import time
import unittest
from pathlib import Path
from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]


class NativeRunBaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.work = Path(self.temp.name)/'work'; self.work.mkdir()
        self.entries = self.work/'entries'; self.entries.mkdir()
        self.script = self.entries/'native_entry.m'
        self.kernel = Kernel(ROOT, Path(self.temp.name)/'runtime', self.work, executable='octave-cli')
        self.wait()

    def tearDown(self):
        self.kernel.close()
        self.temp.cleanup()

    def wait(self):
        deadline = time.monotonic()+30
        while time.monotonic()<deadline:
            state = self.kernel.snapshot()
            if state['status'] in ('idle', 'dead'):
                self.assertEqual(state['status'], 'idle', state)
                return state
            time.sleep(.025)
        self.fail('Native execution did not finish')

    def code(self, code):
        self.kernel.submit(code)
        state = self.wait()
        self.assertFalse(state['error'], state['error'])
        return state

    def test_file_and_profile_preserve_shadows_identity_and_dependencies(self):
        (self.entries/'sibling.m').write_text('function y=sibling()\ny=73;\nendfunction')
        self.code('source=11; rethrow=12; lasterror=13; run=14; builtin=15;')
        self.script.write_text("entry_value=sibling(); entry_path=mfilename('fullpath'); entry_cwd=pwd();")
        for mode in ('file', 'profile'):
            with self.subTest(mode=mode):
                self.kernel.submit(mode=mode, argument=str(self.script))
                state = self.wait()
                self.assertFalse(state['error'], state['error'])
                self.assertEqual(state['cwd'], str(self.work))
                self.code("assert(entry_value==73); assert(source==11 && rethrow==12 && lasterror==13 && run==14 && builtin==15); assert(strcmp(entry_path,"+self.kernel.quote(self.script.with_suffix(''))+")); assert(strcmp(entry_cwd,"+self.kernel.quote(self.entries)+"));")

    def test_errors_keep_native_trace_and_deliberate_cd_with_shadows(self):
        chosen = self.work/'chosen'; chosen.mkdir()
        self.script.write_text("source=111; rethrow=222; lasterror=333; cd("+self.kernel.quote(chosen)+"); missing_native_guard;")
        for mode in ('file', 'profile'):
            with self.subTest(mode=mode):
                self.kernel.submit(mode=mode, argument=str(self.script))
                state = self.wait()
                self.assertIn('missing_native_guard', state['error'])
                self.assertIn('native_entry:1', state['error'])
                self.assertIn('run:78', state['error'])
                self.assertNotIn('subscripts', state['error'])
                self.assertEqual(state['cwd'], str(chosen))
                self.code('assert(source==111 && rethrow==222 && lasterror==333);')

    def test_caller_scope_matches_direct_run(self):
        self.script.write_text("caller_has_source=evalin('caller',\"exist('source','var')\"); base_has_source=exist('source','var');")
        self.code('(@run)('+self.kernel.quote(self.script)+');')
        self.code('assert(caller_has_source==0 && base_has_source==0);')
        self.kernel.submit(mode='file', argument=str(self.script))
        self.assertFalse(self.wait()['error'])
        self.code('assert(caller_has_source==0 && base_has_source==0);')


if __name__ == '__main__':
    unittest.main()
