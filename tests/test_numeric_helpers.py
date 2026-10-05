import json
import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class NumericHelperTests(unittest.TestCase):
    def run_octave(self, body, timeout=90):
        command = "addpath('%s'); %s" % (str(ROOT / 'octave' / 'compat'), body)
        result = subprocess.run([OCTAVE, '--no-init-file', '--no-site-file', '--quiet',
                                 '--eval', command], cwd=ROOT, text=True,
                                capture_output=True, timeout=timeout)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_native_structure_fields_and_interpolation_orders(self):
        # Exact mesh values isolate interpolation from ODE integration error.
        # Polynomial remainder: max |f^(p+1)| prod|q-x_i|/(p+1)!.
        self.run_octave("""
        for pair={{'ode45',6},{'ode23',4},{'ode15s',6}}
          item=pair{1}; n=item{2}; errors=[];
          for h=[0.1 0.05]
            x=0:h:1; s=struct('x',x,'y',exp(x),'solver',item{1}); q=x(1:end-1)+h/2;
            v=deval(s,q); errors(end+1)=max(abs(v-exp(q)));
            assert(errors(end)<=exp(1)*((n-1)*h)^n/factorial(n));
          endfor
          assert(errors(1)/errors(2)>2^(n-1));
        endfor
        for fun={@ode45,@ode23,@ode15s}
          s=fun{1}(@(t,y)-y,[0 1],1);
          assert(isequal(sort(fieldnames(s)),sort({'x';'y';'solver'})));
        endfor
        """)

    def test_deval_alternate_order_mesh_values_and_validation(self):
        self.run_octave("""
        s=ode45(@(t,y)-y,[0 1],1); assert(norm(deval(s,s.x)-s.y,inf)<1e-12);
        assert(isequal(deval(s,[0.2 0.7]),deval([0.2 0.7],s)));
        for q={-0.1,1.1,NaN,[0 1;0 1]}
          failed=false; try, deval(s,q{1}); catch e, failed=strncmp(e.message,'deval:',6); end_try_catch; assert(failed);
        endfor
        assert(isequal(size(deval(s,[])),[1 0]));
        """)

    def test_deval_ode15s_smooth_problem(self):
        # Deliberately nonstiff: no claim that mesh-only interpolation restores
        # IDA's order/DAE dense extension or resolves stiff startup layers.
        self.run_octave("""
        s=ode15s(@(t,y)-y,[0 1],1,odeset('RelTol',1e-8,'AbsTol',1e-10,'MaxStep',0.025));
        q=linspace(0,1,41); assert(max(abs(deval(s,q)-exp(-q)))<2e-6);
        """)

    def test_adams_accuracy_table_and_rhs_evaluations(self):
        output = self.run_octave("""
        function v=mf_count(t,y,f)
          global mf_calls; mf_calls+=1; v=f(t,y);
        endfunction
        global mf_calls;
        fs={@(t,y)-y,@(t,y)[y(2);-y(1)],@(t,y)y.*(1-y),@(t,y)[-y(1);y(1)-2*y(2)]};
        spans=[5 20*pi 8 4]; ys={1,[0;1],0.2,[1;0]};
        exact={@(t)exp(-t),@(t)[sin(t);cos(t)],@(t)1./(1+4*exp(-t)),@(t)[exp(-t);exp(-t)-exp(-2*t)]};
        table=[];
        for k=1:4
          for tol=[1e-3 1e-6 1e-9]
            opts=odeset('RelTol',tol,'AbsTol',tol/100); mf_calls=0;
            s=ode113(@(t,y)mf_count(t,y,fs{k}),[0 spans(k)],ys{k},opts);
            e=s.y-exact{k}(s.x); err=max(abs(e(:))); calls=mf_calls;
            assert(calls==s.__mf_stats__.nfevals);
            % Order-four global error at locally controlled fifth-order steps
            % scales as tol^(4/5), with accumulated error proportional to time.
            assert(err < 2*spans(k)*tol^(4/5));
            mf_calls=0;
            [t,y]=ode45(@(t,y)mf_count(t,y,fs{k}),[0 spans(k)],ys{k},opts);
            e=y.'-exact{k}(t.');
            table(end+1,:)=[k tol err calls max(abs(e(:))) mf_calls];
          endfor
        endfor
        disp(['NUMERIC_TABLE=' jsonencode(table)]);
        """)
        table = json.loads(next(line.split('=', 1)[1] for line in output.splitlines()
                                if line.startswith('NUMERIC_TABLE=')))
        for k in range(4):
            rows = table[k*3:(k+1)*3]
            self.assertLess(rows[1][2], rows[0][2] / 20)
            self.assertLess(rows[2][2], rows[1][2] / 20)
        print('\nODE accuracy: problem, tol, ode113 error/RHS, ode45 error/RHS')
        for row in table:
            print(row)

    def test_adams_coefficients_and_fixed_order_convergence(self):
        self.run_octave("""
        [w,c]=__mf_sayisal_adams_weights__([0 -1 -2 -3]);
        assert(norm(w-[55 -59 37 -9]/24,inf)<1e-13 && abs(c-251/720)<1e-13);
        [w,c]=__mf_sayisal_adams_weights__([1 0 -1 -2]);
        assert(norm(w-[9 19 -5 1]/24,inf)<1e-13 && abs(c+19/720)<1e-13);
        errs=[];
        for h=[0.05 0.025]
          s=ode113(@(t,y)-y,[0 1],1,odeset('RelTol',0.1,'AbsTol',1e-2,'InitialStep',h,'MaxStep',h));
          errs(end+1)=abs(s.y(end)-exp(-1));
        endfor
        assert(errs(1)/errs(2)>12);
        """)

    def test_adams_explicit_option_and_input_errors(self):
        self.run_octave("""
        for opts={odeset('Events',@(t,y)deal(y,1,1)),odeset('OutputFcn',@(t,y,flag)0), ...
                  odeset('Mass',1),odeset('NormControl','on'),odeset('NonNegative',1), ...
                  odeset('AbsTol',0),odeset('MaxStep',-1)}
          failed=false; try, s=ode113(@(t,y)-y,[0 1],1,opts{1}); catch e, failed=strncmp(e.message,'ode113:',7); end_try_catch; assert(failed);
        endfor
        failed=false; try, s=ode113(@(t,y)[y;y],[0 1],1); catch e, failed=strncmp(e.message,'ode113:',7); end_try_catch; assert(failed);
        """)

    def test_makima_plateaus_orientation_and_validation(self):
        self.run_octave("""
        q=linspace(0,6,301); y=[-1 -1 -1 0 1 1 1]; v=makima(0:6,y,q);
        assert(all(v>=-1-1e-14 & v<=1+1e-14));
        assert(norm(makima(6:-1:0,fliplr(y),q)-v,inf)<1e-13);
        assert(isequal(size(makima(0:6,y,q.')),size(q.')));
        for args={{[0 0],[1 2]},{[0 1],[1 NaN]},{[0 1],[1 2 3]},{[0 2 1],[1 2 3]}}
          failed=false; try, makima(args{1}{:}); catch e, failed=strncmp(e.message,'makima:',7); end_try_catch; assert(failed);
        endfor
        """)

    def test_cod_residual_and_null_space_minimum_norm(self):
        self.run_octave("""
        rand('state',7); randn('state',7);
        for dims={[8 5 3],[4 9 3],[7 5 5]}
          d=dims{1}; A=randn(d(1),d(3))*randn(d(3),d(2)); B=randn(d(1),3);
          for c=[false true]
            if c, AA=(1+2i)*A; BB=(2-1i)*B; else, AA=A; BB=B; endif
            X=lsqminnorm(AA,BB,1e-10); reference=pinv(AA,1e-10)*BB;
            assert(norm(X-reference,'fro') < 1e-10*max(1,norm(reference,'fro')));
            assert(norm(AA'*(AA*X-BB),'fro') < 1e-10*norm(AA,'fro')*norm(BB,'fro'));
            assert(norm(null(AA)'*X,'fro') < 1e-10*max(1,norm(X,'fro')));
          endfor
        endfor
        """)

    def test_cod_default_threshold_warning_and_sparse_rejection(self):
        self.run_octave("""
        % Default follows pivoted QR, not norm(A) or the SVD spectrum.
        A=diag([1 eps]); assert(isequal(lsqminnorm(A,[1;1]),[1;0]));
        assert(norm(lsqminnorm(diag([1 4*eps]),[1;4*eps])-[1;1])<1e-12);
        lastwarn(''); lsqminnorm(zeros(2),ones(2,1),'warn'); [msg,id]=lastwarn();
        assert(strcmp(id,'MATLAB:rankDeficientMatrix'));
        lastwarn(''); lsqminnorm(zeros(2),ones(2,1),'nowarn'); [msg,id]=lastwarn(); assert(isempty(msg));
        for args={{sparse(eye(2)),ones(2,1)},{eye(2),sparse(ones(2,1))}, ...
                  {eye(2),ones(2,1),-1},{eye(2),ones(2,1),'RegularizationFactor',1}}
          failed=false; try, lsqminnorm(args{1}{:}); catch e, failed=strncmp(e.message,'lsqminnorm:',11); end_try_catch; assert(failed);
        endfor
        """)

    def test_histcounts2_empty_percentage_and_explicit_errors(self):
        self.run_octave("""
        N=histcounts2([],[],[0 1],[0 1]); assert(isequal(N,0));
        N=histcounts2([],[],[0 1],[0 1],'Normalization','probability'); assert(isnan(N));
        N=histcounts2([0.5 Inf NaN],[0.5 0.5 0.5],[0 1],[0 1],'Normalization','percentage');
        assert(abs(N-100/3)<1e-12);
        for args={{[1 2],[1 2]},{[1 2],[1 2],2}, ...
                  {[1 2],[1 2],[0 1 1],[0 2]}, ...
                  {[1 2],[1 2],[0 Inf],[0 2]}, ...
                  {[1 2],[1;2],[0 3],[0 3]}, ...
                  {[1 2],[1 2],[0 3],[0 3],'BinMethod','auto'}}
          failed=false; try, histcounts2(args{1}{:}); catch e, failed=strncmp(e.message,'histcounts2:',12); end_try_catch; assert(failed);
        endfor
        """)

    def test_smoothing_gaussian_even_nan_flags_types_and_errors(self):
        self.run_octave("""
        g=exp(-1/(2*(2/5)^2)); v=smoothdata([0 1 0],'gaussian',2);
        assert(norm(v-[0 1/(1+g) g/(1+g)],inf)<1e-12);
        v=smoothdata([NaN 2 NaN],'gaussian',3); assert(isequal(v,[2 2 2]));
        v=smoothdata([1 NaN 3],'movmean',3,'includenan'); assert(all(isnan(v)));
        v=smoothdata([1 NaN 3],'movmedian',3,'omitmissing'); assert(isequal(v,[1 2 3]));
        v=smoothdata(uint8([1 2 3]),'movmean',3); assert(isa(v,'double'));
        a=sparse([1 0 3]); v=smoothdata(a,'movmean',3); assert(issparse(v) && norm(full(v)-[0.5 4/3 1.5],inf)<1e-12);
        assert(isequal(smoothdata(1:4,7,'movmean',3),1:4));
        for args={{1:5},{1:5,2},{1:5,'sgolay',5},{1:5,'gaussian',[1 2]}, ...
                  {1:5,'movmean',1.5},{1:5,'movmean',3,'SamplePoints',1:5}, ...
                  {[1 Inf 2],'movmean',3}}
          failed=false; try, smoothdata(args{1}{:}); catch e, failed=strncmp(e.message,'smoothdata:',11); end_try_catch; assert(failed);
        endfor
        """)
