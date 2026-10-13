"""Real, nongraphics Octave checks for the measured SISO CT zpk subset.

Oracle: olc26-control.txt Z/D, ZP/ZS/ZA/ZI, CV, AN and ordered
ADD/SUB/MUL/DIV/FB/SER/PAR rows. These tests never modify live packages.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from tests.control_package_fixture26 import prepare_control

ROOT = Path(__file__).resolve().parents[1]


class ControlZpk26Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.octave = os.environ.get('MATLAB_FREE_OCTAVE') or shutil.which('octave-cli')
        source = ROOT / '.packages' / 'control-4.2.3'
        if not cls.octave or not source.is_dir():
            raise unittest.SkipTest('Requires octave-cli and the local control 4.2.3 package')
        cache = Path.home() / '.cache' / 'indymat-fleet' / 'dalga26'
        cache.mkdir(parents=True, exist_ok=True)
        cls.work = Path(tempfile.mkdtemp(prefix='zpk26-test-', dir=cache))
        cls.addClassCleanup(shutil.rmtree, cls.work)
        cls.package, cls.arch = prepare_control(ROOT, cls.work, cls.octave)
        cls.datatypes = ROOT / '.packages' / 'datatypes-1.5.0'

    def run_octave(self, code):
        def quote(path):
            return str(path).replace("'", "''")
        setup = f"addpath('{quote(self.arch)}'); addpath('{quote(self.datatypes)}'); addpath('{quote(self.package)}','-begin'); "
        result = subprocess.run([self.octave, '--quiet', '--no-gui', '--eval', setup + code],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_empty_population_preserves_channel_names(self):
        self.run_octave("""
for key={'InputName','OutputName'}
  name=key{1}; label='u'; if(strcmp(name,'OutputName')), label='y'; endif
  q=set(zpk(),'Z',{-1},'P',{-2},'K',3,name,{label});
  t=tf(q); assert(isequal(t.InputName,q.InputName)&&isequal(t.OutputName,q.OutputName));
  assert(isequal(q.(name),{label})&&isequal(size(q),[1 1]));
end
q=set(zpk(),'Z',{-1},'P',{-2},'K',3,'InputName',{'u'},'OutputName',{'y'});
t=ss(q); assert(isequal(t.InputName,{'u'})&&isequal(t.OutputName,{'y'}));
q=set(zpk(),'InputName',{'u'},'Z',{-1},'P',{-2},'K',3);
t=tf(q); assert(isequal(t.InputName,{'u'}));
q=zpk(); q.InputName={'u'}; assert(size(q),[0 0]); assert(isequal(q.InputName,{'u'}));
q=set(q,'Z',{-1},'P',{-2},'K',3); t=tf(q); assert(isequal(t.InputName,{'u'}));
""")

    def test_empty_population_and_partial_rejection(self):
        self.run_octave("""
q=zpk(); assert(size(q),[0 0]); assert(size(q.Z),[0 0]); assert(size(q.K),[0 0]);
r=set(q,'Z',{-1},'P',{-2},'K',3); assert(size(r),[1 1]);
[z,p,k]=zpkdata(r+1,'v'); assert(z,-1.25,1e-14); assert(p,-2); assert(k,4);
try, q.K=3; error('test:missing','expected rejection'); catch e,
assert(strcmp(e.identifier,'Control:ltiobject:zpkProperties4'));
assert(strcmp(e.message,'The values of the "Z", "P" and "K" properties must have compatible sizes.')); end;
assert(size(q),[0 0]);
r=zpk(int8(-1),single(-2),int8(100)); assert(strcmp(class(r.K),'double')); assert(strcmp(class(r.Z{1}),'double')); assert(strcmp(class(r.P{1}),'double'));
""")

    def test_storage_and_complex_data(self):
        # Z/D01–D21: exact ordered storage, empties, canceled/repeated and complex roots.
        self.run_octave("""
