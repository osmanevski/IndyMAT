"""A real DONE crossing Stop must not bypass the stopped note."""
import tempfile
import time
import unittest
from pathlib import Path
from backend.kernel import Kernel
from backend.i18n import set_language
ROOT=Path(__file__).resolve().parents[1]

class InterruptNote(unittest.TestCase):
    def test_done_arrives_between_stop_check_and_result_commit(self):
        for language,expected in [('en','Operation stopped.'),('tr','İşlem durduruldu.')]:
            with self.subTest(language=language), tempfile.TemporaryDirectory(dir=ROOT,prefix='stop-note-') as d:
                set_language(language)
                k=Kernel(ROOT,Path(d)/'runtime',Path(d),executable='octave-cli')
                try:
                    self.wait(k)
                    queue=k.events
                    class Gate:
                        armed=True
                        hits=0
                        received=b''
                        def get(self,**kwargs):
                            data=queue.get(**kwargs)
                            if data:self.received+=data
                            marker=('__MF_DONE_'+str(k.job)+'__').encode()
                            if self.armed and data and marker in self.received:
                                self.armed=False;self.hits+=1
                                # Real Stop calls at this exact DONE boundary.
                                # Completion cancels the now-unnecessary pending
                                # signal before the next user command can start.
                                k.interrupt();k.interrupt()
                            return data
                        def put(self,data):return queue.put(data)
                    gate=Gate(); k.events=gate
                    for i in range(10):
                        gate.armed=True;gate.received=b''
                        k.submit('kept_after_race=29;')
                        s=self.wait(k)
                        self.assertEqual(s['status'],'idle',s)
                        self.assertEqual(s['error'],expected,s)
                    self.assertEqual(gate.hits,10)
                    k.submit('assert(kept_after_race==29);')
                    self.assertFalse(self.wait(k)['error'])
                finally:k.close();set_language('en')
    def wait(self,k):
        end=time.monotonic()+8
        while time.monotonic()<end:
            s=k.snapshot()
            if s['status'] in ('idle','dead'):return s
            time.sleep(.01)
        self.fail(str(k.snapshot()))
if __name__=='__main__':unittest.main()
