import os,shutil,sys,tempfile,time,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from backend.lint import LintTimeout,OctaveLinter

class LintTests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
  self.linter=OctaveLinter(self.root/'runtime',os.environ.get('MATLAB_FREE_OCTAVE') or shutil.which('octave'),timeout=5)
 def tearDown(self):self.temp.cleanup()
 def test_valid_file_has_no_issues(self):self.assertEqual(self.linter.check('x = 1;\ny = x + 2;'),[])
 def test_syntax_errors_report_octave_line_and_column(self):
  issue=self.linter.check('x = ;')[0];self.assertEqual((issue['line'],issue['column']),(1,5))
  issue=self.linter.check('if true\n  disp(1);')[0];self.assertEqual(issue['line'],3)
 def test_parse_does_not_execute_buffer(self):
  marker=self.root/'must-not-exist';quoted=str(marker).replace("'","''")
  code=f"disp('MUST NOT PRINT'); fid=fopen('{quoted}','w'); fprintf(fid,'executed'); fclose(fid);"
  self.assertEqual(self.linter.check(code),[]);self.assertFalse(marker.exists())
 def test_timeout_is_bounded_and_temp_file_is_removed(self):
  sleeper=self.root/'slow-octave';sleeper.write_text('#!/bin/sh\nexec sleep 10\n');sleeper.chmod(0o700)
  linter=OctaveLinter(self.root/'slow-runtime',sleeper,timeout=.1);started=time.monotonic()
  with self.assertRaises(LintTimeout):linter.check('x=1;')
  self.assertLess(time.monotonic()-started,1);self.assertEqual(list((self.root/'slow-runtime').glob('buffer-*.m')),[])
 def test_size_limit(self):
  with self.assertRaisesRegex(ValueError,'500 KB'):self.linter.check('x'*500_001)


def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main(verbosity=2)
