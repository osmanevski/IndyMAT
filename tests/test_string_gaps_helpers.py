"""String API contracts and native pass-through, measured with octave-cli.

The yk-* probe outcomes are recorded by the orchestrator against MATLAB
R2025b; these tests guard the corresponding implemented subsets locally.
"""
import shutil
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts import compat, fark
from backend.source_adapter import STRING_AWARE

OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class StringGapHelperTests(unittest.TestCase):
    def run_octave(self, body):
        done = subprocess.run([OCTAVE, '--quiet', '--no-init-file', '--no-site-file',
                               '--no-history', '--eval', compat.setup() + body],
                              cwd=ROOT, capture_output=True, text=True, timeout=40)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        return done.stdout

    def test_char_layout_empty_and_missing(self):
        self.run_octave("s=string({'a','bb'}); c=char(s); assert(isequal(size(c),[1 2 2])); assert(isequal(double(c(:)'),[97 32 98 98])); "
                        "assert(isequal(char(string({'a';'bb'})),['a ';'bb'])); "
                        "assert(isequal(size(char(string({'a','bb';'ccc','d'}))),[2 3 2])); "
                        "assert(isequal(size(char(string(''))),[0 0])); "
                        "assert(isequal(size(char(string({'',''}))),[1 0 2])); "
                        "assert(isequal(size(char(strings(0,2))),[0 0 2])); "
                        "assert(isequal(size(char(string(missing))),[0 0]));")

    def test_string_constructor_nd_char_roundtrip_and_cellstr_empty(self):
        self.run_octave("s=string({'a','bb';'ccc','d'}); t=string(char(s)); "
                        "assert(isequal(size(t),[2 2])); assert(isequal(cellstr(t),{'a  ','bb ';'ccc','d  '})); "
                        "s=reshape(string({'ab','cd','ef','gh'}),2,1,2); assert(isequal(string(char(s)),s)); "
                        "s=erase(string('a'),'a'); c=cellstr(s); assert(isequal(size(c{1}),[0 0]));")

    def test_numeric_conversion_is_elementwise_and_never_evaluates(self):
        self.run_octave("s=string({'1e3','-2.5e-2','1,2','NaN','Inf','-Inf','','  ','1+2','2+3i'}); "
                        "expected=[1000 -.025 12 NaN Inf -Inf NaN NaN NaN 2+3i]; "
                        "assert(isequaln(str2double(s),expected)); assert(isequaln(double(s),expected)); "
                        "assert(isnan(double(string('system(1)')))); "
                        "assert(isnan(double(string(missing)))); "
                        "assert(isequal(size(str2double(strings(0,3))),[0 3])); "
                        "assert(isequal(size(double(reshape(s,2,1,5))),[2 1 5])); "
                        "assert(isequal(bin2dec(string({'1','10';'11','100'})),[1 2;3 4])); "
                        "assert(isequal(hex2dec(string({'A','FF'})),[10 255]));")

    def test_measured_string_conversion_edge_contracts(self):
        self.run_octave("assert(isequal([strlength(string('é')),strlength(string('😀')),strlength(string('a😀'))],[1 2 3])); "
                        "r=string('x')+[1 2]; assert(isequal(cellstr(r),{'x1','x2'})); "
                        "s=[string('abc'),string(missing)]; r=strcat(s,' X '); assert(ismissing(r(2))); "
                        "failed=false; try, char(s); catch, failed=true; end; assert(failed); "
                        "failed=false; try, compose('<%s>',s); catch, failed=true; end; assert(failed);")

    def test_num2str_rejects_string_objects_but_preserves_numeric_path(self):
        self.run_octave("assert(strcmp(num2str(12.5),builtin('num2str',12.5))); assert(strcmp(num2str(string('text')),'text')); "
                        "failed=false; try, num2str([string('a'),string('b')]); catch e, failed=strcmp(e.identifier,'MATLAB:num2str:nonNumericInput'); end; assert(failed);")

    def test_split_empty_payload_and_shapes(self):
        self.run_octave("s=split(string('a,b,,c'),string(',')); assert(isequal(size(s),[4 1])); "
                        "assert(isequal(size(char(s(3))),[0 0])); "
                        "s=splitlines(string(sprintf('a\\n'))); assert(isequal(size(char(s(2))),[0 0])); "
                        "[s,m]=strsplit(string('a,,b'),string(','),'CollapseDelimiters',false); "
                        "assert(isequal(cellstr(s),{'a','','b'})); assert(isequal(size(m),[1 2])); "
                        "[s,m]=strsplit(string(''),','); assert(isequal(size(s),[1 1])); assert(isempty(m)); "
                        "assert(strcmp(char(strjoin(string({'a';'b'}),string('-'))),'a-b')); "
                        "assert(strcmp(strjoin({'a','b'},string('-')),'a-b')); "
                        "failed=false; try, strjoin(string(missing),struct()); catch e, failed=!isempty(strfind(e.message,'strjoin:')); end; assert(failed); "
                        "assert(isa(strsplit('a,b',string(',')),'cell'));")

    def test_extractbetween_first_singleton_dimension(self):
        self.run_octave("s=extractBetween(string({'<a>','<b>'}),'<','>'); assert(isequal(cellstr(s),{'a','b'})); "
                        "s=extractBetween(string({'<a><b>','<c><d>'}),'<','>'); "
                        "assert(isequal(size(s),[1 2 2])); assert(isequal(cellstr(s(:)),{'a';'c';'b';'d'})); "
                        "s=extractBetween(string({'<a><b>';'<c><d>'}),'<','>'); "
                        "assert(isequal(size(s),[2 1 2])); assert(isequal(cellstr(s(:)),{'a';'c';'b';'d'})); "
                        "s=extractBetween(string({'abc','xyz'}),2,3); assert(isequal(cellstr(s),{'bc','yz'})); "
                        "s=extractBetween(string('<a>'),'<','>','Boundaries','inclusive'); assert(strcmp(char(s),'<a>')); "
                        "assert(isequal(extractBetween('abc',string('a'),string('c')),{'b'}));")

    def test_mixed_dispatch_preserves_character_source(self):
        self.run_octave("assert(startsWith('abc',string('A'),'IgnoreCase',true)); "
                        "assert(endsWith({'abc','ABC'},string('c'),'IgnoreCase',true)); "
                        "assert(strcmp(insertBefore('abc',string('b'),string('X')),'aXbc')); "
                        "assert(strcmp(insertAfter('abc',string('b'),string('X')),'abXc')); "
                        "assert(strcmp(extractBefore('abc',string('b')),'a')); "
                        "assert(strcmp(extractAfter('abc',string('b')),'c')); "
                        "assert(strcmp(replace('abc',string('b'),string('X')),'aXc')); "
                        "assert(strcmp(erase('abc',string('b')),'ac')); "
                        "assert(strcmp(char(strcat(' a ',string(' b '))),' a b ')); "
                        "assert(strcmp(char(append(' a ',string(' b '))),' a  b '));")

    def test_compose_and_sprintf_supported_subsets(self):
        self.run_octave("assert(isequal(compose('%s:%.1f',string({'a';'b'}),[1.25;2.75]),{'a:1.2';'b:2.8'})); "
                        "s=compose(string('%s:%.1f'),string({'a';'b'}),[1.25;2.75]); assert(isa(s,'string')); "
                        "failed=false; try, compose('%s:%.1f',string({'a','b'}),[1.25 2.75]); catch e, failed=!isempty(strfind(e.message,'enough conversion')); end; assert(failed); "
                        "assert(strcmp(char(sprintf(string('%s:%d'),string('x'),2)),'x:2')); "
                        "assert(ischar(sprintf('%s:%d',string('x'),2))); "
                        "assert(strcmp(sprintf('<%s>',string({'abc','xyz'})),'<abc><xyz>')); "
                        "assert(strcmp(char(sprintf(string('a\\nb'))),sprintf('a\\nb')));")

    def test_regexp_container_types_ascii_and_missing_policy(self):
        self.run_octave("s=string('ab12'); assert(isequal(regexp(s,'[0-9]+'),3)); "
                        "assert(isequal(cellstr(regexp(s,'[0-9]+','match')),{'12'})); "
                        "[a,b,t,m]=regexp(s,'([a-z]+)([0-9]+)'); "
                        "assert(a==1 && b==4 && isequal(t,{[1 2;3 4]})); assert(isa(m,'string')); "
                        "t=regexp(s,'([a-z]+)([0-9]+)','tokens'); assert(isa(t{1},'string')); "
                        "m=regexpi(string({'AB12','CD34'}),'[a-z]+','match'); assert(iscell(m) && isa(m{1},'string')); "
                        "assert(iscell(regexp('ab12',string('[0-9]+'),'match'))); "
                        "s=regexprep(string({'ab12','cd34'}),string('[0-9]+'),string('X')); assert(isequal(cellstr(s),{'abX','cdX'})); "
                        "s=regexprep(string({'a',missing}),'a','X'); assert(ismissing(s(2))); "
                        "for s={string('é')}, failed=false; try, regexp(s{1},'x'); catch e, failed=!isempty(strfind(e.message,'regexp:')); end; assert(failed); endfor; "
                        "m=regexp([string('abc'),string(missing)],'[a-z]+','match'); assert(iscell(m) && isequal(size(m),[1 2]) && isa(m{1},'string') && isequal(size(m{2}),[0 0])); "
                        "m=regexp(string(''),'[a-z]+','match'); assert(isequal(size(m),[0 0]));")

    def test_predicates_converters_comparisons_and_set_operations(self):
        self.run_octave("assert(isStringScalar(string('')) && isStringScalar(string(missing))); "
                        "assert(!isStringScalar(strings(0,1)) && !isStringScalar('abc')); "
                        "s=string({'a','b'}); assert(isstring(s) && isequal(size(s),[1 2])); "
                        "assert(isequal(convertStringsToChars(s),{'a','b'})); assert(isa(convertCharsToStrings({'a','b'}),'string')); "
                        "assert(isequal(strcmpi(s,'A'),[true false])); assert(isequal(strncmp(s,'a',1),[true false])); "
                        "assert(isequal(s==string('a'),[true false])); assert(isequal(s>string('a'),[false true])); "
                        "s=sort(string({'b','a'}),string('descend')); assert(isequal(cellstr(s),{'b','a'})); "
                        "s=unique(string({'b','a','b'}),string('stable')); assert(isequal(cellstr(s),{'b','a'})); "
                        "assert(isequal(ismember(string({'b','c'}),string({'a','b'})),[true false]));")

    def test_sort_native_forms_pass_through_values_indices_and_errors(self):
        self.run_octave("for x={[],zeros(0,3),[3 NaN 1],single([3 1 2]),int64([3 1 2]),complex([3 1],[1 4]),sparse([3 1]),['b';'a']}, "
                        "for args={{},{'descend'},{1,'ascend'},{2,'descend'}}, "
                        "[a,i]=sort(x{1},args{1}{:}); [b,j]=builtin('sort',x{1},args{1}{:}); assert(isequaln(a,b) && isequal(i,j)); endfor; endfor; "
                        "for args={{1,'bad'},{'bad'},{0},{1.5}}, "
                        "try, sort([1 2],args{1}{:}); error('test:expected'); catch a, end; "
                        "try, builtin('sort',[1 2],args{1}{:}); error('test:expected'); catch b, end; assert(strcmp(a.message,b.message)); endfor")

    def test_sort_options_stable_indices_nd_and_integer_precision(self):
        self.run_octave("[a,i]=sort([3 NaN 1 NaN],'MissingPlacement','first'); assert(isequaln(a,[NaN NaN 1 3]) && isequal(i,[2 4 3 1])); "
                        "[a,i]=sort([2 1 2 1],'ascend','ComparisonMethod','real'); assert(isequal(i,[2 4 1 3])); "
                        "[a,i]=sort([1+2i 1-2i -3+1i],'ComparisonMethod','real'); assert(isequal(i,[3 2 1])); "
                        "[a,i]=sort([-2 2 -1 1],'ComparisonMethod','abs'); assert(isequal(i,[4 3 2 1])); "
                        "A=reshape([NaN 2 1 3 NaN 4 2 1],2,2,2); [B,I]=sort(A,2,'descend','MissingPlacement','last'); assert(isequal(size(B),size(A)) && isequal(size(I),size(A))); "
                        "[a,i]=sort(uint64([2^63 2^63+2048]),'ComparisonMethod','real'); assert(isequal(i,[1 2]) && isa(a,'uint64')); "
                        "assert(isequal(sort([3 1],string('MissingPlacement'),string('last')),[1 3]));")

    def test_sort_unchanged_path_overhead_bound(self):
        output = self.run_octave("x=[3 1 2]; N=10000; sort(x); "
                                 "tic; for k=1:N, y=builtin('sort',x); endfor; native=toc; "
                                 "tic; for k=1:N, y=sort(x); endfor; shadow=toc; "
                                 "extra=(shadow-native)/N; fprintf('@@OVERHEAD %.3f us/call\\n',extra*1e6); assert(extra<200e-6);")
        self.assertIn('@@OVERHEAD', output)

    def test_every_added_table_reference_exists_and_object_probes_complete(self):
        probes = {p['id']: p for p in fark.load(fark.BASE/'yoklamalar')}
        ids = {i for values in STRING_AWARE.values() for i in values if i.startswith('yk-')}
        self.assertTrue(ids)
        profile = fark.adapted_profile()
        generated, adaptations = fark.adapt_probes([probes[i] for i in sorted(ids)], profile)
        for adaptation in adaptations:
            self.assertIn(adaptation.status, ('adapted', 'unchanged'), adaptation.diagnostics)
        results = fark.run(generated, 'octave', 40)
        for identifier, result in zip(sorted(ids), results):
            self.assertNotEqual(result, 'CALISMADI', identifier)
            self.assertFalse(result.startswith('HATA|'), (identifier, result))


if __name__ == '__main__':
    unittest.main()
