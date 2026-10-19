"""Stop latches while trusted evaluative helpers are still executing."""
import os
import signal
import unittest
from unittest.mock import patch
from tests import test_debug_stop_recovery as support

class HelperStop(unittest.TestCase):
    setUp=support.StopRecovery.setUp
    tearDown=support.StopRecovery.tearDown
    wait=support.StopRecovery.wait
    pause=support.StopRecovery.pause
    def run_latched(self,needle,run,expected_signals=0):
        original=self.k._write_command;hit=[];signals=[];kill=os.killpg
        def write(proc,command,**kwargs):
            target=needle in command and not hit
            if target:command="(@builtin)('pause',0.8); "+command
            result=original(proc,command,**kwargs)
            if target:
                hit.append(True);self.k.interrupt();self.k.interrupt()
            return result
        def trace(pid,sig):
            if pid==self.k.proc.pid and sig==signal.SIGINT:signals.append(sig)
            return kill(pid,sig)
        self.k._write_command=write
        with patch('backend.kernel.os.killpg',trace):
            run()
            state=self.wait(lambda s:s['status'] in ('idle','dead'),seconds=25)
        print('HELPER_STOP',needle,state['status'],'SIGINT',len(signals),flush=True)
        self.assertTrue(hit)
        self.assertEqual(state['status'],'idle',state)
        self.assertEqual(len(signals),expected_signals)
    def test_snapshot_stop_waits_for_completion(self):
        self.run_latched('(@__mf_debug_snapshot__)',lambda:self.k.submit('keyboard;'))
    def test_inspection_stop_waits_for_completion(self):
        self.pause('keyboard;')
        self.run_latched('(@__mf_debug_inspect__)',lambda:self.k.debug('inspect','preserved'))
    def test_rtc_stop_waits_for_completion(self):
        path=self.work/'latched_rtc.m'
        path.write_text('function latched_rtc()\n keyboard;\n a=1;\n a=2;\nendfunction\n')
        self.pause('latched_rtc();')
        self.run_latched('(@__mf_run_to_cursor__)',lambda:self.k.run_to_cursor(path,4))
    def test_no_record_progress_watchdog_falls_back_to_one_signal(self):
        self.k.HELPER_STOP_WATCHDOG=0.1  # Inject the clock bound, not a signal burst.
        self.run_latched('(@__mf_debug_snapshot__)',lambda:self.k.submit('keyboard;'),expected_signals=1)
    def test_user_debug_eval_remains_interruptible(self):
        self.pause('keyboard;')
        self.k.debug('eval',"fprintf('EVAL_RUNNING\\n'); fflush(stdout); while true; end;")
        self.wait(lambda s:'EVAL_RUNNING' in s['output'],seconds=20)
        self.k.interrupt();self.k.interrupt()
        self.assertEqual(self.wait(lambda s:s['status'] in ('idle','dead'),seconds=25)['status'],'idle')

if __name__=='__main__':unittest.main()
