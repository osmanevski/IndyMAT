"""Real transport must preserve literal bytes or reject before writing."""
import tempfile
import time
import unittest
from pathlib import Path
from backend.kernel import Kernel
from backend.i18n import get_language, set_language
ROOT=Path(__file__).resolve().parents[1]
FLEET=ROOT
class DebugTransportBytes(unittest.TestCase):
 def setUp(self):
  previous_language=get_language();set_language('en');self.addCleanup(set_language,previous_language)
  self.temp=tempfile.TemporaryDirectory(dir=FLEET,prefix='transport-')
  self.work=Path(self.temp.name)
  self.k=Kernel(ROOT,self.work/'runtime',self.work,executable='octave-cli')
  self.wait(); self.k.submit('keyboard;'); self.wait(True)
 def tearDown(self):
  self.k.close(); self.temp.cleanup()
 def wait(self,paused=False,serial=-1):
  end=time.monotonic()+8
  while time.monotonic()<end:
   s=self.k.snapshot()
   if (s['status']=='paused' and s['debug']['ready'] and s['debug']['serial']>serial) if paused else s['status'] in ('idle','dead'): return s
   time.sleep(.01)
  self.fail(str(self.k.snapshot()))
 def check_literal(self,c):
  serial=self.k.snapshot()['debug']['serial']
  self.k.debug('eval',"literal='a"+c+"b'; assert(isequal(double(literal),[97 "+' '.join(str(x) for x in c.encode())+" 98]));")
  s=self.wait(True,serial)
  self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='literal'),'a'+c+'b',s)
  self.assertNotIn('assert (',s['output'],s['output'])
 def test_ctrl_d(self): self.check_literal(chr(4))
 def test_unicode_separator(self): self.check_literal('\u2028')
 def test_all_controls(self):
  for n in list(range(32))+[127]:
   with self.subTest(byte=n):
    if n in (0,10,13):
     ticket=self.k._debug_ticket
     message='Debug expressions cannot contain NUL (U+0000).' if n==0 else 'Enter a single-line Octave expression.'
     with self.assertRaises(ValueError) as raised:
      self.k.debug('eval',"literal='a"+chr(n)+"b';")
     self.assertEqual(str(raised.exception),message)
     self.assertEqual(ticket,self.k._debug_ticket)
    else:self.check_literal(chr(n))
 def test_splitlines_boundaries(self):
  for c in ['\v','\f','\x1c','\x1d','\x1e','\x85','\u2028','\u2029']:
   with self.subTest(boundary=ord(c)):self.check_literal(c)
 def test_control_quoted_paths(self):
  path=self.work/"quo'ted\x04\x15\x7f\u2028.m"
  path.write_text('path_result=83;')
  serial=self.k.snapshot()['debug']['serial']
  self.k.debug('eval',Kernel.call('source',str(path)))
  s=self.wait(True,serial)
  self.assertNotIn('error:',s['output'],s['output'])
  self.assertEqual(next(v['preview'] for v in s['variables'] if v['name']=='path_result'),'83')
if __name__=='__main__':unittest.main()
