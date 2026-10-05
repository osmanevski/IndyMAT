import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class YdHelperTests(unittest.TestCase):
    def run_octave(self, body):
        command = "warning('off','Octave:shadowed-function'); addpath('%s'); %s" % (
            ROOT / 'octave' / 'compat', body)
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file', '--no-site-file',
                                 '--eval', command], cwd=ROOT, text=True,
                                capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_integer_division_all_int8_pairs_and_modes(self):
        self.run_octave("""
          [a,b]=ndgrid(int8(-128:127),int8([-128:-1 1:127]));
          for mode={'fix','floor','ceil'}
            expected=int8(feval(mode{1},double(a)./double(b)));
            assert(isequal(idivide(a,b,mode{1}),expected));
          endfor
          assert(isequal(idivide(a,b),int8(fix(double(a)./double(b)))));
          assert(isequal(idivide(int8([-1 0 1]),int8(0)),int8([-128 0 127])));
          assert(isequal(idivide(int8([-7;7]),int8([-3 3])),int8([2 -2;-2 2])));
        """)

    def test_integer_division_64_bit_precision_and_saturation(self):
        self.run_octave("""
          hi=bitshift(uint64(1),63);
          assert(idivide(hi+uint64(3),uint64(3))==(hi-uint64(2))/uint64(3)+uint64(1));
          assert(idivide(intmin('int64'),int64(1))==intmin('int64'));
          assert(idivide(intmin('int64'),int64(-1))==intmax('int64'));
          assert(idivide(intmin('int64'),int64(3))==-int64((hi-uint64(2))/uint64(3)));
          assert(idivide(intmin('int64'),int64(3),'floor')==-int64((hi-uint64(2))/uint64(3))-int64(1));
          assert(idivide(intmax('uint64'),uint64(2))==hi-uint64(1));
          assert(idivide(intmax('uint64'),uint64(2),'ceil')==hi);
        """)

    def test_bitget_classes_signed_shapes_and_positions(self):
        self.run_octave("""
          for cls={'double','single','int8','uint8','int16','uint16','int32','uint32','int64','uint64'}
            x=cast(10,cls{1}); y=bitget(x,1:8);
            assert(strcmp(class(y),cls{1}) && isequal(double(y),[0 1 0 1 0 0 0 0]));
          endfor
          assert(isequal(bitget(int8([-128 -1;0 1]),8),int8([1 1;0 0])));
          assert(bitget(intmin('int64'),64)==int64(1));
          assert(bitget(intmax('uint64'),64)==uint64(1));
          failed=false; try, bitget(uint8(1),9); catch, failed=true; end; assert(failed);
        """)

    def test_eye_prototype_storage_and_dimensions(self):
        self.run_octave("""
          for p={single(0),uint8(0),false,complex(0),sparse(0),sparse(complex(0)),sparse(false)}
            prototype=p{1}; y=eye([2 3],'like',prototype);
            assert(isequal(full(y),cast(builtin('eye',2,3),class(prototype))));
            assert(strcmp(class(y),class(prototype)));
            assert(isreal(y)==isreal(prototype) && issparse(y)==issparse(prototype));
          endfor
          assert(isequal(size(eye(0,3,'like',single(0))),[0 3]));
          large=eye(100000,100001,'like',sparse(0));
          assert(issparse(large) && nnz(large)==100000);
          failed=false; try, eye(2,3,4,'like',single(0)); catch, failed=true; end; assert(failed);
        """)

    def test_isfield_empty_and_unchanged_forms(self):
        self.run_octave("""
          for names={cell(0,0),cell(0,3),cell(2,0,4)}
            assert(isequal(isfield(struct('a',1),names{1}),false));
          endfor
          for s={struct('a',1),struct([]),42}, for names={'a',{'a','b'},[1 2],''}
            assert(isequal(isfield(s{1},names{1}),builtin('isfield',s{1},names{1})));
          endfor; endfor
        """)

    def test_linspace_single_grids_and_native_forms(self):
        self.run_octave("""
          assert(isequal(linspace(single(0),single(1),4),single([0 1/3 2/3 1])));
          for n=[0 1 2 7 100]
            e=single([realmax('single')/2 realmax('single')]);
            grid=linspace(e(1),e(2),n);
            assert(isa(grid,'single') && numel(grid)==n && all(isfinite(grid)));
            if n>0, assert(grid(end)==e(2)); endif
            if n>1, assert(grid(1)==e(1) && all(diff(grid)>=0)); endif
          endfor
          assert(isequal(size(linspace(single([0;1]),single([1;2]),4)),[2 4]));
          assert(isa(linspace(single(1i),single(2i),4),'single'));
          for args={{0,1},{0,1,4},{single(0),1,4},{0,single(1),4},{[0;1],[1;2],4}}
            assert(isequal(linspace(args{1}{:}),builtin('linspace',args{1}{:})));
          endfor
        """)

    def test_original_handles_and_passthrough_overhead(self):
        result = self.run_octave("""
          old=pwd(); p=path(); original=__mf_original_function__('idivide');
          assert(strcmp(pwd(),old) && strcmp(path(),p));
          assert(isequal(original,__mf_original_function__('idivide')));
          for args={{int8(5),int8(2),'round'},{int8(5),2},{5,int8(2)}}
            assert(isequal(idivide(args{1}{:}),original(args{1}{:})));
          endfor
          bg=__mf_original_function__('bitget');
          % Invalid unchanged bitget calls preserve the original diagnostic.
          try, bitget('x',1); catch wrapped, end;
          try, bg('x',1); catch native, end;
          assert(strcmp(wrapped.identifier,native.identifier) && strcmp(wrapped.message,native.message));
          wrapped={@()eye(2),@()isfield(struct('a',1),'a'),@()linspace(0,1,4),@()idivide(int8(5),int8(2),'round'),@()bitget('x',1)};
          native={@()builtin('eye',2),@()builtin('isfield',struct('a',1),'a'),@()builtin('linspace',0,1,4),@()original(int8(5),int8(2),'round'),@()bg('x',1)};
          names={'eye','isfield','linspace','idivide','bitget'};
          for j=1:numel(names)
            measurements=zeros(3,2);
            for repeat=1:3
              for side=1:2
                h=wrapped{j}; if side==2, h=native{j}; endif
                tic; for k=1:2000, try, h(); catch, end; endfor
                measurements(repeat,side)=toc/2000;
              endfor
            endfor
            times=median(measurements);
            overhead=times(1)-times(2);
            fprintf('OVERHEAD %s native=%.3f us wrapper=%.3f us extra=%.3f us\\n',names{j},times(2)*1e6,times(1)*1e6,overhead*1e6);
            assert(overhead<200e-6);
          endfor
        """)
        print(result.stdout, end='')


if __name__ == '__main__':
    unittest.main()
