"""Native dynamic keyboard pauses, without HTTP, graphics or source adaptation."""
import tempfile
import time
import unittest
from pathlib import Path

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]
FORMS = {'direct': 'keyboard', 'eval': "eval('keyboard')",
         'base': "evalin('base','keyboard')", 'caller': 'dynamic_caller()',
         'handle': 'feval(@keyboard)'}


class DynamicPauseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='debug-test-')
        self.work = Path(self.temp.name)
        (self.work/'dynamic_caller.m').write_text(
            "function dynamic_caller()\n evalin('caller','keyboard');\nendfunction\n")
        self.kernel = Kernel(ROOT, self.work/'runtime', self.work, executable='octave-cli')
        self.wait()
        self.kernel.submit('kept=37; kept_handle=@sin;')
        self.assertFalse(self.wait()['error'])

    def tearDown(self):
        self.kernel.close()
        self.temp.cleanup()

    def wait(self, paused=False):
        deadline = time.monotonic()+10
        while time.monotonic()<deadline:
            state = self.kernel.snapshot()
            if paused:
                if state['status']=='paused' and (state.get('debug') or {}).get('ready'):
                    return state
            elif state['status'] in ('idle', 'dead'):
                return state
            time.sleep(.01)
        self.fail(str(self.kernel.snapshot()))

    def exercise(self, mode, form):
        epoch = self.kernel.generation
        pid = self.kernel.proc.pid
        code = 'before_pause=19; '+FORMS[form]+'; after_pause=23;'
        path = self.work/'dynamic_entry.m'
        path.write_text(code)
        def start():
            job = self.kernel.submit(code if mode=='code' else '', mode=mode,
                                     argument=str(path) if mode=='file' else '')
            state = self.wait(paused=True)
            self.assertEqual(state['job'], job)
            self.assertNotIn('source_adapter', state)
            self.assertIn('before_pause', {v['name'] for v in state['variables']})
            with self.assertRaises(ValueError):self.kernel.input('arbitrary code')
            return state
        paused = start()
        if mode=='code' and form in ('direct', 'eval', 'base', 'handle'):
            self.assertEqual(paused['debug']['file'], '')
            self.assertEqual(paused['debug']['line'], 0)
        self.kernel.debug('continue')
        self.assertFalse(self.wait()['error'])
        self.kernel.submit('assert(after_pause==23); clear after_pause;')
        self.assertFalse(self.wait()['error'])
        start()
        self.kernel.interrupt(); self.kernel.interrupt()
        self.assertEqual(self.wait()['status'], 'idle')
        self.assertEqual(self.kernel.generation, epoch)
        self.assertEqual(self.kernel.proc.pid, pid)
        self.kernel.submit('assert(kept==37); assert(kept_handle(0)==0); assert(before_pause==19); assert(~exist("after_pause","var"));')
        self.assertFalse(self.wait()['error'])
        # Step can leave a locationless pause too (possibly pausing at the next
        # native line); Continue must then complete without resetting anything.
        start()
        self.kernel.debug('step')
        deadline=time.monotonic()+10
        while time.monotonic()<deadline:
            state=self.kernel.snapshot()
            if state['status']=='idle':break
            if state['status']=='paused' and state['debug']['ready']:
                self.kernel.debug('continue');state=self.wait();break
            time.sleep(.01)
        self.assertEqual(state['status'],'idle',state)
        self.assertFalse(state['error'],state)
        self.assertEqual(self.kernel.generation,epoch)


def make_test(mode, form):
    def test(self):self.exercise(mode, form)
    return test

for mode in ('code', 'file'):
    for form in FORMS:
        setattr(DynamicPauseTests, f'test_{mode}_{form}', make_test(mode, form))

if __name__=='__main__':unittest.main()
