"""Real Octave checks for recorded MATLAB rejection gaps.

Negative expectations come from the existing R2025b probe records, not a new
MATLAB run. Positive checks verify unchanged delegation to Octave originals.
"""
import shutil
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which("octave-cli")


@unittest.skipUnless(OCTAVE, "octave-cli is required")
class TextValidation26Tests(unittest.TestCase):
    def octave(self, body):
        path = str(ROOT / "octave" / "compat").replace("'", "''")
        prelude = f"warning('off','Octave:shadowed-function'); addpath('{path}'); "
        result = subprocess.run(
            [OCTAVE, "--quiet", "--no-init-file", "--no-history", "--eval", prelude + body],
            cwd=ROOT, capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    @staticmethod
    def rejects(expression, identifier):
        return (
            "caught=false; try, " + expression + "; catch err, caught=true; "
            f"assert(strcmp(err.identifier,'{identifier}')); end; assert(caught); "
        )

    def test_strncmpi_vector_length_and_builtin_delegation(self):
        self.octave(
            self.rejects("strncmpi({'Abc','XY'},{'aBd','xy'},[2 1])", "MATLAB:strcmp:SizeMustBeInt")
            + "assert(isequal(strncmpi({'Abc','XY'},{'aBd','xy'},2),"
            "builtin('strncmpi',{'Abc','XY'},{'aBd','xy'},2))); "
            "assert(strncmpi('Abc','aBd',2)); assert(~strncmpi('Abc','aBd',3)); "
            "strncmpi('Abc','aBd',2); "
        )

    def test_strfind_multi_pattern_and_builtin_delegation(self):
        self.octave(
            self.rejects("strfind('abc',{'a','bc','z'})", "MATLAB:string:MustBeTextOrPatternScalar")
            + "assert(isequal(strfind({'banana','ana'},'ana'),"
            "builtin('strfind',{'banana','ana'},'ana'))); "
            "assert(isequal(strfind('aaaa','aa','overlaps',false),"
            "builtin('strfind','aaaa','aa','overlaps',false))); "
            "assert(isequal(strfind('abc',{'a'}),builtin('strfind','abc',{'a'}))); "
            "assert(isempty(strfind('abc','z'))); strfind('abc','a'); "
        )

    def test_strrep_multirow_and_builtin_delegation(self):
        self.octave(
            self.rejects("strrep(char('aba','bab'),'a','XY')", "MATLAB:strrep:MultiRowInput")
            + "assert(strcmp(strrep('aba','a','XY'),'XYbXY')); "
            "assert(isequal(strrep({'aba','bab'},'a','XY'),"
            "builtin('strrep',{'aba','bab'},'a','XY'))); "
            "assert(isequal(strrep('aaaa','aa','X','overlaps',false),"
            "builtin('strrep','aaaa','aa','X','overlaps',false))); "
            "assert(strcmp(strrep('abc','z',''),'abc')); strrep('abc','a','b'); "
        )

    def test_structfun_empty_array_and_original_delegation(self):
        self.octave(
            self.rejects("structfun(@isempty,struct('a',{}))", "MATLAB:structfun:NonScalar")
            + "original=__mf_original_function__('structfun'); s=struct('a',1,'b',2); "
            "assert(isequal(structfun(@(x) x+1,s),original(@(x) x+1,s))); "
            "assert(isequal(structfun(@(x) [x x],s,'UniformOutput',false),"
            "original(@(x) [x x],s,'UniformOutput',false))); "
            "[a,b]=structfun(@(x) deal(x,x+1),s); "
            "[c,d]=original(@(x) deal(x,x+1),s); assert(isequal(a,c)&&isequal(b,d)); "
            "assert(isequal(structfun(@isempty,struct()),original(@isempty,struct()))); "
            "structfun(@isempty,s); "
        )


if __name__ == "__main__":
    unittest.main()
