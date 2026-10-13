"""Real octave-cli acceptance checks for cumulative/array validation shadows."""
import shutil
import subprocess
import unittest
from pathlib import Path


class ValidationHelpers(unittest.TestCase):
    def test_real_engine(self):
        exe = shutil.which('octave-cli')
        if not exe:
            self.skipTest('octave-cli unavailable')
        root = Path(__file__).resolve().parents[1]
        script = "warning('off','Octave:shadowed-function');addpath('%s');" % str(root/'octave'/'compat').replace("'", "''")
        script += """
assert(isequal(cumsum([1 2;3 4]),builtin('cumsum',[1 2;3 4])));
assert(isequal(cumsum([1 2 3],'reverse'),builtin('cumsum',[1 2 3],'reverse')));
assert(isequal(cummax([2 1 3]),builtin('cummax',[2 1 3])));
assert(isequal(cummin([2 1 3]),builtin('cummin',[2 1 3])));
assert(isequal(permute(reshape(1:24,2,3,4),[3 1 2]),builtin('permute',reshape(1:24,2,3,4),[3 1 2])));
assert(isequal(bitcmp(uint16(1)),uint16(65534)));
failed=false;try,[a,b]=cummax([2 1 3]);catch e,failed=strcmp(e.identifier,'MATLAB:maxlhs');end;assert(failed);
failed=false;try,[a,b]=cummin([2 1 3]);catch e,failed=strcmp(e.identifier,'MATLAB:maxlhs');end;assert(failed);
failed=false;try,cumsum([1 2;3 4],'all');catch e,failed=strcmp(e.identifier,'MATLAB:cumsum:wrongFlag');end;assert(failed);
s=sparse([1 0;0 2]); r=permute(s,[2 1]); assert(issparse(r) && isequal(r,s));
r=permute(sparse([1 0 3]),[2 1]); assert(issparse(r) && isequal(full(r),[1;0;3]));
failed=false;try,bitcmp(uint16(1),8);catch e,failed=strcmp(e.identifier,'MATLAB:bitcmp:unknownTypeString');end;assert(failed);
assert(isequal(spline([0 1 2],[0 1 0],[0 1 2]),[0 1 0]));
assert(max(abs(spline([0 1 2],[0 0 1 4 4],[0 .5 2])-[0 .25 4]))<1e-12);
failed=false;try,spline([0 1 2],[1 0 1 0],[0 .5 2]);catch e,failed=strcmp(e.identifier,'MATLAB:chckxy:NumSitesMismatchValues');end;assert(failed);
disp('VALIDATION26_OK');
"""
        result = subprocess.run([exe,'--no-gui','--quiet','--no-init-file','--no-site-file','--no-history','--eval',script],capture_output=True,text=True,timeout=45)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertIn('VALIDATION26_OK',result.stdout)


if __name__ == '__main__':
    unittest.main()
