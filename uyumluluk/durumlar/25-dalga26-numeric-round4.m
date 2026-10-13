% MATLAB R2025b olc26-numeric-next measured outputs.
%% wave26-norm-negative-vector | norm
assert(abs(norm([3 2],-1)-1.2000000000000002)<1e-14);
assert(abs(norm([3 -2],-2)-1.6641005886756874)<1e-14);
assert(isinf(norm([3 2],0))); assert(norm([3 2],-Inf)==2);
%% wave26-norm-negative-special | norm
assert(norm(Inf,0)==1 && isinf(norm(Inf,-1)) && norm([Inf 1],-2)==1);
assert(isnan(norm([NaN Inf],-1))); assert(norm(zeros(1,0),0)==0);
assert(isinf(norm(zeros(0,1),-2)) && norm(zeros(0,1),-Inf)==0);
%% wave26-norm-negative-single | norm
x=norm(single([3 2]),-1); assert(isa(x,'single') && abs(double(x)-1.2)<1e-6);