q=zpk([-2 -1],[-4 -3],0); assert(strcmp(class(q),'zpk')); assert(isa(q,'lti'));
assert(!isa(q,'tf')); assert(!isa(q,'DynamicSystem'));
[z,p,k]=zpkdata(q); assert(z,{[-2;-1]}); assert(p,{[-4;-3]}); assert(k,0);
[z,p,k]=zpkdata(zpk([],[],3),'v'); assert(size(z),[0 1]); assert(size(p),[0 1]); assert(k,3);
q=zpk(); assert(size(q),[0 0]); [z,p,k]=zpkdata(q,'v'); assert(iscell(z)); assert(size(z),[0 0]); assert(isempty(k));
q=zpk({[-1 -1]},{[-1 -3]},2); assert(q.Z,{[-1;-1]}); assert(q.P,{[-1;-3]});
q=zpk(-1+2i,-3,2+1i); assert(q.Z,{-1+2i}); assert(q.K,2+1i);
w=[.1 1 10]; expected=(2+1i)*(1i*w+1-2i)./(1i*w+3);
assert(reshape(freqresp(q,w),1,[]),expected,1e-13);
try tf(q); error('test:miss','Expected conversion limitation'); catch e; assert(strcmp(e.identifier,'IndyMAT:control:UnsupportedComplexConversion')); end
q=zpk(single(-1),single(-3),single(2)); assert(isa(q.K,'double'));
""")

    def test_properties_and_mutation(self):
        # ZP01–ZP18/ZP21, ZS01–ZS12/ZA01–ZA10/ZI01–ZI03.
        self.run_octave("""
q=zpk([-2 -1],[-4 -3],2); original=q;
assert(numel(properties(q)),20); assert(get(q,'z'),q.Z); assert(isstruct(get(q)));
assert(isa(q.Notes,'string')); assert(size(q.Notes),[0 1]); assert(q.InputName,{''});
q=set(q,'Z',{[-7;-8]},'K',5,'InputName',{'u'},'OutputName',{'y'},'Name','control26');
assert(original.Z,{[-2;-1]}); assert(original.K,2); assert(q.Z,{[-7;-8]}); assert(q.K,5);
set(q,'K',7); assert(q.K,7); q.Z{1}(1)=-9; assert(q.Z,{[-9;-8]});
assert(zpkdata(q(1,1)),q.Z); assert(zpkdata(q(:,:)),q.Z); assert(zpkdata(q(1)),q.Z);
try set(q,'K',9,'InputDelay',1); error('test:miss','Expected delay limitation'); catch e; assert(strcmp(e.identifier,'IndyMAT:control:UnsupportedDelay')); end
assert(q.K,7);
""")

    def test_conversions_and_numerical_analysis(self):
        # CV01–CV08; AN01–AN07/AN09–AN11; actual SS zero/pole algorithm for CV04.
        self.run_octave("""
q=zpk([-2 -1],[-4 -3],2); [n,d]=tfdata(tf(q),'v'); assert(n,[2 6 4]); assert(d,[1 7 12]);
roundtrip=zpk(tf(q)); assert(roundtrip.Z,{[-2;-1]},1e-13); assert(roundtrip.P,{[-4;-3]},1e-13);
s=ss(q); rebuilt=zpk(s); [z,k]=zero(s); p=pole(s); assert(rebuilt.Z,{z}); assert(rebuilt.P,{p}); assert(rebuilt.K,k);
assert(rebuilt.Z,{[-1;-2]},1e-12); assert(rebuilt.P,{[-3;-4]},1e-12);
w=[.1 1 10]; expected=[.33372732497239171+.030558037741394983i .37647058823529417+.30588235294117649i 1.69629863967099+.66751028155646952i];
assert(reshape(freqresp(q,w),1,[]),expected,1e-12); assert(freqresp(s,w),freqresp(q,w),1e-12);
[m,p,wout]=bode(q,w); assert(m,abs(expected(:)),1e-12); assert(wout,w(:));
assert(pole(q),[-4;-3]); assert(zero(q),[-2;-1]); assert(dcgain(q),1/3,1e-12); assert(isstable(q));
assert(!isstable(zpk([],1,1))); assert(!isstable(zpk([],0,1))); assert(isinf(dcgain(zpk([],0,1))));
[y,t]=step(q,linspace(0,2,21)); [yr,tr]=step(tf(q),linspace(0,2,21)); assert(y,yr,1e-12); assert(t,tr);
""")

    def test_mixed_operations(self):
        # All measured ordered Z/T/S/N pairs: exact class and independent transfer algebra.
        self.run_octave("""
