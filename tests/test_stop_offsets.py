"""Real reader proofs, signal offsets, and lost recovery lines."""
import os
import queue
import signal
import time
import unittest
from unittest.mock import patch
from tests import test_debug_stop_recovery as support

class StopOffsets(unittest.TestCase):
    setUp=support.StopRecovery.setUp
    tearDown=support.StopRecovery.tearDown
    wait=support.StopRecovery.wait
    def stop_before_entry(self):
        write=self.k._write_command;once=True
        def held(proc,command,**kwargs):
            nonlocal once
            if once and kwargs.get('reader','job')=='job':
                once=False
                command="(@fprintf)(1,'BEFORE_ENTRY\\n'); (@fflush)(1); (@builtin)('pause',30); "+command
            return write(proc,command,**kwargs)
        self.k._write_command=held
        self.k.submit('should_not_run=1;')
        self.wait(lambda s:'BEFORE_ENTRY' in s['output'],seconds=20)
        self.k.interrupt();self.k.interrupt()
    def verify_idle(self):
        s=self.wait(lambda s:s['status'] in ('idle','dead'),seconds=20)
        print('OFFSET_STOP',s['status'],self.k._debug_prompt_file.read_text(),flush=True)
        self.assertEqual(s['status'],'idle',s)
        self.k.submit("assert(preserved==127); assert(!exist('should_not_run','var'));")
        self.assertFalse(self.wait(lambda s:s['status']=='idle',seconds=20)['error'])
    def test_sigint_before_execute_recovers_without_rerunning_user_code(self):
        self.stop_before_entry();self.verify_idle()
    def test_old_offset_root_record_is_powerless(self):
        self.stop_before_entry()
        self.wait(lambda s:'Q ' in self.k._debug_prompt_file.read_text(),seconds=20)
        # Predicate unit exercised with a real engine and its real ticket/trace.
        with self.k.lock:
            ticket=self.k._debug_ticket
            proof='A '+ticket+'\nQ '+ticket+' 0 0\n'
            self.k._trace_offset=len(proof.encode())
            self.assertFalse(self.k._root_recovery_proven(proof,ticket))
            self.k._trace_offset=len(('A '+ticket+'\n').encode())
            self.assertTrue(self.k._root_recovery_proven(proof,ticket))
            self.assertFalse(self.k._root_recovery_proven(proof,ticket+'x'))
            self.assertFalse(self.k._root_recovery_proven(proof.replace(' 0 0',' 0 2'),ticket))
            self.assertFalse(self.k._root_recovery_proven(proof.replace(' 0 0',' 1 0'),ticket))
        self.verify_idle()
    def test_signal_eats_first_partial_recovery_line(self):
        stream=self.k.proc.stdin;lost=[];owner=self
        class InterruptLine:
            def write(self,data):
                if b'(@__mf_recover__)' in data and not lost:
                    lost.append(data)
                    stream.write(b"(@fprintf)(1,'MUST_NOT_EXECUTE')")
                    # Interrupt an unfinished readline command: real SIGINT,
                    # no injected completion/reader record or prompt heuristic.
                    owner.k._send_interrupt_locked(owner.k.proc)
                    return len(data)
                return stream.write(data)
            def close(self):return stream.close()
        self.k.proc.stdin=InterruptLine()
        self.stop_before_entry();self.verify_idle()
        self.assertEqual(len(lost),1)
        self.assertNotIn('MUST_NOT_EXECUTE',self.k.snapshot()['output'])
    def test_done_in_transit_commits_once(self):
        original=self.k.events;held=[];owner=self
        class DelayDone:
            def put(self,data):return original.put(data)
            def get(self,**kwargs):
                if held:return held.pop()
                data=original.get(**kwargs)
                if data and ('__MF_DONE_'+str(owner.k.job)+'__').encode() in data:
                    held.append(data)
                    owner.k.interrupt();owner.k.interrupt()
                    raise queue.Empty
                return data
        self.k.events=DelayDone()
        self.k.submit('preserved=preserved+1;')
        s=self.wait(lambda s:s['status'] in ('idle','dead'),seconds=20)
        self.assertEqual(s['status'],'idle',s)
        self.k.events=original
        self.k.submit('assert(preserved==128);')
        self.assertFalse(self.wait(lambda s:s['status']=='idle',seconds=20)['error'])

if __name__=='__main__':unittest.main()
