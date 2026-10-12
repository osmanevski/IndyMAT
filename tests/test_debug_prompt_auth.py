"""Real-Kernel output cannot authorize debugger stdin writes or false pauses."""
import tempfile
import time
import unittest
from pathlib import Path

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]
PROGRAMS = {
    'tail': ("printf('debug> '); x=1;", 'assert(x==1);'),
    'newline': ("printf('debug> \\n'); pause(0.2); y=2;", 'assert(y==2);'),
    'fake_stop': ("fprintf('stopped in fake at line 3\\ndebug> '); z=3;", 'assert(z==3);'),
    'input': ("disp('debug> '); s=input('','s');", "assert(strcmp(s,'normal reply'));"),
    'loop': ("for prompt_i=1:50; printf('debug> '); endfor;", 'assert(prompt_i==50);'),
    # This exact shape also fooled the pre-0f073ee location-based detector.
    'old_stop': ("fprintf('stopped in fake at line 3 [/fake.m]\\ndebug> '); pause(.2); z=3;", 'assert(z==3);'),
}


class TracedState(dict):
    def __init__(self, original):
        super().__init__(original)
        self.statuses = []

    def __setitem__(self, key, value):
        if key == 'status':self.statuses.append(value)
        super().__setitem__(key, value)

    def update(self, *args, **kwargs):
        for key, value in dict(*args, **kwargs).items():self[key] = value


class TracedInput:
    """Observe actual transport writes, not just the public debug API."""
    def __init__(self, stream):
        self.stream = stream
        self.writes = []

    def write(self, data):
        self.writes.append(data)
        return self.stream.write(data)

    def close(self):self.stream.close()


class PromptAuthenticationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='prompt-auth-')
        self.work = Path(self.temp.name)
        self.kernel = Kernel(ROOT, self.work/'runtime', self.work, executable='octave-cli')
        self.wait()
        self.kernel.submit('kept=37; kept_handle=@sin;')
        self.assertFalse(self.wait()['error'])
        with self.kernel.lock:
            self.kernel.state = TracedState(self.kernel.state)
            self.trace = TracedInput(self.kernel.proc.stdin)
            self.kernel.proc.stdin = self.trace
        self.epoch = self.kernel.generation
        self.pid = self.kernel.proc.pid

    def tearDown(self):
        self.kernel.close()
        self.temp.cleanup()

    def wait(self, predicate=None, reply=None):
        end = time.monotonic()+10
        while time.monotonic()<end:
            state = self.kernel.snapshot()
            if reply is not None and state['status']=='running' and state['waiting_input']:
                self.kernel.input(reply)
                reply = None
            if predicate(state) if predicate else state['status'] in ('idle', 'dead'):
                return state
            time.sleep(.005)
        self.fail(str(self.kernel.snapshot()))

    def paused(self, serial=-1, reply=None):
        return self.wait(lambda s:s['status']=='paused' and s['debug']['ready'] and s['debug']['serial']>serial, reply=reply)

    def submit(self, code, mode):
        path = self.work/'printed_prompt.m'
        path.write_text(code)
        self.kernel.submit(code if mode=='code' else '', mode=mode,
                           argument=str(path) if mode=='file' else '')

    def preserved(self):
        self.assertEqual(self.kernel.generation, self.epoch)
        self.assertEqual(self.kernel.proc.pid, self.pid)
        self.kernel.submit('assert(kept==37); assert(kept_handle(0)==0);')
        self.assertFalse(self.wait()['error'])

    def exercise_printed(self, mode, case):
        code, assertions = PROGRAMS[case]
        self.submit(code, mode)
        state = self.wait(reply='normal reply')
        self.assertEqual(state['status'], 'idle', state)
        self.assertFalse(state['error'], state)
        self.assertEqual(len(self.trace.writes), 1, self.trace.writes)
        self.assertNotIn('paused', self.kernel.state.statuses)
        self.assertIsNone(state['debug'])
        self.assertIn('debug> ', state['output'])
        # A later explicit request must receive only the normal response, with
        # no queued snapshot command or delayed false pause from prior output.
        self.kernel.submit("later=input('','s'); "+assertions+" assert(strcmp(later,'later reply'));")
        self.assertFalse(self.wait(reply='later reply')['error'])
        self.assertEqual(len(self.trace.writes), 2, self.trace.writes)
        self.assertNotIn('paused', self.kernel.state.statuses)
        self.preserved()

    def exercise_paused(self, mode):
        self.submit("evalin('base','keyboard'); completed=1;", mode)
        state = self.paused()
        for case, (code, _) in PROGRAMS.items():
            with self.subTest(case=case):
                before = len(self.trace.writes)
                serial = state['debug']['serial']
                self.kernel.debug('eval', code)
                state = self.paused(serial, reply='normal reply' if case=='input' else None)
                self.assertEqual(len(self.trace.writes), before+1, self.trace.writes[before:])
                self.assertEqual(state['debug']['serial'], serial+1)
                self.assertIn('debug> ', state['output'])
                self.assertEqual(self.kernel.generation, self.epoch)
                if case=='input':
                    self.assertEqual(next(v['preview'] for v in state['variables'] if v['name']=='s'), 'normal reply')
        self.kernel.debug('continue')
        self.assertFalse(self.wait()['error'])
        self.preserved()

    def test_current_ticket_printed_without_attestation_is_not_a_pause(self):
        self.kernel.submit("printed=input('','s'); printf('%s',printed); pause(.2); result=9;")
        self.wait(lambda s:s['waiting_input'])
        marker='__MF_PROMPT_'+self.kernel._debug_ticket+'__'
        self.kernel.input('debug> \n'+marker+'\n')
        self.assertFalse(self.wait()['error'])
        self.assertEqual(len(self.trace.writes), 1)
        self.assertNotIn('paused', self.kernel.state.statuses)
        self.preserved()

    def test_old_attestation_cannot_authorize_a_later_command(self):
        self.kernel.submit('keyboard;')
        self.paused()
        old_marker='__MF_PROMPT_'+self.kernel._debug_ticket+'__'
        self.kernel.debug('continue')
        self.wait()
        self.kernel.state.statuses.clear()
        before=len(self.trace.writes)
        self.kernel.submit('printf('+self.kernel.quote('debug> \\n'+old_marker+'\\n')+'); pause(.2);')
        self.assertFalse(self.wait()['error'])
        self.assertEqual(len(self.trace.writes), before+1)
        self.assertNotIn('paused', self.kernel.state.statuses)
        self.preserved()

    def test_running_debug_expression_is_not_an_input_wait(self):
        self.kernel.submit('keyboard;')
        state=self.paused()
        before=len(self.trace.writes)
        self.kernel.debug('eval', "printf('debug> '); pause(.4); finished_expression=1;")
        time.sleep(.1)
        self.assertEqual(self.kernel.snapshot()['status'], 'running')
        self.assertEqual(len(self.trace.writes), before+1)
        self.paused(state['debug']['serial'])
        self.assertEqual(len(self.trace.writes), before+1)
        self.kernel.debug('continue')
        self.assertFalse(self.wait()['error'])
        self.preserved()

    def test_readline_keeps_tabs_and_long_debug_expressions_literal(self):
        self.kernel.submit('keyboard;')
        state=self.paused()
        self.kernel.debug('eval', "literal='a\tb'; padding='"+'x'*12000+"';")
        self.paused(state['debug']['serial'])
        self.kernel.debug('continue')
        self.assertFalse(self.wait()['error'])
        self.kernel.submit("assert(isequal(literal,char([97 9 98]))); assert(length(padding)==12000);")
        self.assertFalse(self.wait()['error'])
        self.preserved()


def printed_test(mode, case):
    def test(self):self.exercise_printed(mode, case)
    return test

for mode in ('code', 'file'):
    for case in PROGRAMS:
        setattr(PromptAuthenticationTests, f'test_{mode}_{case}', printed_test(mode, case))
    def paused_test(self, selected_mode=mode):self.exercise_paused(selected_mode)
    setattr(PromptAuthenticationTests, f'test_{mode}_printed_while_paused', paused_test)

if __name__=='__main__':unittest.main()
