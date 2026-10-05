import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class MathNumericsHelperTests(unittest.TestCase):
    def run_octave(self, body):
        command = ("warning('off','Octave:shadowed-function'); addpath('%s'); %s"
                   % (ROOT / 'octave' / 'compat', body))
        result = subprocess.run(
            [OCTAVE, '--quiet', '--no-init-file', '--no-site-file', '--eval', command],
            cwd=ROOT, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_simplex_initialization_preserves_shape_and_calls(self):
        self.run_octave("""
        global seen; seen={};
        function f=record_point(x)
          global seen; seen{end+1}=x; f=sum((x(:)-[2;-1]).^2);
        endfunction
        for start={[0 0],[0;0],[2 -1]}
          seen={}; x0=start{1}; [x,f,flag,out]=fminsearch(@record_point,x0);
          assert(isequal(seen{1},x0));
          next=x0; if x0(1)==0, next(1)=0.00025; else, next(1)*=1.05; end;
          assert(isequal(seen{2},next));
          next=x0; if x0(2)==0, next(2)=0.00025; else, next(2)*=1.05; end;
          assert(isequal(seen{3},next));
          assert(all(cellfun(@(v)isequal(size(v),size(x0)),seen)));
          assert(isequal(size(x),size(x0)) && norm(x(:)-[2;-1])<1e-3);
          assert(f<1e-7 && flag==1 && out.funcCount==numel(seen));
          assert(strcmp(out.algorithm,'Nelder-Mead simplex direct search'));
        endfor
        """)

    def test_nonquadratic_and_matrix_objectives(self):
        self.run_octave("""
        [x,f,flag]=fminsearch(@(v)100*(v(2)-v(1)^2)^2+(1-v(1))^2,[-1.2 1]);
        assert(norm(x-[1 1])<1e-3 && f<1e-7 && flag==1);
        target=[1 -2;3 -4];
        [x,f,flag]=fminsearch(@(v)sum((v(:)-target(:)).^2),target+0.1);
        assert(isequal(size(x),[2 2]) && norm(x-target,'fro')<1e-3 && f<1e-7 && flag==1);
        [x,f,flag]=fminsearch(@(v)(v+3)^2,1);
        assert(abs(x+3)<1e-3 && f<1e-7 && flag==1);
        """)

    def test_evaluation_budget_and_exception_propagation(self):
        self.run_octave("""
        global calls; calls=0;
        function f=unbounded(x), global calls; calls+=1; f=-x; endfunction
        text=evalc('[x,f,flag,out]=fminsearch(@unbounded,0);');
        assert(flag==0 && out.funcCount==calls && out.funcCount>=200 && out.funcCount<=202);
        assert(~isempty(strfind(text,'fminsearch: maximum number')));
        failed=false;
        try, fminsearch(@(x)error('Probe:objective','expected'),0);
        catch e, failed=strcmp(e.identifier,'Probe:objective'); end;
        assert(failed);
        """)

    def test_fminsearch_options_problem_and_defaults_delegate(self):
        self.run_octave("""
        native=__mf_original_function__('fminsearch');
        assert(isequal(fminsearch('defaults'),native('defaults')));
        assert(isequal(fminsearch(@(v)sum((v-[2 -1]).^2),[0 0]), ...
                       fminsearch(@(v)sum((v-[2 -1]).^2),[0 0],[])));
        opts=optimset('Display','off','MaxIter',3);
        fun=@(v)sum((v-[2 -1]).^2);
        [a,b,c,d]=fminsearch(fun,[0 0],opts);
        [aa,bb,cc,dd]=native(fun,[0 0],opts);
        assert(isequaln({a,b,c,d},{aa,bb,cc,dd}));
        p=struct('objective',fun,'x0',[0 0],'solver','fminsearch','options',opts);
        [a,b,c,d]=fminsearch(p); [aa,bb,cc,dd]=native(p);
        assert(isequaln({a,b,c,d},{aa,bb,cc,dd}));
        """)

    def test_rat_text_evaluates_and_pads_each_row(self):
        self.run_octave("""
        for x={[pi 1/3;sqrt(2) -2],[-0.75 0 2 1/7],single([0.1 -1.25])}
          data=x{1}; text=rat(data); tolerance=1e-6*norm(double(data(:)),1);
          assert(rows(text)==numel(data));
          for k=1:numel(data)
            assert(abs(eval(text(k,:))-data(k))<=tolerance);
          endfor
        endfor
        assert(strcmp(rat(1/3),'0 + 1/(3)'));
        assert(strcmp(rat(pi,1e-2),'3 + 1/(7)'));
        assert(strcmp(rat(2),'2') && strcmp(rat(0),'0'));
        assert(isequal(rat([]),''));
        assert(strcmp(rat(Inf),'Inf') && strcmp(rat(-Inf),'-Inf'));
        """)

    def test_rat_numerical_and_complex_forms_delegate(self):
        self.run_octave("""
        native=__mf_original_function__('rat');
        for args={{[pi 1/3;sqrt(2) -2]},{[0.5 -0.75],1e-3},{single([0.1 1.5])},{[Inf 0 -Inf]},{0.5+pi*i}}
          [n,d]=rat(args{1}{:}); [nn,dd]=native(args{1}{:});
          assert(isequaln(n,nn) && isequaln(d,dd));
        endfor
        for x={0.5+pi*i,[1+i -2i]}
          assert(isequal(rat(x{1}),native(x{1})));
        endfor
        """)

    def test_delegated_path_overhead(self):
        output = self.run_octave("""
        native_rat=__mf_original_function__('rat'); native_fmin=__mf_original_function__('fminsearch');
        [n,d]=rat(1/3); fminsearch('defaults'); loops=2000;
        native_time=zeros(1,5); shadow_time=zeros(1,5);
        for trial=1:5
          tic; for k=1:loops, [n,d]=native_rat(1/3); end; native_time(trial)=toc;
          tic; for k=1:loops, [n,d]=rat(1/3); end; shadow_time(trial)=toc;
        endfor
        rat_cost=(median(shadow_time)-median(native_time))/loops;
        for trial=1:5
          tic; for k=1:loops, x=native_fmin('defaults'); end; native_time(trial)=toc;
          tic; for k=1:loops, x=fminsearch('defaults'); end; shadow_time(trial)=toc;
        endfor
        fmin_cost=(median(shadow_time)-median(native_time))/loops;
        printf('OVERHEAD rat %.3f us; fminsearch %.3f us\\n',rat_cost*1e6,fmin_cost*1e6);
        assert(rat_cost<200e-6 && fmin_cost<200e-6);
        """)
        print(output.strip())
