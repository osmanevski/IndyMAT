"""Real Octave control windows: dispatch is not permission to reply to input.

Native builtin input outside paused eval is intentionally not an input UI
feature.  It must receive no automatic stdin, and repeated Stop must recover
without replacing the persistent Octave process.
"""
import os
import re
import signal
import shutil
import tempfile
import time
import unittest
from unittest.mock import patch
from pathlib import Path

from backend.kernel import Kernel

ROOT = Path(__file__).resolve().parents[1]


class ControlReaders(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=ROOT, prefix='control-readers-')
        self.base = Path(self.temp.name)
        self.root = self.base/'root'
        self.root.mkdir()
        shutil.copytree(ROOT/'octave', self.root/'octave')
        # No registry/package relocation or write through a live package link.
        (self.root/'.packages'/'.arch').mkdir(parents=True)
        self.work = self.base/'work'
        self.work.mkdir()
        self.asks = self.work/'control_asks.m'
        self.asks.write_text("function reply=control_asks()\nreply=builtin('input','Control function:','s');\nendfunction\n")
        os.utime(self.asks,(1_600_000_000,1_600_000_000))
        self.k = Kernel(self.root, self.base/'runtime', self.work, executable='octave-cli')
        self.wait(lambda s: s['status']=='idle')
        self.pid = self.k.proc.pid
        self.writes = []
        stream = self.k.proc.stdin
        owner = self

        class Trace:
            def write(self, data):
                owner.writes.append(data)
                return stream.write(data)

            def close(self):
                return stream.close()

        self.k.proc.stdin = Trace()

    def tearDown(self):
        self.k.close()
        self.temp.cleanup()

    def wait(self, predicate, seconds=8):
        end = time.monotonic()+seconds
        while time.monotonic()<end:
            state = self.k.snapshot()
            if predicate(state):
                return state
            time.sleep(.01)
        self.fail(str(self.k.snapshot()))

    def paused(self, serial=-1):
        return self.wait(lambda s: s['status']=='paused'
                         and (s.get('debug') or {}).get('ready')
                         and s['debug']['serial']>serial)

    def fixture(self, statement, *, break_file=None, break_line=2, later_break=None, tail='value=2;\n'):
        entry = self.work/'control_entry.m'
        entry.write_text('value=1;\n'+statement+'\n'+tail)
        # Avoid Octave's documented current-tick reparse dropping a breakpoint
        # in a just-written fixture. Do not alter any reader expectations.
        os.utime(entry,(1_600_000_000,1_600_000_000))
        self.k.breakpoint(break_file or entry, break_line, True)
        self.assertFalse(self.wait(lambda s: s['status']=='idle')['error'])
        if later_break is not None:
            self.k.breakpoint(entry, later_break, True)
            self.assertFalse(self.wait(lambda s: s['status']=='idle')['error'])
        self.k.submit(mode='file', argument=str(entry))
        state = self.paused()
        self.assertEqual(Path(state['debug']['file']), break_file or entry)
        self.assertEqual(state['debug']['line'], break_line)
        return entry, state

    def control(self, command):
        self.writes.clear()
        self.k.debug(command)
        return self.k._debug_ticket

    def input_window(self, ticket, prompt):
        # Prompt text only synchronizes the test; authorization comes from the
        # real breakpoint snapshot and private C/D/P/Q channel, never stdout.
        pattern = re.compile(r'^Q '+re.escape(ticket)+r' 0 ([1-9][0-9]*)$', re.M)
        def waiting(state):
            proof = self.k._debug_prompt_file.read_text()
            return prompt in state['output'] and len(pattern.findall(proof))>=2
        state = self.wait(waiting)
        proof = self.k._debug_prompt_file.read_text()
        self.assertEqual(state['status'], 'running', state)
        self.assertFalse(state['waiting_input'], state)
        self.assertFalse((state.get('debug') or {}).get('ready'), state)
        self.assertFalse(self.k._reader_authorized)
        self.assertEqual(self.k._reader_mode, 'control')
        self.assertIn('D '+ticket+'\n', proof)
        self.assertLess(proof.index('D '+ticket+'\n'), proof.index('Q '+ticket))
        self.assertNotIn('P '+ticket+'\n', proof)
        self.assertEqual(len(self.writes), 1, self.writes)
        with self.assertRaises(ValueError):
            self.k.debug('continue')
        with self.assertRaises(ValueError):
            self.k.debug('eval', 'control_injected=1;')
        with self.assertRaises(ValueError):
            self.k.input('unauthorized reply')
        self.assertEqual(len(self.writes), 1, self.writes)
        print('CONTROL_INPUT', self.id(), 'state', state['status'],
              'waiting_input', state['waiting_input'], 'writes', len(self.writes),
              'proof', proof, flush=True)
        self.stop_and_reuse()

    def stop_and_reuse(self):
        self.k.interrupt()
        self.k.interrupt()
        state = self.wait(lambda s: s['status']=='idle'
                          or (s['status']=='paused' and (s.get('debug') or {}).get('ready')))
        if state['status']=='paused':
            self.k.interrupt()
            self.k.interrupt()
            state = self.wait(lambda s: s['status']=='idle')
        self.assertEqual(state['status'], 'idle', state)
        self.assertEqual(self.k.proc.pid, self.pid)
        self.assertIsNone(self.k.proc.poll())
        self.k.submit('control_after_stop=83;')
        state = self.wait(lambda s: s['status']=='idle')
        self.assertFalse(state['error'], state)
        self.assertEqual(next(v['preview'] for v in state['variables']
                              if v['name']=='control_after_stop'), '83')

    def test_step_over_builtin_input(self):
        self.fixture("reply=builtin('input','Control direct:','s');")
        self.input_window(self.control('step'), 'Control direct:')

    def test_stop_signals_do_not_overlap_reader_abort(self):
        self.fixture("reply=builtin('input','Control direct:','s');")
        sent=[]
        original=os.killpg
        def traced(pid,kind):
            if kind==signal.SIGINT:sent.append(time.monotonic())
            return original(pid,kind)
        with patch('backend.kernel.os.killpg',traced):
            self.input_window(self.control('step'), 'Control direct:')
        print('STOP_SIGNAL_INTERVALS',[b-a for a,b in zip(sent,sent[1:])],flush=True)
        self.assertTrue(sent)
        self.assertLessEqual(len(sent),4)
        for before,after in zip(sent,sent[1:]):
            self.assertGreaterEqual(after-before,.24,'Two paths interrupted the same abort concurrently')

    def test_step_over_function_input(self):
        self.fixture('reply=control_asks();')
        self.input_window(self.control('step'), 'Control function:')

    def test_step_in_function_input(self):
        _, state = self.fixture('reply=control_asks();')
        self.control('in')
        state = self.paused(state['debug']['serial'])
        self.assertEqual(Path(state['debug']['file']), self.asks)
        self.assertEqual(state['debug']['line'], 2)
        self.input_window(self.control('step'), 'Control function:')

    def test_step_out_function_input(self):
        self.fixture('reply=control_asks();', break_file=self.asks)
        self.input_window(self.control('out'), 'Control function:')

    def test_continue_input_before_next_breakpoint(self):
        self.fixture("reply=builtin('input','Control direct:','s');", later_break=3)
        # A later real breakpoint must not authenticate the pending input.
        self.input_window(self.control('continue'), 'Control direct:')

    def keyboard_control(self, statement):
        entry, state = self.fixture(statement,
                                   tail="reply=builtin('input','After keyboard:','s');\nvalue=3;\n")
        self.control('step')
        state = self.paused(state['debug']['serial'])
        self.assertEqual(Path(state['debug']['file']), entry)
        self.assertIn(state['debug']['line'], (2, 3))
        self.assertFalse(state['waiting_input'])
        self.input_window(self.control('continue'), 'After keyboard:')

    def test_step_over_keyboard(self):
        self.keyboard_control('keyboard;')

    def test_step_over_eval_keyboard(self):
        self.keyboard_control("eval('keyboard');")

    def error_control(self, command):
        _, state = self.fixture("error('control reader error');")
        self.control(command)
        state = self.wait(lambda s: s['status']=='idle'
                          or (s['status']=='paused' and (s.get('debug') or {}).get('ready')
                              and s['debug']['serial']>state['debug']['serial']))
        if state['status']=='paused':
            self.assertFalse(state['waiting_input'])
            self.k.debug('continue')
            state = self.wait(lambda s: s['status']=='idle')
        self.assertIn('control reader error', state['error'] or '', state)
        self.assertFalse(state['waiting_input'])
        self.stop_and_reuse()

    def test_step_error(self):
        self.error_control('step')

    def test_continue_error(self):
        self.error_control('continue')

    def return_from_outermost(self, keyboard):
        entry = self.work/'control_return.m'
        entry.write_text('function control_return()\n'+('keyboard;\n' if keyboard else '')+'return;\nerror("return fell through");\nendfunction\n')
        os.utime(entry,(1_600_000_000,1_600_000_000))
        self.k.breakpoint(entry, 2, True)
        self.assertFalse(self.wait(lambda s: s['status']=='idle')['error'])
        self.k.submit(mode='file', argument=str(entry))
        state = self.paused()
        self.assertEqual(Path(state['debug']['file']), entry)
        self.assertEqual(state['debug']['line'], 2)
        if keyboard:
            # Match the Round 3 keyboard pause before return, independently
            # authenticated after the fixture breakpoint.
            self.control('continue')
            state = self.paused(state['debug']['serial'])
            self.assertEqual(Path(state['debug']['file']), entry)
            self.assertEqual(state['debug']['line'], 2)
        serial = state['debug']['serial']
        self.control('out')
        state = self.wait(lambda s: s['status']=='idle'
                          or (s['status']=='paused' and (s.get('debug') or {}).get('ready')
                              and s['debug']['serial']>serial))
        self.assertFalse(state['error'], state)
        self.assertFalse(state['waiting_input'], state)
        if state['status']=='paused':
            # An outstanding native step can stop in bridge collection after
            # the outer user function returns.  Only a genuine P permits its
            # snapshot; C then P re-authenticates the resulting ready pause.
            self.assertTrue(self.k._reader_authorized)
            ticket = self.k._debug_ticket
            proof = self.k._debug_prompt_file.read_text()
            self.assertIn('C '+ticket+'\n', proof)
            self.assertIn('P '+ticket+'\n', proof)
            self.assertLess(proof.index('C '+ticket+'\n'), proof.index('P '+ticket+'\n'))
            self.assertEqual(len(self.writes), 2, self.writes)
            self.assertIn(b'(@__mf_debug_snapshot__)', self.writes[1])
            self.assertTrue(state['debug']['stack'], state)
        self.stop_and_reuse()

    def test_return_from_outermost_function(self):
        self.return_from_outermost(False)

    def test_return_after_outermost_keyboard(self):
        self.return_from_outermost(True)



if __name__=='__main__':
    unittest.main()
