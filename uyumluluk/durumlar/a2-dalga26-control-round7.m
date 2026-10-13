% Real MATLAB R2025b olcum-inc2-matlab.txt, pid-int8/single/int-kp/logical and zpk-empty rows.
%% wave26-pid-numeric-storage | pid tf tfdata
q=pid(int8(100),int8(100),int8(100),int8(1)); assert(isa(q.Kp,'double'));
[n,d]=tfdata(tf(q),'v'); assert(isequal(n,[200 200 100])&&isequal(d,[1 1 0]));
q=pid(single(2),single(3),single(4),single(.2)); assert(isa(q.Kp,'double')&&isa(q.Tf,'double'));
q.Kp=int8(2); assert(isa(q.Kp,'double')&&q.Kp==2);
%% wave26-pid-logical-reject | pid
failed=false; try, pid(true,1); catch e, failed=strcmp(e.identifier,'Control:ltiobject:pidSet1')&&strcmp(e.message,'The value of the "Kp" property must be a finite and real number.'); end; assert(failed);
%% wave26-zpk-empty-population | zpk set zpkdata
q=zpk(); assert(isequal(size(q),[0 0])&&iscell(q.Z)&&isequal(size(q.Z),[0 0])&&isequal(size(q.K),[0 0]));
q=set(q,'Z',{-1},'P',{-2},'K',3); assert(isequal(size(q),[1 1]));
r=q+1; assert(isa(r,'zpk')); [z,p,k]=zpkdata(r,'v'); assert(abs(z+1.25)<1e-14&&p==-2&&k==4);
%% wave26-zpk-empty-partial-reject | zpk
q=zpk(); failed=false; try, q.K=3; catch e, failed=strcmp(e.identifier,'Control:ltiobject:zpkProperties4')&&strcmp(e.message,'The values of the "Z", "P" and "K" properties must have compatible sizes.'); end; assert(failed);
