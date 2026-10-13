"""Real octave-cli checks for the narrow numeric compatibility shadows.

These are Octave regression checks, not new MATLAB-reference cases.
"""
from pathlib import Path
import shutil
import subprocess
import unittest


class NumericEdgeHelpers(unittest.TestCase):
    def test_real_engine(self):
        octave = shutil.which("octave-cli")
        if not octave:
            self.skipTest("octave-cli unavailable")
        compat = Path(__file__).resolve().parents[1] / "octave" / "compat"
        script = """
warning ('off', 'Octave:shadowed-function');
addpath ('%s', '-begin');
assert (all (isnan (hypot ([Inf NaN], [NaN Inf]))));
printed = evalc ('hypot (Inf, NaN)');
assert (isnan (ans));
assert (isequal (hypot ([3 5], [4 12]), [5 13]));
assert (isa (hypot (single ([3 5]), single ([4 12])), 'single'));
assert (isequaln (hypot ([Inf; 3], [NaN 4]), [NaN Inf; NaN 5]));
assert (isequaln (hypot (NaN, [Inf 4]), [NaN NaN]));
assert (hypot (3, 4, 12) == builtin ('hypot', 3, 4, 12));
assert (trace ([1 2;3 4]) == 5);
assert (trace (zeros (0)) == 0);
assert (isequal (trace (single (eye (2))), single (2)));
failed = false;
try, trace ([1 2 3;4 5 6]); catch err
  failed = strcmp (err.identifier, 'MATLAB:trace:square');
end_try_catch
assert (failed);
assert (isequal (gcd ([12 18 21], [8 24 14]), [4 6 7]));
assert (isequal (gcd (12, [8 24]), builtin ('gcd', 12, [8 24])));
[g, a, b] = gcd ([12 18], [8 24]);
assert (isequal (a .* [12 18] + b .* [8 24], g));
assert (gcd (12, 8, 20) == builtin ('gcd', 12, 8, 20));
failed = false;
try, gcd ([12;18], [8 24]); catch err
  failed = strcmp (err.identifier, 'MATLAB:gcd:InputSizeMismatch');
end_try_catch
assert (failed);
original = __mf_original_function__ ('dec2base');
assert (isequal (dec2base ([0 1 10 35], 36), original ([0 1 10 35], 36)));
assert (isequal (dec2base ([3 5 8], 2, 5), original ([3 5 8], 2, 5)));
assert (isequal (dec2base (10, 'ab'), original (10, 'ab')));
assert (isequal (dec2base (-10, 16, 4, 2), original (-10, 16, 4, 2)));
failed = false;
try, dec2base ([-1 -10], 16); catch err
  failed = strcmp (err.identifier, 'MATLAB:dec2base:FirstArg');
end_try_catch
assert (failed);
assert (strcmp (dec2bin (-3), '11111101'));
assert (strcmp (dec2bin (-3, 9), '111111101'));
assert (strcmp (dec2hex (-3), 'FD'));
assert (strcmp (dec2hex (-3, 3), 'FFD'));
disp ('NUMERIC_EDGE_HELPERS_OK');
""" % str(compat).replace("'", "''")
        result = subprocess.run(
            [octave, "--quiet", "--no-gui", "--eval", script],
            text=True, capture_output=True, timeout=45, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("NUMERIC_EDGE_HELPERS_OK", result.stdout)


if __name__ == "__main__":
    unittest.main()
