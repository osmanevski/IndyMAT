import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class TextPatternHelperTests(unittest.TestCase):
    def run_octave(self, body):
        command = ("warning('off','Octave:shadowed-function'); addpath('%s'); "
                   "addpath('%s'); %s") % (ROOT / 'octave' / 'compat',
                                          ROOT / '.packages' / 'datatypes-1.5.0', body)
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file', '--eval', command],
                                cwd=ROOT, text=True, capture_output=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_greedy_runs_and_cardinality_neighbors(self):
        self.run_octave("assert(isequal(extract('a12 b1234',digitsPattern(1,3)),{'12';'123';'4'})); "
                        "assert(isequal(extract('a12345',digitsPattern(2)),{'12';'34'})); "
                        "assert(isequal(extract('a12345',digitsPattern(2,Inf)),{'12345'})); "
                        "assert(isequal(extract('ab cde',lettersPattern(2)),{'ab';'cd'})); "
                        "assert(isequal(extract('ab cDE',lettersPattern),{'ab';'cDE'})); "
                        "assert(~contains('a1-b',alphanumericsPattern(3))); "
                        "assert(contains('a1b-',alphanumericsPattern(3))); "
                        "assert(isequal(extract(sprintf('x \\t\\r\\ny'),whitespacePattern),{sprintf(' \\t\\r\\n')}));")

    def test_query_shapes_and_case_options(self):
        self.run_octave("v={'a1','abc';'22','X'}; assert(isequal(contains(v,digitsPattern),logical([1 0;1 0]))); "
                        "assert(isequal(count(v,digitsPattern),[1 0;1 0])); "
                        "s=string({'abc','123'}); assert(isequal(startsWith(s,lettersPattern),logical([1 0]))); "
                        "assert(isequal(endsWith(string({'abc','a1'}),digitsPattern),logical([0 1]))); "
                        "assert(contains('ABC',pattern('ab'),'IgnoreCase',true)); "
                        "assert(~contains('ABC',pattern('ab'),'IgnoreCase',false)); "
                        "assert(~endsWith(sprintf('a\\n'),lettersPattern)); "
                        "assert(contains('',optionalPattern('x')) && startsWith('',optionalPattern('x'))); "
                        "assert(isequal(size(contains(cell(0,2),digitsPattern)),[0 2]));")

    def test_extract_first_singleton_dimension(self):
        self.run_octave("r=extract(string('a1 b22'),digitsPattern); assert(isa(r,'string') && isequal(cellstr(r),{'1';'22'})); "
                        "r=extract(string({'a1b2','c3d4'}),digitsPattern); assert(isequal(cellstr(r),{'1','3';'2','4'})); "
                        "r=extract(string({'a1b2';'c3d4'}),digitsPattern); assert(isequal(cellstr(r),{'1','2';'3','4'})); "
                        "s=string({'a1b2','e5f6';'c3d4','g7h8'}); r=extract(s,digitsPattern); "
                        "assert(isequal(size(r),[2 2 2])); c=cellstr(r); "
                        "assert(isequal(c(:,:,1),{'1','5';'3','7'}) && isequal(c(:,:,2),{'2','6';'4','8'})); "
                        "assert(isequal(size(extract('abc',digitsPattern)),[0 1])); "
                        "failed=false; try, extract({'a1','b'},digitsPattern); catch e, failed=~isempty(strfind(e.message,'extract:')); end; assert(failed);")

    def test_optional_composition_and_literal_escaping(self):
        self.run_octave("p=digitsPattern+optionalPattern('kg'); r=extract(string({'12kg','7'}),p); "
                        "assert(isequal(cellstr(r),{'12kg','7'})); "
                        "p=string('x')+optionalPattern(digitsPattern); r=extract(string({'x12','x'}),p); "
                        "assert(isequal(cellstr(r),{'x12','x'})); "
                        "p=optionalPattern(digitsPattern)+string('x'); assert(isequal(extract('12x x',p),{'12x';'x'})); "
                        "p=pattern('a.b')|pattern('c+'); assert(isequal(extract('axb a.b c+',p),{'a.b';'c+'})); "
                        "assert(strcmp(replace('a1b22',digitsPattern,'$1'),'a$1b$1')); "
                        "assert(strcmp(replace('a1',digitsPattern,''),'a'));")

    def test_string_and_mixed_editing_bridges(self):
        self.run_octave("s=string({'a1','b22'}); r=replace(s,digitsPattern,'N'); assert(isa(r,'string') && isequal(cellstr(r),{'aN','bN'})); "
                        "r=insertBefore(s,digitsPattern,'_'); assert(isequal(cellstr(r),{'a_1','b_22'})); "
                        "r=insertAfter(s,digitsPattern,'_'); assert(isequal(cellstr(r),{'a1_','b22_'})); "
                        "assert(strcmp(insertBefore('a1b2',digitsPattern,'_'),'a_1b_2')); "
                        "assert(strcmp(insertAfter('a1b2',digitsPattern,'_'),'a1_b2_')); "
                        "assert(strcmp(replace('a1',digitsPattern,string('N')),'aN')); "
                        "assert(isequal(replace({'a1';'b2'},digitsPattern,'N'),{'aN';'bN'})); "
                        "assert(strcmp(replace('abc',digitsPattern,'N'),'abc'));")

    def test_unsupported_subsets_are_explicit(self):
        self.run_octave("for action={'p=digitsPattern(0);','p=digitsPattern(-1);','p=digitsPattern(3,2);', "
                        "'p=digitsPattern([1 2]);','p=digitsPattern(1,65536);','p=lettersPattern(1.5);', "
                        "'p=whitespacePattern(Inf);','p=pattern(string({\"a\",\"b\"}));', "
                        "'x=contains(\"é\",lettersPattern);','x=count(\"abc\",optionalPattern(\"x\"));', "
                        "'x=extract(\"abc\",optionalPattern(\"x\"));','x=replace(\"abc\",digitsPattern,{\"X\",\"Y\"});', "
                        "'x=contains(\"a\",lettersPattern,\"IgnoreCase\",1);'}, "
                        "failed=false; try, eval(action{1}); catch e, failed=~isempty(strfind(e.message,':')); end; assert(failed); endfor; "
                        "s=string('abc'); s(1)=missing; failed=false; try, contains(s,lettersPattern); catch e, failed=~isempty(strfind(e.message,'missing')); end; assert(failed);")

    def test_textscan_valid_pairs_and_unchanged_forms(self):
        self.run_octave("for args={{'1 a 2 b','%f%s'},{'1,a;2,c;3','%f%s',1,'Delimiter',','}, "
                        "{'1,a,2,b','%f%s','Delimiter',',','CollectOutput',true}, "
                        "{'1,a,2,b','%f%s','Delimiter',',','ReturnOnError',false}, "
                        "{'1,,2,b','%f%s','Delimiter',','},{'NaN a Inf b','%f%s'}, "
                        "{'1 a\\n','%f%s'},{'1','%f%s'}, {'1 2','%f'}, "
                        "{'1,a;2,b','%f%s','Delimiter',',','EmptyValue',99}, "
                        "{'1 a','%f%s','Whitespace',' '},{'1 2 3','%f%*f%f'}}, "
                        "[a,p]=textscan(args{1}{:}); [b,q]=builtin('textscan',args{1}{:}); "
                        "assert(isequaln(a,b) && p==q); endfor;")

    def test_textscan_stops_at_failed_numeric_conversion(self):
        self.run_octave("[r,p]=textscan('1,a;2,b','%f%s','Delimiter',','); "
                        "assert(isequal(r,{1,{'a;2'}}) && p==6); "
                        "assert(isequal(textscan('1,a,2,b,bad,c','%f%s','Delimiter',','),{[1;2],{'a';'b'}})); "
                        "assert(isequal(textscan('bad,a','%f%s','Delimiter',','),{zeros(0,1),cell(0,1)})); "
                        "assert(isequal(textscan('1 a bad b','%f%s'),{1,{'a'}})); "
                        "for fn={@textscan,@(varargin)builtin('textscan',varargin{:})}, "
                        "failed=false; try, fn{1}('1,a;2,b','%f%s','Delimiter',',','ReturnOnError',false); "
                        "catch e, failed=~isempty(strfind(e.message,'Read error')); end; assert(failed); endfor;")

    def test_textscan_unchanged_overhead(self):
        result = self.run_octave("n=3000; args={'1 2','%f'}; deltas=[]; "
                                 "for trial=1:3, tic; for k=1:n, v=builtin('textscan',args{:}); endfor; b=toc; "
                                 "tic; for k=1:n, v=textscan(args{:}); endfor; w=toc; deltas(end+1)=(w-b)/n; endfor; "
                                 "overhead=median(deltas); fprintf('textscan unchanged %.3f us\\n',overhead*1e6); assert(overhead<150e-6);")
        print(result.stdout.strip())
