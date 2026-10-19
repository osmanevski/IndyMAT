"""Repeated ordinary Stop keeps one persistent engine and its workspace."""
import time
import unittest
from tests import test_debug_stop_recovery as support

class RepeatedStop(unittest.TestCase):
    setUp=support.StopRecovery.setUp
    tearDown=support.StopRecovery.tearDown
    wait=support.StopRecovery.wait
    def repeat(self,code,delay):
        self.k.submit('kept_handle=@(x)x*x;')
        self.wait(lambda s:s['status']=='idle',seconds=30)
        pid=self.k.proc.pid
        for index in range(20):
            self.k.submit(code)
            time.sleep(delay)  # Reproduce the requested user Stop timing.
            self.k.interrupt();self.k.interrupt()
            s=self.wait(lambda s:s['status'] in ('idle','dead'),seconds=30)
            self.assertEqual(s['status'],'idle',(index,s))
            self.assertEqual(self.k.proc.pid,pid)
            self.k.submit('assert(preserved==127); assert(kept_handle(3)==9);')
            s=self.wait(lambda s:s['status'] in ('idle','dead'),seconds=30)
            self.assertEqual(s['status'],'idle',(index,s))
            self.assertFalse(s['error'],(index,s))
        print('REPEATED_STOP',code,'20/20 idle; same engine, variable and handle preserved',flush=True)
    def test_pause_stop_twenty_times(self):self.repeat('pause(1);',0.1)
    def test_busy_loop_stop_twenty_times(self):self.repeat('while true; end;',0.3)

if __name__=='__main__':unittest.main()
