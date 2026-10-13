"""R2025b covariance rows and documented delta method, private patch tree only."""
import contextlib
import io
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from scripts import patch_packages

ROOT = Path(__file__).resolve().parents[1]
SCRATCH = Path.home()/'.cache/indymat-fleet/dalga26/uyum-tmp'

@unittest.skipUnless(shutil.which('octave-cli'),'octave-cli unavailable')
class NorminvConfidenceTests(unittest.TestCase):
    def test_measured_covariance_bounds_and_guards(self):
        SCRATCH.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=SCRATCH) as tmp:
            project=Path(tmp); package=project/'.packages/statistics-2.0.0'
            shutil.copytree(ROOT/'.packages/statistics-2.0.0',package,symlinks=True)
            with contextlib.redirect_stdout(io.StringIO()):
                patch_packages.patch_package(project,ROOT/'octave/paket-yamalari/statistics-2.0.0')
            body=f"addpath('{package}/Distribution_Functions');"+r"""
p=[.1 .5 .9];
[x,lo,hi]=norminv(p,2,3,eye(2));
assert(max(abs(x-[-1.8446546966338015 2 5.8446546966338015]))<1e-12);
assert(max(abs(lo-[-5.030651009351967 .040036015459945506 2.6586583839156357]))<1e-12);
assert(max(abs(hi-[1.3413416160843643 3.9599639845400545 9.030651009351967]))<1e-12);
[x,lo,hi]=norminv(p,2,3,[4 1;1 9]);
assert(max(abs(lo-[-9.7378038341543736 -1.919927969080109 -3.2103943683732155]))<1e-12);
assert(max(abs(hi-[6.0484944408867714 5.919927969080109 14.899703761640819]))<1e-12);
[x,lo,hi]=norminv(p,2,3,zeros(2));assert(isequal(x,lo)&&isequal(x,hi));
failed=false;try,[x,lo,hi]=norminv(p,2,3,[1 2;2 1]);catch e,failed=strcmp(e.identifier,'stats:norminv:BadCovarianceSymPos');end;assert(failed);
for bad={single(p),p',[0 .5 1]}
failed=false;try,[x,lo,hi]=norminv(bad{1},2,3,eye(2));catch e,failed=strcmp(e.identifier,'IndyMAT:norminv:UnsupportedConfidenceForm');end;assert(failed);end;
assert(isequal(norminv(p,0,1,'upper'),norminv(p,0,1,'bogus')));
"""
            run=subprocess.run(['octave-cli','--quiet','--no-gui','--no-init-file','--no-site-file',
                                '--no-history','--eval',body],capture_output=True,text=True,timeout=30)
            self.assertEqual(run.returncode,0,run.stdout+run.stderr)
