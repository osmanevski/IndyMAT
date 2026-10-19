"""A repeated Stop signal needs a reader that outlived the previous one.

The hook tick that is running when a SIGINT arrives still writes its reader
record; the interrupt reaches the reader at the next tick start. That single
record is no evidence that the reader ignored the signal. Here the engine
spins in a loop (no reader, no hook ticks) and SIGINT delivery is held back,
so every reader record in the trace is injected by the test.
"""
import os
import signal
import time
import unittest
from unittest.mock import patch
from tests import test_debug_stop_recovery as support


class StopSignalEvidence(unittest.TestCase):
    setUp = support.StopRecovery.setUp
    tearDown = support.StopRecovery.tearDown
    wait = support.StopRecovery.wait

    def test_one_reader_record_after_a_signal_does_not_signal_again(self):
        sent = []
        deliver = os.killpg

        def held(pid, sig):
            if pid == self.k.proc.pid and sig == signal.SIGINT:
                sent.append(time.monotonic())
                return None
            return deliver(pid, sig)

        self.k.submit("fprintf('SPINNING\\n'); fflush(stdout); while true; end")
        self.wait(lambda s: 'SPINNING' in s['output'])
        ticket = self.k._debug_ticket
        with patch('backend.kernel.os.killpg', held):
            self.k.interrupt()
            self.k.interrupt()
            self.wait(lambda s: len(sent) == 1)
            with self.k._debug_prompt_file.open('a') as trace:
                trace.write(f'Q {ticket} 0 2\n')
            time.sleep(1.5)  # Six times the retry spacing, many collector passes.
            self.assertEqual(len(sent), 1, self.k._debug_prompt_file.read_text())
            with self.k._debug_prompt_file.open('a') as trace:
                trace.write(f'Q {ticket} 0 2\n')
            self.wait(lambda s: len(sent) == 2)
            self.assertEqual(self.k.snapshot()['status'], 'stopping')
        print('SIGNAL_EVIDENCE', self.k._debug_prompt_file.read_text(), flush=True)
        # Deliver one real interrupt: the loop ends and the root is recovered.
        os.killpg(self.k.proc.pid, signal.SIGINT)
        state = self.wait(lambda s: s['status'] in ('idle', 'dead'), seconds=20)
        self.assertEqual(state['status'], 'idle', state)
        self.assertEqual(self.k.proc.pid, self.pid)
        self.k.submit('assert(preserved==127);')
        self.assertFalse(self.wait(lambda s: s['status'] == 'idle')['error'])


if __name__ == '__main__':
    unittest.main()
