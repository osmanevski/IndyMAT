import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class MathematicsHelperTests(unittest.TestCase):
    def run_octave(self, body):
        # Capture originals before compat is on PATH, including builtin handles.
        command = ("warning('off','Octave:shadowed-function'); native=struct(); "
                   "for name={'idivide','null','lu','polyfit','interp1','interp2',"
                   "'corrcoef','ifft'}, native.(name{1})=str2func(name{1}); end; "
                   "addpath('%s'); %s") % (ROOT / 'octave' / 'compat', body)
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file', '--no-site-file',
                                 '--eval', command], cwd=ROOT, text=True,
                                capture_output=True, timeout=90)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_idivide_exact_signed_and_unsigned_truncation(self):
        self.run_octave("""
            a=int8(-128:127);
            for b=[-128:-1 1:127]
              assert(isequal(idivide(a,int8(b)),int8(fix(double(a)/b))));
            end
            a=intmin('int64'); b=int64(3);
            q=-int64((bitshift(uint64(1),63)-uint64(2))./uint64(3));
            assert(idivide(a,b)==q);
            a=intmax('uint64'); assert(idivide(a,uint64(3))*uint64(3)==a);
            assert(idivide(intmin('int64'),int64(-1))==intmax('int64'));
            assert(isequal(size(idivide(int16([-7;7]),int16([3 -3]))),[2 2]));
            assert(isequal(idivide(int8([-7 0 7]),int8(0)),int8([-128 0 127])));
            for mode={'floor','ceil','round'}
              assert(isequal(idivide(int16([-7 7]),int16([3 -3]),mode{1}),native.idivide(int16([-7 7]),int16([3 -3]),mode{1})));
            end
            assert(isequal(idivide(int16(7),3),native.idivide(int16(7),3)));
        """)

    def test_null_rational_and_orthonormal_pass_through(self):
        self.run_octave("""
            for a={[1 2 3;2 4 6],[0 1 2;0 0 0],zeros(0,4),zeros(4,0),eye(3),[1 1i 2]}
              z=null(a{1},'r'); assert(norm(a{1}*z,'fro')<1e-12);
              assert(columns(z)==columns(a{1})-rank(a{1}));
              assert(isequaln(null(a{1}),native.null(a{1})));
              assert(isequaln(null(a{1},1e-6),native.null(a{1},1e-6)));
            end
            assert(isequal(null([1 2 3;2 4 6],'r'),[-2 -3;1 0;0 1]));
        """)

    def test_lu_and_polyfit_unchanged_forms_and_shapes(self):
        self.run_octave("""
            a=[0 1;2 3]; [l,u,p]=lu(a,'vector');
            assert(isrow(p) && norm(a(p,:)-l*u)<1e-12);
            [l,u,p,q]=lu(sparse(a),'vector');
            assert(isrow(p) && isrow(q) && norm(a(p,q)-l*u)<1e-12);
            [l,u,p]=lu(a); [nl,nu,np]=native.lu(a);
            assert(isequal(l,nl) && isequal(u,nu) && isequal(p,np));
            x=1:6; y=x.^2; [p,s]=polyfit(x,y,2); [np,ns]=native.polyfit(x,y,2);
            assert(isequal(p,np) && isequal(s,ns));
            [p,s,mu]=polyfit(x,y,2); [np,ns,nmu]=native.polyfit(x,y,2);
            assert(isequal(p,np) && isequal(s,ns) && isequal(mu,nmu(:)));
        """)

    def test_interp1_makima_arrays_sites_pp_and_extrapolation(self):
        self.run_octave("""
            x=0:3; y=[0 1 1 2]; q=[.5 2.5];
            assert(norm(interp1(x,y,q,'makima')-[.615625 1.384375])<1e-12);
            assert(isequal(interp1(y,q+1,'makima'),interp1(x,y,q,'makima')));
            assert(isequal(interp1(fliplr(x),fliplr(y),q,'makima'),interp1(x,y,q,'makima')));
            assert(isequal(interp1(x([2 1 4 3]),y([2 1 4 3]),q,'makima'),interp1(x,y,q,'makima')));
            pp=interp1(x,y,'makima','pp'); assert(isequal(ppval(pp,q),interp1(x,y,q,'makima')));
            assert(isequal(interp1(x,y,[-1 4],'makima',-7),[-7 -7]));
            assert(isequal(interp1(x,y,[-1 4],'makima'),interp1(x,y,[-1 4],'makima','extrap')));
            v=interp1(x,[y;2*y].',q,'makima'); assert(isequal(size(v),[2 2]));
            assert(norm(v-[.615625 1.23125;1.384375 2.76875])<1e-12);
            assert(isnan(interp1(x,y,NaN,'makima')));
            for action={"interp1(x,single(y),q,'makima');","interp1(x,complex(y,1),q,'makima');","interp1(x,[0 1 NaN 2],q,'makima');"}
              failed=false; try, eval(action{1}); catch, failed=true; end; assert(failed);
            end
            for method={'linear','nearest','pchip','spline'}
              assert(isequaln(interp1(x,y,[-1 q 4],method{1}),native.interp1(x,y,[-1 q 4],method{1})));
            end
        """)

    def test_interp2_fill_and_explicit_extrapolation(self):
        self.run_octave("""
            z=[1 2;3 4]; v=interp2(z,[1.5 2.5],[1 1.5]);
            assert(v(1)==1.5 && isnan(v(2)) && !isna(v(2)));
            assert(isequal(interp2(z,[1.5 2.5],[1 1.5],'linear',-3),native.interp2(z,[1.5 2.5],[1 1.5],'linear',-3)));
            assert(isequaln(interp2(z,1.5,1.5),native.interp2(z,1.5,1.5)));
        """)

    def test_corrcoef_complete_keeps_variable_count(self):
        self.run_octave("""
            [r,p,l,h]=corrcoef([1 NaN 3;2 4 6],'Rows','complete');
            assert(isequal(size(r),[3 3]) && all(isnan([r(:);p(:);l(:);h(:)])));
            assert(isequal(size(corrcoef([NaN 1;2 NaN],'Rows','complete')),[2 2]));
            r=corrcoef([1 NaN 3],[2 4 6],'Rows','complete'); assert(isequal(size(r),[2 2]));
            assert(corrcoef([1 NaN 3],'Rows','complete')==1);
            for args={{[1 2;2 3;3 4]},{[1 2;2 3;3 4],'Rows','complete'},{[1 NaN;2 3;3 4],'Rows','pairwise'}}
              [r,p]=corrcoef(args{1}{:}); [nr,np]=native.corrcoef(args{1}{:});
              assert(isequaln(r,nr) && isequaln(p,np));
            end
        """)

    def test_ifft_symmetry_uses_half_spectrum_and_pass_through(self):
        self.run_octave("""
            for x={1:5,(1:4).',single([1 2 3]),reshape(1:12,3,4)}
              a=x{1}; assert(isequaln(ifft(a),native.ifft(a)));
              assert(norm(double(ifft(fft(a), 'symmetric')(:))-double(a(:)))<1e-5);
              assert(isequaln(ifft(a,8,2,'nonsymmetric'),native.ifft(a,8,2)));
            end
            x=[1+7i 2+3i 7+9i 99+5i];
            assert(isequal(ifft(x,'symmetric'),real(native.ifft([1 2+3i 7 2-3i]))));
            assert(isequal(ifft([1 2+3i 4+5i],5,'symmetric'),real(native.ifft([1 2+3i 4+5i 4-5i 2-3i]))));
            assert(isequal(size(ifft([1 2],5,2,'symmetric')),[1 5]));
            assert(isa(ifft(single([1 2]),'symmetric'),'single'));
        """)

    def test_unchanged_call_overhead(self):
        result = self.run_octave("""
            cases={{'idivide',{int16(7),int16(3),'round'}}, {'null',{[1 2 3;2 4 6]}},
                   {'lu',{[0 1;2 3]}}, {'polyfit',{1:5,(1:5).^2,2}},
                   {'interp1',{0:3,[0 1 1 2],.5}}, {'interp2',{[1 2;3 4],1.5,1.5,'linear',-3}},
                   {'corrcoef',{[1 2;2 3;3 4]}}, {'ifft',{[1 2 3 4]}}};
            cases=cases(:).'; n=2000;
            for entry=cases
              name=entry{1}{1}; args=entry{1}{2}; wrapped=str2func(name); base=native.(name);
              info=functions(wrapped); assert(!isempty(strfind(info.file,'octave/compat/')));
              wrapped(args{:}); base(args{:}); tw=[]; tb=[];
              for repeat=1:3
                tic; for k=1:n, unused=base(args{:}); end; tb(end+1)=toc/n;
                tic; for k=1:n, unused=wrapped(args{:}); end; tw(end+1)=toc/n;
              end
              overhead=(median(tw)-median(tb))*1e6;
              fprintf('OVERHEAD %s %.3f us (native %.3f, wrapper %.3f)\\n',name,overhead,median(tb)*1e6,median(tw)*1e6);
              assert(overhead<250);
            end
        """)
        print(result.stdout, end='')
