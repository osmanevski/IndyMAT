"""Real octave-cli PID representation checks, private schema-2 package copies.

Oracle: dalga26/olcum2/olc26-control.txt, MATLAB R2025b.
PID01-10 defaults/realizations; PID11/14/15 validation; PP01/02 getters;
PP03 query error; PS01-15 mutation/formulas; PA01 independent copy;
PC01-05/08/10 conversions; PADD/PSUB/PMUL/PDIV/PFB/PSER/PPAR
ordered PID/tf/ss/double result classes; PINV inverse. Project scope errors
for valid MATLAB forms PID12/13 and unmeasured forms are separate tests.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from tests.control_package_fixture26 import prepare_control

ROOT=Path(__file__).resolve().parents[1]

class ControlPID26Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not shutil.which('octave-cli') or not (ROOT/'.packages/control-4.2.3').is_dir():
            raise unittest.SkipTest('real octave-cli/control package is unavailable')
        fleet=Path.home()/'.cache/indymat-fleet/dalga26'
        cls.temp=tempfile.TemporaryDirectory(prefix='pid26-private-',dir=fleet)
        cls.addClassCleanup(cls.temp.cleanup)
        project=Path(cls.temp.name)
        cls.control, cls.arch = prepare_control(ROOT, project, shutil.which('octave-cli'))
        cls.datatypes=project/'.packages/datatypes-1.5.0'
        shutil.copytree(ROOT/'.packages/datatypes-1.5.0',cls.datatypes)

    def octave(self,body):
        text=f"addpath('{self.datatypes}'); addpath('{self.arch}'); addpath('{self.control}');\n"+body
        result=subprocess.run(['octave-cli','--quiet','--no-init-file','--eval',text],capture_output=True,text=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

    def test_integer_single_and_logical_parameters(self):
        self.octave("""
