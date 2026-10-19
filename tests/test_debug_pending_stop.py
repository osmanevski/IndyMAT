"""Deterministic Stop while an evaluative debugger helper owns stdin."""
import threading
import unittest
from tests import test_debug_stop_recovery as recovery

class PendingStop(unittest.TestCase):
    setUp=recovery.StopRecovery.setUp
    tearDown=recovery.StopRecovery.tearDown
    wait=recovery.StopRecovery.wait
    pause=recovery.StopRecovery.pause
    def interrupt_after_write(self, needle):
        observed=[]; reached=threading.Event(); original=self.k._write_command
        def write(proc,command,**kwargs):
            result=original(proc,command,**kwargs)
            if needle in command and not reached.is_set():
                reached.set()
                try:self.k.interrupt();self.k.interrupt()
                except Exception as exc:observed.append(exc)
            return result
        self.k._write_command=write
        return observed,reached
    def finish(self,errors,reached):
        self.assertTrue(reached.wait(5))
        self.assertEqual(errors,[],[str(e) for e in errors])
        state=self.wait(lambda s:s['status'] in ('idle','dead'))
        self.assertEqual(state['status'],'idle',state)
        self.assertIsNone(self.k.run_to_cursor_state)
    def test_stop_pending_snapshot(self):
        errors,reached=self.interrupt_after_write('(@__mf_debug_snapshot__)')
        self.k.submit('keyboard;')
        self.finish(errors,reached)
    def test_stop_pending_run_to_cursor(self):
        path=self.work/'pending_rtc.m'
        path.write_text('function pending_rtc()\n keyboard;\n a=1;\n a=2;\nendfunction\n')
        self.pause('pending_rtc();')
        errors,reached=self.interrupt_after_write('(@__mf_run_to_cursor__)')
        self.k.run_to_cursor(path,4)
        self.finish(errors,reached)

if __name__=='__main__':unittest.main()
