% R2025b olc26-control: PID01–10 PS01–15 PC01–10 and ordered operation rows.
%% wave26-control-pid-construct | pid tf tfdata
q=pid(2,3,4,.2); assert(isa(q,'pid') && isa(q,'lti') && ~isa(q,'tf'));
assert(isequal([q.Kp q.Ki q.Kd q.Tf],[2 3 4 .2]));
[n,d]=tfdata(tf(q),'v'); assert(max(abs(n-[22 13 15]))<1e-11); assert(max(abs(d-[1 5 0]))<1e-11);
%% wave26-control-pid-mutation | pid set
q=pid(2,3,4,.2); r=q; r.Kp=5; assert(q.Kp==2 && r.Kp==5); set(q,'Kp',8); assert(q.Kp==8);
r=set(q,'IFormula','Trapezoidal'); assert(isempty(r.IFormula));
%% wave26-control-pid-conversion | pid tf ss
q=pid(2,3,4,.2); r=pid(tf(q)); assert(max(abs([r.Kp r.Ki r.Kd r.Tf]-[2 3 4 .2]))<1e-10);
assert(isa(ss(q),'ss')); r=pid(ss(q)); assert(max(abs([r.Kp r.Ki r.Kd r.Tf]-[2 3 4 .2]))<1e-10);
%% wave26-control-pid-dispatch | pid tf ss
q=pid(2,3,4,.2); assert(isa(q+2,'pid')); assert(isa(q/2,'pid')); assert(isa(2/q,'tf'));
assert(isa(q+pid(1,2),'tf')); assert(isa(q+tf(1,[1 2]),'tf')); assert(isa(q+ss(tf(1,[1 2])),'ss'));
%% wave26-control-zpk-storage | zpk zpkdata
q=zpk([-2 -1],[-4 -3],2); assert(strcmp(class(q),'zpk')); assert(isa(q,'lti'));
[z,p,k]=zpkdata(q); assert(iscell(z)&&iscell(p)); assert(isequal(z{1},[-2;-1])&&isequal(p{1},[-4;-3])&&k==2);
[z,p,k]=zpkdata(q,'v'); assert(isequal(z,[-2;-1])&&isequal(p,[-4;-3])&&k==2);
%% wave26-control-zpk-static | zpk zpkdata
q=zpk(3); [z,p,k]=zpkdata(q,'v'); assert(strcmp(class(q),'zpk')&&isequal(size(z),[0 1])&&isequal(size(p),[0 1])&&k==3);
%% wave26-control-zpk-conversion | zpk tf ss freqresp
q=zpk(-1,[-2 -3],2); w=[.1 1 10]; expected=2*(1i*w+1)./((1i*w+2).*(1i*w+3));
assert(max(abs(reshape(freqresp(q,w),1,[])-expected))<1e-10);
assert(isa(tf(q),'tf')&&isa(ss(q),'ss')&&isa(zpk(tf(q)),'zpk')&&isa(zpk(ss(q)),'zpk'));
%% wave26-control-zpk-dispatch | zpk tf ss feedback series parallel inv
q=zpk(-1,[-2 -3],2); t=tf(1,[1 1]);
assert(isa(q+t,'zpk')&&isa(t*q,'zpk')&&isa(2/q,'zpk'));
assert(isa(feedback(q,1),'zpk')&&isa(series(q,t),'zpk')&&isa(parallel(q,t),'zpk')&&isa(inv(q),'zpk'));
assert(isa(q+ss(t),'ss')&&isa(ss(t)*q,'ss'));

%% wave26-control-zpk-zero-complex | zpk zpkdata | MATLAB D01/D04/D05/D09/D19/D20: actual value representation and ordered columns
q=zpk([-2 -1],[-4 -3],0); [z,p,k]=zpkdata(q); assert(isa(q,'zpk') && isa(q,'lti')); assert(isequal(z,{[-2;-1]}) && isequal(p,{[-4;-3]}) && k==0);
[z,p,k]=zpkdata(zpk([],[],3),'v'); assert(isequal(size(z),[0 1]) && isequal(size(p),[0 1]) && k==3);
q=zpk(-1+2i,-3,2+1i); assert(isequal(q.Z,{-1+2i}) && q.K==2+1i);

%% wave26-control-zpk-properties | zpk get set | MATLAB ZP01/ZP02/ZS10/ZS11/ZA10: values, caller mutation, independent copies
q=zpk([-2 -1],[-4 -3],2); original=q; q=set(q,'Z',{[-7;-8]},'K',5); assert(isequal(original.Z,{[-2;-1]}) && original.K==2); assert(isequal(q.Z,{[-7;-8]}) && q.K==5);
set(q,'K',7); q.Z{1}(1)=-9; assert(q.K==7 && isequal(q.Z,{[-9;-8]})); assert(numel(properties(q))==20 && isstruct(get(q)));

%% wave26-control-zpk-conversions | zpk tf ss zpkdata | MATLAB CV01/CV02/CV03/CV04: actual conversion algorithms and frequency equivalence
q=zpk([-2 -1],[-4 -3],2); [n,d]=tfdata(tf(q),'v'); assert(norm(n-[2 6 4])<1e-12 && norm(d-[1 7 12])<1e-12);
r=zpk(ss(q)); [z,p,k]=zpkdata(r,'v'); assert(norm(z-[-1;-2])<1e-10 && norm(p-[-3;-4])<1e-10 && abs(k-2)<1e-10); assert(norm(squeeze(freqresp(r,[.1 1 10]))-squeeze(freqresp(q,[.1 1 10])))<1e-10);

%% wave26-control-zpk-analysis | zpk pole zero dcgain isstable freqresp | MATLAB AN01/AN02/AN04/AN05/AN06/AN07
q=zpk([-2 -1],[-4 -3],2); assert(isequal(pole(q),[-4;-3]) && isequal(zero(q),[-2;-1])); assert(abs(dcgain(q)-1/3)<1e-12 && isstable(q));
expected=[.33372732497239171+.030558037741394983i .37647058823529417+.30588235294117649i 1.69629863967099+.66751028155646952i]; assert(norm(reshape(freqresp(q,[.1 1 10]),1,[])-expected)<1e-10);

%% wave26-control-zpk-ordered-products | zpk tf zpkdata | MATLAB MUL_ZT/MUL_TZ/DIV_ZT/DIV_TZ: ordered root concatenation
q=zpk(-1,[-3 -4],2); t=tf([1 5],[1 8 12]); [z,p,k]=zpkdata(q*t,'v'); assert(norm(z-[-1;-5])<1e-10 && norm(p-[-3;-4;-6;-2])<1e-10 && abs(k-2)<1e-10);
[z,p,k]=zpkdata(t*q,'v'); assert(norm(z-[-5;-1])<1e-10 && norm(p-[-6;-2;-3;-4])<1e-10 && abs(k-2)<1e-10);
