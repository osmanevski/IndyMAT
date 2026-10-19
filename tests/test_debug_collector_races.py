"""Real collector dispatch interleavings and fail-closed thread failures."""
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from backend.i18n import get_language, set_language
from backend.kernel import Kernel

ROOT=Path(__file__).resolve().parents[1]

class CollectorRaces(unittest.TestCase):
    def setUp(self):
        previous=get_language();set_language('en');self.addCleanup(set_language,previous)
        self.temp=tempfile.TemporaryDirectory(dir=ROOT,prefix='collector-race-')
        self.work=Path(self.temp.name)
        self.k=Kernel(ROOT,self.work/'runtime',self.work,executable='octave-cli')
        self.wait(lambda s:s['status']=='idle')
    def tearDown(self):
        self.k.close();self.temp.cleanup()
    def wait(self,predicate,seconds=8):
        end=time.monotonic()+seconds
        while time.monotonic()<end:
            state=self.k.snapshot()
            if predicate(state):return state
            time.sleep(.01)
        self.fail(str(self.k.snapshot()))
    def test_stop_supersedes_attested_prompt_before_collector_dispatch(self):
        k=self.k;lock=k.lock;fired=threading.Event();failures=[];writes=[];tickets=[]
        original=k._write_control
        def trace(*args,**kwargs):
            result=original(*args,**kwargs)
            writes.append(args[1])
            return result
        k._write_control=trace
        class Interleave:
            def __enter__(self):lock.acquire();return self
            def __exit__(self,*args):
                eligible=(not fired.is_set() and threading.current_thread() is not threading.main_thread()
                          and k._reader_authorized and k.state['status']=='running')
                lock.release()
                if eligible:
                    fired.set();tickets.append(k._debug_ticket)
                    k.interrupt();k.interrupt()
                    tickets.append(k._debug_ticket)
        k.lock=Interleave()
        def failed(args):failures.append(args.exc_value)
        with patch.object(threading,'excepthook',failed):
            k.submit('keyboard;')
            self.assertTrue(fired.wait(5))
            state=self.wait(lambda s:bool(failures) or s['status'] in ('idle','dead'))
        print('SUPERSEDED_PROMPT',tickets,'writes',writes,'status',state['status'],
              'thread_errors',[repr(e) for e in failures],flush=True)
        self.assertFalse(failures,'Collector exited on a superseded prompt')
        self.assertNotEqual(tickets[0],tickets[1])
        self.assertEqual(state['status'],'idle',state)
        self.assertEqual(writes,[k.call('dbquit')])
        k.submit('after_race=73;')
        self.assertFalse(self.wait(lambda s:s['status']=='idle')['error'])
    def test_unexpected_collector_exception_is_visible_dead_state(self):
        queue=self.k.events;failures=[]
        class BrokenQueue:
            def get(self,**kwargs):raise RuntimeError('injected collector failure')
            def put(self,data):return queue.put(data)
        self.k.events=BrokenQueue()
        with patch.object(threading,'excepthook',lambda args:failures.append(args.exc_value)):
            self.k.submit('preserved_before_failure=19;')
            state=self.wait(lambda s:bool(failures) or s['status']=='dead')
        print('COLLECTOR_FAILURE',state['status'],state['error'],[repr(e) for e in failures],flush=True)
        self.assertFalse(failures,'Unexpected exception escaped the collector boundary')
        self.assertEqual(state['status'],'dead',state)
        self.assertIn('injected collector failure',state['error'])
        self.assertIn('Variables could not be recovered.',state['error'])
        self.assertFalse(state['waiting_input'])
        self.assertIsNone(state['debug'])
        self.k.interrupt();self.k.interrupt()
        self.assertEqual(self.k.snapshot()['status'],'dead')
        self.k.reset()
        self.assertEqual(self.wait(lambda s:s['status']=='idle')['status'],'idle')

if __name__=='__main__':unittest.main()
