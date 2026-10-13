"""Measured finite calendar-duration payloads; real octave-cli, no graphics."""
import shutil
import subprocess
import unittest
from pathlib import Path
from scripts import compat

ROOT = Path(__file__).resolve().parents[1]

@unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli unavailable')
class CalendarPayloadTests(unittest.TestCase):
    def run_body(self, body):
        setup = compat.setup().replace("set(0,'defaultfigurevisible','off'); ", '')
        setup += f"addpath({compat.quote(ROOT/'uyumluluk/yardimcilar')});"
        result = subprocess.run(['octave-cli','--quiet','--no-gui','--no-init-file',
                                 '--no-site-file','--no-history','--eval',setup+body],
                                capture_output=True,text=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

    def test_measured_components_exact_reconstruction(self):
        self.run_body("x=calendarDuration([1 0],[2 -3],[4 5],[6 7],[8 9],[10.125 11.5]);"
                      "[p,ok]=uy_ozet_nesne(x,1000); assert(ok);"
                      "assert(isequal(p.Months,[14 -3])); assert(isequal(p.Days,[4 5]));"
                      "assert(isequal(p.Seconds,[22090.125 25751.5])); assert(strcmp(p.Format,'ymdt'));"
                      "assert(isempty(strfind(uy_ozet(x),'<partial:')));")

    def test_each_component_shape_and_format_are_distinguished(self):
        self.run_body("x=calendarDuration(0,1,2,3,4,5); s=uy_ozet(x);"
                      "for y={calendarDuration(0,2,2,3,4,5),calendarDuration(0,1,3,3,4,5),"
                      "calendarDuration(0,1,2,3,4,6)}, assert(!strcmp(s,uy_ozet(y{1}))); end;"
                      "y=x; y.Format='mdt'; assert(!strcmp(s,uy_ozet(y)));"
                      "[p,ok]=uy_ozet_nesne(calmonths([1 2;3 4]),1000);"
                      "assert(ok && isequal(p.Months,[1 2;3 4]));")

    def test_unsupported_nonfinite_budget_and_handles_stay_partial(self):
        self.run_body("for x={calmonths(NaN),calmonths(Inf),calmonths(1:101),@sin},"
                      "[p,ok]=uy_ozet_nesne(x{1},100); assert(!ok && isempty(p)); end;")

if __name__ == '__main__':
    unittest.main()
