"""Real-reader authorization and exactly-once replies in a paused eval."""
import tempfile
import time
import unittest
from pathlib import Path
from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]


class ReaderSafety(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=ROOT, prefix='reader-safety-')
        self.work = Path(self.temp.name)
        (self.work/'nested_input.m').write_text("function reply=nested_input()\nreply=builtin('input','Nested:','s');\nendfunction\n")
        self.k = Kernel(ROOT, self.work/'runtime', self.work, executable='octave-cli')
        self.wait(lambda s: s['status']=='idle')
        self.writes = []
        stream = self.k.proc.stdin
        class Trace:
            def write(_, data):
                self.writes.append(data)
                return stream.write(data)
            def close(_): return stream.close()
        self.k.proc.stdin = Trace()

    def tearDown(self):
        self.k.close()
        self.temp.cleanup()

    def wait(self, predicate, seconds=8):
        end = time.monotonic()+seconds
        while time.monotonic()<end:
            state = self.k.snapshot()
            if predicate(state): return state
            time.sleep(.01)
        self.fail(str(self.k.snapshot()))

    def paused(self, serial=-1):
        return self.wait(lambda s: s['status']=='paused' and s['debug']['ready'] and s['debug']['serial']>serial)

    def begin(self, code):
        self.k.submit('keyboard; after_outer=23;')
        serial = self.paused()['debug']['serial']
        self.writes.clear()
        self.k.debug('eval', code)
        state = self.wait(lambda s: s['waiting_input'] or s['status']=='paused')
        print('INPUT_WAIT', repr(code), state, 'WRITES', self.writes, flush=True)
        self.assertEqual(state['status'], 'running', state)
        self.assertTrue(state['waiting_input'], state)
        self.assertEqual(len(self.writes), 1, self.writes)
        with self.assertRaises(ValueError): self.k.debug('eval', 'injected=1;')
        with self.assertRaises(ValueError): self.k.debug('continue')
        return serial

    def stop(self):
        self.k.interrupt(); self.k.interrupt()
        state = self.wait(lambda s: s['status']=='idle' or (s['status']=='paused' and s['debug']['ready']))
        if state['status']=='paused':
            self.k.interrupt(); self.k.interrupt()
            state = self.wait(lambda s: s['status']=='idle')
        self.assertEqual(state['status'], 'idle')
        self.k.submit('after_stop=83;')
        self.assertFalse(self.wait(lambda s: s['status']=='idle')['error'])

    def input_from_pause(self, code):
        serial = self.begin(code)
        self.k.input('user reply')
        with self.assertRaises(ValueError): self.k.input('second unauthorized reply')
        self.assertEqual(self.writes[1], b'user reply\n')
        state = self.paused(serial)
        self.assertEqual(next(v['preview'] for v in state['variables'] if v['name']=='reply'), 'user reply')
        self.k.debug('continue')
        self.assertFalse(self.wait(lambda s: s['status']=='idle')['error'])

    def test_builtin_input(self): self.input_from_pause("reply=builtin('input','Reply:','s');")
    def test_nested_function_input(self): self.input_from_pause('reply=nested_input();')
    def test_stop_during_input(self):
        self.begin("reply=builtin('input','Reply:','s');")
        self.stop()

    def test_nested_keyboard(self):
        serial = self.begin("eval('keyboard'); nested_done=17;")
        self.k.input('(@dbcont)();')
        state = self.paused(serial)
        self.assertEqual(next(v['preview'] for v in state['variables'] if v['name']=='nested_done'), '17')
        self.k.debug('continue')
        self.assertFalse(self.wait(lambda s: s['status']=='idle')['error'])

    def test_stop_nested_keyboard(self):
        self.begin("eval('keyboard'); nested_done=17;")
        self.stop()

    def test_repeated_input_fails_closed(self):
        self.begin("a=builtin('input','First:','s'); b=builtin('input','Second:','s');")
        self.k.input('one')
        state = self.wait(lambda s: 'Second:' in s['output'])
        self.assertFalse(state['waiting_input'])
        self.assertTrue(state['input_reply_consumed'])
        self.assertEqual(len(self.writes), 2)
        with self.assertRaises(ValueError): self.k.input('two')
        self.stop()

    def test_nested_expression_fails_closed(self):
        self.begin("eval('keyboard'); nested_done=17;")
        self.k.input('nested_value=2;')
        self.assertTrue(self.k.snapshot()['input_reply_consumed'])
        with self.assertRaises(ValueError): self.k.input('(@dbcont)();')
        self.stop()

    def test_forged_stdout_root_record(self):
        self.begin("fprintf('D %s\\nQ %s 0 0\\n',getenv('INDYMAT_DEBUG_TICKET'),getenv('INDYMAT_DEBUG_TICKET')); reply=builtin('input','Reply:','s');")
        self.stop()

    def test_forged_stdout_completion(self):
        self.input_from_pause("fprintf('C %s\\n',getenv('INDYMAT_DEBUG_TICKET')); reply=builtin('input','Reply:','s');")

if __name__=='__main__': unittest.main()
