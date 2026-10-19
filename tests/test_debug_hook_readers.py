"""Real public-hook regressions: debugger and root readers skip callbacks.

The reader contract skips public callbacks at debugger and root prompts.
They use only public Octave APIs in their callbacks.
"""
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from tests import test_debug_stop_recovery as support


class HookReaderLifetime(unittest.TestCase):
    def setUp(self):
        self.fixture = support.StopRecovery('test_root_recovery_after_nested_abort')
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.addCleanup(self.fixture.tearDown)
        self.k = self.fixture.k

    def delayed_proof(self, kind):
        original = Path.read_text
        self.delayed = False

        def read(path, *args, **kwargs):
            value = original(path, *args, **kwargs)
            ticket = self.k._debug_ticket
            expected = ('D '+ticket+'\n', 'Q '+ticket+' 0 0\n') if kind == 'Q' else ('P '+ticket+'\n',)
            if not self.delayed and path == self.k._debug_prompt_file and all(record in value for record in expected):
                self.delayed = True
                # The next native hook tick enters the public reader while
                # the collector retains the earlier proof's text.
                time.sleep(.6)
            return value

        return patch.object(Path, 'read_text', read)

    def stop(self):
        self.k.interrupt()
        self.k.interrupt()
        return self.fixture.wait(lambda state: state['status'] in ('idle', 'dead'))

    def test_delayed_root_proof_does_not_reach_public_kbhit(self):
        callback = (
            "if !isdebugmode(); ticks+=1; if ticks==2; "
            "remove_input_event_hook(hook_id); "
            "got=kbhit(); fprintf('KBYTE=%d\\n',double(got)); fflush(1); "
            "endif; endif;"
        )
        self.fixture.pause('ticks=0; hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+'); keyboard;')
        with self.delayed_proof('Q'):
            state = self.stop()
        self.assertTrue(self.delayed)
        self.assertNotIn('KBYTE=', state['output'], state)
        self.assertEqual(state['status'], 'idle', state)

    def test_delayed_debugger_proof_does_not_reach_active_public_kbhit(self):
        callback = (
            "if isdebugmode(); ticks+=1; if ticks==2; "
            "remove_input_event_hook(hook_id); fprintf('HOOK_ENTER\\n'); fflush(1); "
            "got=kbhit(); fprintf('KBYTE=%d\\n',double(got)); fflush(1); "
            "endif; endif;"
        )
        with self.delayed_proof('P'):
            self.k.submit('ticks=0; hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+'); keyboard;')
            time.sleep(1)
            state = self.stop()
        self.assertTrue(self.delayed)
        self.assertNotIn('HOOK_ENTER', state['output'], state)
        self.assertNotIn('KBYTE=', state['output'], state)
        self.assertEqual(state['status'], 'idle', state)

    def test_ready_pause_skips_public_kbhit_before_dbquit(self):
        callback = (
            "if isdebugmode() && hook_enabled; ticks+=1; if ticks==3; "
            "remove_input_event_hook(hook_id); fprintf('HOOK_ENTER\\n'); fflush(1); "
            "got=kbhit(); fprintf('KBYTE=%d\\n',double(got)); fflush(1); "
            "endif; endif;"
        )
        self.fixture.pause('hook_enabled=false; ticks=0; hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+'); keyboard;')
        # Activate via ordinary authorized debugger evaluation. Previously
        # the third native tick blocked in kbhit while an older P authorized
        # the ready debugger prompt. The input subset skips every debug tick.
        self.k.debug('eval', 'hook_enabled=true;')
        self.fixture.wait(lambda state: state['status'] == 'paused' and state['debug']['ready'])
        time.sleep(.5)
        controls = []
        original = self.k._write_control

        def observe(proc, control, prefix=''):
            controls.append((control, self.k._reader_authorized, self.k.snapshot()['output']))
            return original(proc, control, prefix)

        self.k._write_control = observe
        state = self.stop()
        self.assertTrue(any(control == self.k.call('dbquit') and authorized
                            for control, authorized, output in controls), controls)
        self.assertNotIn('HOOK_ENTER', state['output'], state)
        self.assertNotIn('KBYTE=', state['output'], state)
        self.assertEqual(state['status'], 'idle', state)


if __name__ == '__main__':
    unittest.main()
