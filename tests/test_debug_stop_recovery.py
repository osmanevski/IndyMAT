"""Stop uses post-dispatch reader proof; absent proof ends visibly, never resets."""
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch
from backend.kernel import Kernel
from backend.i18n import get_language, set_language

ROOT=Path(__file__).resolve().parents[1]

class StopRecovery(unittest.TestCase):
    def setUp(self):
        previous_language=get_language();set_language('en');self.addCleanup(set_language,previous_language)
        self.temp=tempfile.TemporaryDirectory(dir=ROOT,prefix='stop-proof-')
        self.work=Path(self.temp.name)
        (self.work/'cleanup_input.m').write_text("function cleanup_input()\n c=onCleanup(@() builtin('input','Destructor:','s'));\n keyboard;\nendfunction\n")
        self.k=Kernel(ROOT,self.work/'runtime',self.work,executable='octave-cli')
        self.wait(lambda s:s['status']=='idle')
        self.k.submit('preserved=127;')
        self.wait(lambda s:s['status']=='idle')
        self.pid=self.k.proc.pid; self.generation=self.k.generation
        self.proofs=[];self.writes=[]
        original=self.k._write_command
        def observe(proc,command,**kwargs):
            if "'snapshot'" in command:
                self.proofs.append(self.k._debug_prompt_file.read_text())
            self.writes.append(command)
            return original(proc,command,**kwargs)
        self.k._write_command=observe
        if hasattr(self.k,'_write_recovery_locked'):
            recover=self.k._write_recovery_locked
            def observe_recovery(*args):
                proof=self.k._debug_prompt_file.read_text()
                written=recover(*args)
                if written:
                    self.proofs.append(proof)
                    self.writes.append('authenticated recovery snapshot')
                return written
            self.k._write_recovery_locked=observe_recovery
    def tearDown(self):self.k.close();self.temp.cleanup()
    def wait(self,predicate,seconds=8):
        end=time.monotonic()+seconds
        while time.monotonic()<end:
            s=self.k.snapshot()
            if predicate(s):return s
            time.sleep(.01)
        self.fail(str(self.k.snapshot()))
    def pause(self,code):
        self.k.submit(code)
        self.wait(lambda s:s['status']=='paused' and s['debug']['ready'])
        self.writes.clear()
    def stopped(self):
        self.k.interrupt();self.k.interrupt()
        s=self.wait(lambda s:s['status'] in ('idle','dead'))
        self.assertEqual(s['status'],'idle',s)
        self.assertEqual(self.k.proc.pid,self.pid)
        self.assertEqual(self.k.generation,self.generation)
        self.k.submit('assert(preserved==127);')
        self.assertFalse(self.wait(lambda s:s['status']=='idle')['error'])
    def test_root_recovery_after_nested_abort(self):
        self.pause('keyboard;')
        self.k.debug('eval',"eval('keyboard');")
        self.wait(lambda s:s['waiting_input'])
        self.stopped()
        self.assertEqual(len(self.proofs),1,self.proofs)
        records=self.proofs[0].splitlines()
        self.assertTrue(records[0].startswith('A '),records)
        self.assertEqual(records[1],'D '+records[0][2:],records)
        self.assertIn('Q '+records[0][2:]+' 0 0',records)
    def test_destructor_reader_gets_interrupt_not_protocol(self):
        self.pause('cleanup_input();')
        self.stopped()
        self.assertEqual(len(self.proofs),1,self.proofs)
        records=self.proofs[0].splitlines()
        counts=[int(r.split()[-1]) for r in records if r.startswith('Q ')]
        self.assertTrue(any(n>0 for n in counts),records)
        self.assertEqual(counts[-1],0,records)
    def test_recovery_snapshot_is_not_interrupted_by_a_pending_retry(self):
        callback=("if !isdebugmode() && !isempty(getenv('INDYMAT_DEBUG_CONTROL')); "
                  "remove_input_event_hook(hook_id); "
                  "evalin('base',\"reply=input('Hook input:','s');\"); endif;")
        self.pause('hook_id=add_input_event_hook(@eval,'+self.k.quote(callback)+'); keyboard;')
        stream=self.k.proc.stdin
        class SlowRecovery:
            def write(self,data):
                if b'(@__mf_recover__)' in data:
                    data=b"(@builtin)('pause',0.6); "+data
                return stream.write(data)
            def close(self):return stream.close()
        self.k.proc.stdin=SlowRecovery()
        self.stopped()
        self.assertEqual(len(self.proofs),1,self.proofs)
    def test_repeated_unproven_readers_terminate_visibly_and_restart_is_explicit(self):
        self.pause('keyboard;')
        prompt=self.k._debug_prompt_file
        read=Path.read_text
        reads=0
        def omit_proof(path,*args,**kwargs):
            nonlocal reads
            value=read(path,*args,**kwargs)
            # Fault injection: every root proof is reported as a user reader.
            # Termination needs repeated reader evidence after each SIGINT;
            # a missing record alone cannot distinguish a slow unwind.
            if path==prompt and '\nD ' in value:
                reads+=1
                ticket=value.split('\n')[0][2:]
                return 'A '+ticket+'\nD '+ticket+'\n'+('Q '+ticket+' 0 1\n')*reads
            return value
        with patch.object(Path,'read_text',omit_proof):
            self.k.interrupt();self.k.interrupt()
            s=self.wait(lambda s:s['status']=='dead')
        self.assertIn('Variables could not be recovered.',s['error'])
        self.assertEqual(self.k.generation,self.generation)
        self.assertFalse(self.proofs)
        self.k.reset()
        self.assertEqual(self.wait(lambda s:s['status']=='idle')['status'],'idle')
        self.assertGreater(self.k.generation,self.generation)

if __name__=='__main__':unittest.main()