a=zpk(-1,[-3 -4],2); b=tf([1 5],[1 8 12]); c=ss(-6,1,1,0); values={a,b,c,2}; w=[.1 1 10];
for i=1:4, for j=1:4
 if(i!=1 && j!=1), continue; endif
 x=values{i}; y=values{j}; if(isnumeric(x)), hx=x; else hx=freqresp(x,w); endif
 if(isnumeric(y)), hy=y; else hy=freqresp(y,w); endif
 outputs={x+y,x-y,x*y,x/y,feedback(x,y),series(x,y),parallel(x,y)};
 expected={hx+hy,hx-hy,hx.*hy,hx./hy,hx./(1+hx.*hy),hx.*hy,hx+hy};
 for k=1:numel(outputs)
  if(i==3 || j==3), assert(strcmp(class(outputs{k}),'ss')); else assert(strcmp(class(outputs{k}),'zpk')); endif
  assert(freqresp(outputs{k},w),expected{k},1e-10);
 endfor
endfor,endfor
q=a*b; assert(q.Z,{[-1;-5]}); assert(q.P,{[-3;-4;-6;-2]},1e-12);
q=b*a; assert(q.Z,{[-5;-1]}); assert(q.P,{[-6;-2;-3;-4]},1e-12);
q=a+a; assert(q.P,{[-3;-4;-3;-4]});
assert(freqresp(inv(a),w),1./freqresp(a,w),1e-12);
assert(freqresp(feedback(a,b,1),w),freqresp(a,w)./(1-freqresp(a,w).*freqresp(b,w)),1e-10);
""")

    def test_native_matched_conversions(self):
        # Historical ys-c2d-matched plus upstream native tf/ss conversion invariants.
        self.run_octave("""
continuous=tf(1,[1 2 1]); expected=[0 .016429 .016429];
for input={continuous,ss(continuous)}
 discrete=c2d(input{1},.2,'matched'); assert(strcmp(class(discrete),class(input{1})));
 assert(get(discrete,'tsam'),.2); n=tfdata(discrete,'v'); assert(round(n*1e6)/1e6,expected,1e-12);
 recovered=d2c(discrete,'matched'); assert(strcmp(class(recovered),class(input{1}))); assert(get(recovered,'tsam'),0);
 assert(freqresp(recovered,[.1 1 10]),freqresp(input{1},[.1 1 10]),1e-9);
endfor
conjugate=tf([1 2 5],[1 4 13]); discrete=c2d(conjugate,.1,'matched');
assert(isa(discrete,'tf')); assert(get(discrete,'tsam'),.1); assert(isreal(tfdata(discrete,'v')));
assert(freqresp(d2c(discrete,'matched'),[.1 1 10]),freqresp(conjugate,[.1 1 10]),1e-9);
""")

    def test_scope_errors(self):
        self.run_octave("""
forms={@() zpk({[-1],[-2]},{[-3],[-4]},[1 2]),@() zpk(-1,-2,1,.1),@() zpk(-1,-2,1,'InputDelay',1),@() set(zpk(1),'Unknown',0),@() subsasgn(zpk(1),substruct('()',{1,1,2}),1)};
ids={'UnsupportedMIMO','UnsupportedDiscreteTime','UnsupportedDelay','UnsupportedProperty','UnsupportedModelArray'};
for i=1:numel(forms)
 try forms{i}(); error('test:miss','Expected scope error'); catch e; assert(strcmp(e.identifier,['IndyMAT:control:' ids{i}])); end
endfor
q=zpk(1); before=q;
forms={@() set(q,'lti',2),@() set(q,'lti',tf(1)),@() zpk(-1,-2,[]),@() zpk([1 2]),@() set(q,'K',[1 2]),@() zpk('s',.1)};
ids={'UnsupportedProperty','UnsupportedProperty','UnsupportedCallForm','UnsupportedMIMO','UnsupportedMIMO','UnsupportedDiscreteTime'};
for i=1:numel(forms)
 try forms{i}(); error('test:miss','Expected invariant error'); catch e; assert(strcmp(e.identifier,['IndyMAT:control:' ids{i}])); end
endfor
assert(q.K,before.K); assert(size(q),[1 1]); assert(q.Ts,0);
assert(strcmp(class(tf(1)+ss(-1,1,1,0)),'ss')); assert(strcmp(class(tf(1)+2),'tf'));
""")

if __name__ == '__main__':
    unittest.main()