q=pid(int8(100),int8(100),int8(100),int8(1));
assert(strcmp(class(q.Kp),'double')); [n,d]=tfdata(tf(q),'v'); assert(n,[200 200 100]); assert(d,[1 1 0]);
q=pid(single(2),single(3),single(4),single(.2)); assert(strcmp(class(q.Kp),'double')&&strcmp(class(q.Tf),'double'));
q.Kp=int8(2); assert(strcmp(class(q.Kp),'double')&&q.Kp==2);
try, pid(true,1); error('test:missing','expected rejection'); catch e,
assert(strcmp(e.identifier,'Control:ltiobject:pidSet1'));
assert(strcmp(e.message,'The value of the "Kp" property must be a finite and real number.')); end;
""")

    def test_measured_constructors_and_realizations(self):
        self.octave('''
        specs={[1 0 0 0],[2 0 0 0],[2 3 0 0],[2 3 4 0],[2 3 4 .2],[0 0 0 0],[2 0 4 .2],[2 3 0 .2],[-2 3 4 .2],[0 3 4 .2]};
        nums={1,2,[2 3],[4 2 3],[22 13 15],0,[22 10],[2 3],[18 -7 15],[20 3 15]};
        dens={1,1,[1 0],[0 1 0],[1 5 0],1,[1 5],[1 0],[1 5 0],[1 5 0]};
        for k=1:numel(specs)
          v=num2cell(specs{k}); q=pid(v{:}); assert(strcmp(class(q),'pid')); assert(isa(q,'lti')); assert(!isa(q,'tf')); assert(size(q),[1 1]);
          assert([q.Kp q.Ki q.Kd q.Tf],specs{k}); assert(q.Ts,0); assert(q.IFormula,''); assert(q.DFormula,'');
          [n,d]=tfdata(tf(q),'vector'); assert(n,nums{k},1e-12); assert(d,dens{k},1e-12); expected=tf(nums{k},dens{k}); assert(freqresp(tf(q),[.1 1 10]),freqresp(expected,[.1 1 10]),1e-11);
        endfor
        assert(pid().Kp,1);
        ''')

    def test_measured_get_set_copy(self):
        self.octave('''
        q=pid(2,3,4,.2); s=get(q); assert(numel(fieldnames(s)),20); assert(properties(q),{'Kp';'Ki';'Kd';'Tf';'IFormula';'DFormula';'InputDelay';'OutputDelay';'InputName';'InputUnit';'InputGroup';'OutputName';'OutputUnit';'OutputGroup';'Notes';'UserData';'Name';'Ts';'TimeUnit';'SamplingGrid'});
        assert(class(s.Notes),'string'); assert(size(s.Notes),[0 1]);
        keys={'Kp','Ki','Kd','Tf'}; vals={5,6,7,.4};
        for k=1:4, r=set(q,keys{k},vals{k}); assert(get(r,keys{k}),vals{k}); assert(q.Kp,2); endfor
        for key={'IFormula','DFormula'}
          for f={'ForwardEuler','BackwardEuler','Trapezoidal'}
            r=set(q,key{1},f{1}); assert(get(r,key{1}),''); assert(freqresp(tf(r),[.1 1 10]),freqresp(tf(q),[.1 1 10]),1e-12);
          endfor
        endfor
        r=q; r.Kp=5; assert(r.Kp,5); assert(q.Kp,2); set(q,'Kp',8); assert(q.Kp,8); r=set(q,'Name','pid26'); assert(r.Name,'pid26');
        ''')

    def test_measured_exact_conversions(self):
        self.octave('''
        fixtures={pid(2,3,4,.2),pid(2,3),pid(2),pid(2,0,4,.2)};
        for q=fixtures
          q=q{1}; r=pid(tf(q)); assert([r.Kp r.Ki r.Kd r.Tf],[q.Kp q.Ki q.Kd q.Tf],1e-10); assert(freqresp(tf(r),[.1 1 10]),freqresp(tf(q),[.1 1 10]),1e-10);
        endfor
        q=pid(2,3,4,.2); s=ss(q); assert(class(s),'ss'); r=pid(s); assert([r.Kp r.Ki r.Kd r.Tf],[2 3 4 .2],1e-10);
        ''')

    def test_measured_operations(self):
        self.octave('''
        q=pid(2,3,4,.2); p=pid(1,2); t=tf([1 5],[1 8 12]); s=ss(tf(1,[1 6])); w=[.1 1 10];
        ops={'plus','minus','mtimes','mrdivide','feedback','series','parallel'};
        for op=ops
          op=op{1};
          for b={p,t,s}
            b=b{1}; for rev=0:1
              if(rev), a=b; c=q; else, a=q; c=b; endif
              r=feval(op,a,c); expected=feval(op,tf(a),tf(c)); typ='tf'; if(isa(a,'ss')||isa(c,'ss')),typ='ss';endif; assert(class(r),typ); assert(freqresp(r,w),freqresp(expected,w),1e-9);
            endfor
          endfor
          for rev=0:1
            if(rev), a=2; b=q; else, a=q; b=2; endif
            r=feval(op,a,b); aa=a; bb=b; if(isa(a,'pid')),aa=tf(a);endif; if(isa(b,'pid')),bb=tf(b);endif;
            expected=feval(op,aa,bb); typ='pid'; if(strcmp(op,'feedback')||(strcmp(op,'mrdivide')&&rev)),typ='tf';endif;
            assert(class(r),typ); assert(freqresp(tf(r),w),freqresp(expected,w),1e-9);
          endfor
        endfor
        assert(class(inv(q)),'tf'); assert(freqresp(inv(q),w),1./freqresp(tf(q),w),1e-10);
        ''')

    def test_measured_invalid_and_explicit_scope(self):
        self.octave('''
        calls={@() pid(2,3,4,-.2),@() pid(Inf),@() pid(1+1i),@() set(pid(),'IFormula','bogus'),@() set(pid()),@() set(pid(),'InputDelay',1)};
        ids={'Control:ltiobject:pidSet2','Control:ltiobject:pidSet1','Control:ltiobject:pidSet1','Control:ltiobject:pidInvalidFormula','Control:general:OptionHelper1','Control:ltiobject:ReadOnlyDelay'};
        for k=1:numel(calls), caught=false; try,calls{k}();catch e,caught=true;assert(e.identifier,ids{k});end_try_catch;assert(caught);endfor
        calls={@() pid(2,3,4,.2,.1),@() pid([1 2]),@() pid(tf(1,[1 2 3])),@() set(pid(),'Ts',.1),@() pid(tf([22 13 15],[1 5 1e-20]))};
        ids={'IndyMAT:control:UnsupportedDiscreteTime','IndyMAT:control:UnsupportedModelArray','IndyMAT:control:UnsupportedPIDConversion','IndyMAT:control:UnsupportedDiscreteTime','IndyMAT:control:UnsupportedPIDConversion'};
        for k=1:numel(calls),caught=false;try,calls{k}();catch e,caught=true;assert(e.identifier,ids{k});end_try_catch;assert(caught);endfor
        q=pid(2,3,4,.2); try,set(q,'Kp',8,'Tf',-1);catch,end_try_catch;assert(q.Kp,2);
        ''')

if __name__=='__main__': unittest.main()
