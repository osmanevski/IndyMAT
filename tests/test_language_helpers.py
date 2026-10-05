import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class LanguageHelperTests(unittest.TestCase):
    def run_octave(self, body):
        command = ("warning('off','Octave:shadowed-function'); addpath('%s'); "
                   "addpath('%s'); %s") % (ROOT / 'octave' / 'compat',
                                          ROOT / '.packages' / 'datatypes-1.5.0', body)
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file', '--eval', command],
                                cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_round_one_argument_is_builtin_equivalent(self):
        self.run_octave("for x={[-2.5 1.5 Inf NaN],single([1.2 -2.5]),int64([1 -2]),complex(1.5,-2.5),sparse(eye(2)),zeros(0,3)}, assert(isequaln(round(x{1}),builtin('round',x{1}))); endfor")

    def test_round_duration_dispatch(self):
        self.run_octave("d=duration(0,0,61.7); assert(seconds(round(d))==62); assert(seconds(round(d,'minutes'))==60);")

    def test_round_uncertain_ties_and_extremes_fail_explicitly(self):
        self.run_octave("for args={{2.675,2},{1.005,2},{single(1.005),2},{1,301},{1e-310,2,'significant'},{uint64(2^63),-2}}, failed=false; try, round(args{1}{:}); catch e, failed=~isempty(strfind(e.message,'round:')); end; assert(failed); endfor")

    def test_round_validation(self):
        self.run_octave("for args={{1,[1 2]},{1,NaN},{1,1+1i},{1,2,'other'},{1,0,'significant'},{'x',2}}, failed=false; try, round(args{1}{:}); catch, failed=true; end; assert(failed); endfor")

    def test_native_exception_contract_unchanged(self):
        self.run_octave("try, error('Probe:id','message'); catch e, assert(isstruct(e) && ~isa(e,'MException')); assert(isequal(fieldnames(e),{'message';'identifier';'stack'})); try, rethrow(e); catch f, assert(isequal(f,e)); end; try, error(e); catch f, assert(strcmp(f.identifier,e.identifier) && strcmp(f.message,e.message)); end; end;")

    def test_report_native_stack_and_validation(self):
        self.run_octave("e=struct('message','hello','identifier','Probe:id','stack',struct('name','caller','file','caller.m','line',7,'column',1)); assert(strcmp(getReport(e,'basic'),'hello')); assert(~isempty(strfind(getReport(e),'caller.m, line 7'))); for args={{e,'bad'},{e,'basic','hyperlinks','on'},{42}}, failed=false; try, getReport(args{1}{:}); catch, failed=true; end; assert(failed); endfor")

    def test_dictionary_native_scalar_indexing_and_transactional_writes(self):
        self.run_octave("d=dictionary([1 2],[10 20]); assert(numel(d)==1 && isequal(size(d),[1 1])); assert(d.numEntries()==2); assert(~isempty(evalc('disp(d)'))); for action={'d([3 4])=[30;40];','d(1)(1)=7;','x=d{1};','x=d(end);','d(1,2)=0;','d.key_data={};','d(3)=single(1);'}, failed=false; try, eval(action{1}); catch e, failed=true; end; assert(failed && numEntries(d)==2 && d(1)==10); endfor")

    def test_dictionary_shape_types_and_empty_configured(self):
        self.run_octave("d=configureDictionary('logical','logical'); assert(isequal(size(keys(d)),[0 1])); d(false)=true; assert(islogical(d(false))); d=dictionary(uint64([1 2]),single([3 4])); assert(isa(d(uint64(1)),'single')); assert(isequal(size(d(uint64([1;2]))),[2 1])); d=dictionary(string({'a','b'}),[1 2]); assert(isa(keys(d),'string')); assert(isequal(cellstr(keys(d)),{'a';'b'}));")

    def test_dictionary_explicit_unsupported_corners(self):
        self.run_octave("d=dictionary(1,10); for action={'x=entries(d);','d=remove(d,2);','x=d(int32(1));','x=lookup(d,2,\"FallbackValue\",[1 2]);','d=insert(d,2,20,\"Overwrite\",false);','d=dictionary(NaN,1);','d=dictionary(1+1i,1);','x=types(dictionary());','x=keys(dictionary());','x=dictionary(1,struct());'}, failed=false; try, eval(action{1}); catch e, failed=~isempty(strfind(e.message,'dictionary:')); end; assert(failed); endfor; try, x=d(2); catch e, assert(strcmp(e.identifier,'MATLAB:dictionary:ScalarKeyNotFound') && strcmp(e.message,'Key not found.')); end; try, x=d([1 2]); catch e, assert(strcmp(e.identifier,'MATLAB:dictionary:KeyNotFound') && ~isempty(strfind(e.message,'Element 2'))); end;")

    def test_dictionary_numeric_keys_do_not_round_uint64(self):
        self.run_octave("a=bitshift(uint64(1),63); b=a+uint64(1); d=dictionary([a b],[1 2]); assert(numEntries(d)==2 && d(a)==1 && d(b)==2);")

    def test_timeit_really_calls_function_and_propagates_errors(self):
        self.run_octave("global calls; calls=0; function counted(), global calls; calls=calls+1; pause(.002); end; t=timeit(@counted); assert(calls>=9 && t>=.001); assert(timeit(@()pause(.002))>=.001); function [a,b]=pair(), a=1;b=2;end; t=timeit(@pair,2); assert(t>0); failed=false; try, timeit(@()error('Probe:timed','expected')); catch e, failed=strcmp(e.identifier,'Probe:timed'); end; assert(failed);")

    def test_timeit_rejects_bad_arguments(self):
        self.run_octave("for args={{1},{@()0,-1},{@()0,1.5},{@()0,65},{@()0,Inf},{@(x)x}}, failed=false; try, timeit(args{1}{:}); catch e, failed=~isempty(strfind(e.message,'timeit:')); end; assert(failed); endfor")

    def test_dictionary_colon_and_arrays_are_explicit(self):
        self.run_octave("d=dictionary(string(':'),7); assert(d(string(':'))==7); for action={'v=d(:);','d(:)=1;','v=[d d];','v=[d;d];','v=repmat(d,2,1);','v=reshape(d,1,1);'}, failed=false; try, eval(action{1}); catch e, failed=~isempty(strfind(e.message,'dictionary')); end; assert(failed); endfor")
