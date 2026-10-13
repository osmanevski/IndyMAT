"""Real octave-cli checks for olc26_numeric_next.m norm rows k=1..8.

Expected values are MATLAB R2025b prints from olcum2/olc26-numeric-next.txt.
No norminv confidence algorithm is inferred from its four printed rows.
"""
import shutil
import subprocess
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli unavailable')
class Numeric26Round4Tests(unittest.TestCase):
    def run_octave(self, code):
        compat = str(ROOT / 'octave' / 'compat').replace("'", "''")
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file',
                                 '--no-site-file', '--no-history', '--eval',
                                 "warning('off','Octave:shadowed-function');"
                                 + "addpath('" + compat + "');" + code],
                                cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_scaled_negative_norm(self):
        self.run_octave("""
xs={[1e200 1e200],[1e-200 1e-200],[1e308 1e308]}; ps=[-2 -2 -1];
es=[7.071067811865474e199 7.071067811865475e-201 5e307];
for k=1:3, v=norm(xs{k},ps(k)); assert(isfinite(v)&&abs(v/es(k)-1)<5e-15); end;
""")

    def test_all_thirty_two_measured_rows(self):
        self.run_octave("""
xs={[3 2],[3 -2],single([3 2]),Inf,[Inf 1],[NaN Inf],zeros(1,0),zeros(0,1)};
expected=[Inf 1.2000000000000002 1.6641005886756874 2;
          Inf 1.2000000000000002 1.6641005886756874 2;
          Inf 1.2000000476837158 1.6641006469726562 2;
          1 Inf Inf Inf;Inf 1 1 1;NaN NaN NaN NaN;
          0 Inf Inf 0;0 Inf Inf 0];
ps=[0 -1 -2 -Inf];
for k=1:numel(xs),for j=1:numel(ps), y=norm(xs{k},ps(j));
assert(strcmp(class(y),class(xs{k})) && isequal(size(y),[1 1]));
e=cast(expected(k,j),class(xs{k}));
if(isfinite(e)),assert(abs(y-e)<=4*eps(e));else,assert(isequaln(y,e));end;
end;end;
""")

    def test_previous_zero_nan_and_matrix_boundaries(self):
        self.run_octave("""
for x={0,[0 0],[3 0 -2],single([3 0 -2])},
for p=[-1 -2 -Inf],assert(norm(x{1},p)==0);end;end;
for x={[],[3 0;0 -2]},for p=[0 -1 -2 -Inf],failed=false;
try,norm(x{1},p);catch e,failed=strcmp(e.identifier,'MATLAB:norm:unknownNorm');end;
assert(failed);end;end;
for p=[0 -1 -2 -Inf],assert(isnan(norm([NaN 1],p)));end;
""")

    def test_other_inputs_and_orders_are_native(self):
        self.run_octave("""
for x={[3+4i 1-2i],[1 2;3 4]},
for p={1,2,Inf,'fro'},assert(isequaln(norm(x{1},p{1}),builtin('norm',x{1},p{1})));end;end;
for p=[0 -1 -2 -Inf],native='';shadow='';
try,builtin('norm',int32([3 2]),p);catch e,native=e.message;end;
try,norm(int32([3 2]),p);catch e,shadow=e.message;end;
assert(!isempty(native) && strcmp(native,shadow));end;
for x={[3 2],single([3 2])},for p=[.5 3 -3],
assert(isequaln(norm(x{1},p),builtin('norm',x{1},p)));end;end;
""")


if __name__ == '__main__':
    unittest.main()
