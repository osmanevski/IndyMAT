"""Stop must not mistake ordinary unwinding for a blocked input reader."""
import os
import signal
import time
import unittest
from unittest.mock import patch
from tests import test_debug_stop_recovery as recovery

class StopProgress(unittest.TestCase):
    setUp=recovery.StopRecovery.setUp
    tearDown=recovery.StopRecovery.tearDown
    wait=recovery.StopRecovery.wait
    # Reuse fixtures, without inheriting the old recovery cases in this suite.
    def test_busy_loop_cleanup_is_not_signalled_again_or_terminated(self):
        (self.work/'slow_unwind.m').write_text("function slow_unwind()\n unwind_protect\n fprintf('LOOP_RUNNING\\n'); fflush(stdout); while true; end\n unwind_protect_cleanup\n fprintf('UNWIND_STARTED\\n'); fflush(stdout); builtin('pause',4); fprintf('UNWIND_FINISHED\\n'); fflush(stdout);\n end_unwind_protect\nendfunction\n")
        sent=[]; kill=os.killpg
        def trace(pid,sig):
            if pid==self.k.proc.pid and sig:sent.append((sig,time.monotonic()))
            return kill(pid,sig)
        with patch('backend.kernel.os.killpg',trace):
            self.k.submit('slow_unwind();')
            self.wait(lambda s:'LOOP_RUNNING' in s['output'])
            self.k.interrupt();self.k.interrupt()
            state=self.wait(lambda s:s['status'] in ('idle','dead'),seconds=12)
        print('SLOW_UNWIND',state['status'],state['output'],[(int(s),round(t-sent[0][1],3)) for s,t in sent],flush=True)
        self.assertEqual(state['status'],'idle',state)
        self.assertIn('UNWIND_FINISHED',state['output'])
        self.assertEqual([s for s,t in sent],[signal.SIGINT])
        self.k.submit('assert(preserved==127);')
        self.assertFalse(self.wait(lambda s:s['status']=='idle')['error'])

    def test_absent_reader_record_does_not_kill_an_unproven_slow_unwind(self):
        self.k.submit('keyboard;')
        self.wait(lambda s:s['status']=='paused' and s['debug']['ready'])
        from pathlib import Path
        read=Path.read_text;prompt=self.k._debug_prompt_file
        def omit(path,*args,**kwargs):
            value=read(path,*args,**kwargs)
            return '\n'.join(r for r in value.split('\n') if not r.startswith('Q ')) if path==prompt else value
        with patch.object(Path,'read_text',omit):
            self.k.interrupt();self.k.interrupt()
            time.sleep(3.5)  # Exercise the removed unconditional termination deadline.
            self.assertEqual(self.k.snapshot()['status'],'stopping')
            self.assertIsNone(self.k.proc.poll())
        self.assertEqual(self.wait(lambda s:s['status'] in ('idle','dead'))['status'],'idle')

    def test_busy_loop_destructor_input_gets_a_fresh_signal(self):
        (self.work/'abort_input.m').write_text("function abort_input()\n c=onCleanup(@() builtin('input','ABORT_INPUT:','s'));\n fprintf('BUSY_READY\\n'); fflush(stdout); while true; end\nendfunction\n")
        self.k.submit('abort_input();')
        self.wait(lambda s:'BUSY_READY' in s['output'])
        self.k.interrupt();self.k.interrupt()
        state=self.wait(lambda s:s['status'] in ('idle','dead'))
        print('BUSY_DESTRUCTOR',state['status'],state['output'],'proof',self.k._debug_prompt_file.read_text(),flush=True)
        for proof in self.proofs:
            readers=[r for r in proof.splitlines() if r.startswith('Q ')]
            self.assertTrue(readers[-1].endswith(' 0 0'),proof)
            self.assertTrue(any(not r.endswith(' 0 0') for r in readers),proof)
        self.assertEqual(state['status'],'idle',state)
        self.k.submit('assert(preserved==127);')
        self.assertFalse(self.wait(lambda s:s['status']=='idle')['error'])

    def test_completed_eval_waits_for_pause_instead_of_sending_sigint(self):
        self.k.submit('keyboard;')
        self.wait(lambda s:s['status']=='paused' and s['debug']['ready'])
        write=self.k._write_command;sent=[];kill=os.killpg
        def complete_then_stop(proc,command,**kwargs):
            value=write(proc,command,**kwargs)
            if kwargs.get('reader')=='eval' and 'completed_before_stop' in command:
                end=time.monotonic()+5
                while 'C '+self.k._debug_ticket not in self.k._debug_prompt_file.read_text():
                    self.assertLess(time.monotonic(),end);time.sleep(.01)
                self.k.interrupt();self.k.interrupt()
            return value
        def trace(pid,sig):
            if pid==self.k.proc.pid and sig:sent.append(sig)
            return kill(pid,sig)
        self.k._write_command=complete_then_stop
        with patch('backend.kernel.os.killpg',trace):
            self.k.debug('eval','completed_before_stop=1;')
            state=self.wait(lambda s:s['status'] in ('idle','dead'))
        print('COMPLETED_EVAL_STOP',state['status'],'signals',sent,flush=True)
        self.assertEqual(state['status'],'idle',state)
        self.assertEqual(sent,[])
