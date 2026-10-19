"""Ordinary file cleanup must not destroy the pause attestation channel."""
import tempfile
import time
import unittest
from pathlib import Path
from backend.kernel import Kernel
ROOT=Path(__file__).resolve().parents[1]
FLEET=ROOT
class ChannelCleanup(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory(dir=FLEET,prefix='reader-'); self.work=Path(self.temp.name)
  (self.work/'nested_input.m').write_text("function reply=nested_input()\nreply=builtin('input','Nested:','s');\nendfunction\n")
  self.k=Kernel(ROOT,self.work/'runtime',self.work,executable='octave-cli'); self.wait(lambda s:s['status']=='idle')
 def tearDown(self):self.k.close(); self.temp.cleanup()
 def wait(self,predicate,seconds=5):
  end=time.monotonic()+seconds
  while time.monotonic()<end:
   s=self.k.snapshot()
   if predicate(s):return s
   time.sleep(.01)
  self.fail(str(self.k.snapshot()))
 def paused(self,serial=-1):return self.wait(lambda s:s['status']=='paused' and s['debug']['ready'] and s['debug']['serial']>serial)
 def test_hook_preserves_ans(self):
  self.k.submit('ans=731; keyboard;')
  s=self.paused()
  self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='ans'),'731')
  serial=s['debug']['serial']
  self.k.debug('eval','ans=19;')
  s=self.paused(serial)
  self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='ans'),'19')
  self.k.debug('continue');self.assertFalse(self.wait(lambda s:s['status']=='idle')['error'])
 def test_nested_keyboard(self):
  self.k.submit('keyboard;');serial=self.paused()['debug']['serial']
  self.k.debug('eval',"eval('keyboard'); nested_done=17;")
  # A reader inside the still-running eval cannot be distinguished from input.
  s=self.wait(lambda s:s['waiting_input']);self.assertEqual(s['status'],'running')
  with self.assertRaises(ValueError):self.k.debug('continue')
  self.k.input('(@dbcont)();');s=self.paused(serial)
  self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='nested_done'),'17')
  self.k.debug('continue');self.assertFalse(self.wait(lambda s:s['status']=='idle')['error'])
 def test_fclose_keyboard(self):
  self.k.submit("fclose('all'); keyboard; finished=19;")
  try:s=self.paused()
  finally:print('FCLOSE_KEYBOARD',self.k.snapshot(),flush=True)
  self.assertTrue(s['debug']['ready']);self.k.debug('continue')
  self.assertFalse(self.wait(lambda s:s['status']=='idle')['error'])
 def test_fclose_breakpoint(self):
  path=self.work/'closed_entry.m';path.write_text('fclose all\nfinished=23;\n')
  self.k.breakpoint(path,2,True);self.wait(lambda s:s['status']=='idle')
  self.k.submit(mode='file',argument=str(path))
  try:s=self.paused()
  finally:print('FCLOSE_BREAKPOINT',self.k.snapshot(),flush=True)
  self.assertEqual(s['debug']['file'],str(path));self.k.debug('continue')
  self.assertFalse(self.wait(lambda s:s['status']=='idle')['error'])
if __name__=='__main__':unittest.main()
