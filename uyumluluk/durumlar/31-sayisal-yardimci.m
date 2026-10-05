% Veri analizi ve sayısal yöntemler

%% sayisaly-deval-decay | ode45 deval | Smooth exponential, solver tolerance plus degree-five mesh interpolation
s = ode45(@(t,y) -y, [0 2], 1, odeset('RelTol',1e-8,'AbsTol',1e-10,'MaxStep',0.05));
x = linspace(0,2,81); v = deval(s,x);
assert(max(abs(v-exp(-x))) < 1e-7);

%% sayisaly-deval-components | ode45 deval | Component selection and query order
s = ode45(@(t,y) [-y(1); -2*y(2)], [0 1], [1;2], odeset('RelTol',1e-8,'AbsTol',1e-10,'MaxStep',0.04));
x = [0.8 0.1 1 0]; v = deval(s,x,[2 1]);
assert(isequal(size(v),[2 4]) && max(abs(v(1,:)-2*exp(-2*x))) < 2e-7);
assert(max(abs(v(2,:)-exp(-x))) < 1e-7);

%% sayisaly-deval-derivative | ode45 deval | Derivative of interpolant, not a new RHS evaluation
s = ode45(@(t,y) -y, [0 1], 1, odeset('RelTol',1e-9,'AbsTol',1e-11,'MaxStep',0.025));
x = linspace(0,1,51); [v,d] = deval(s,x);
assert(max(abs(v-exp(-x))) < 1e-8 && max(abs(d+exp(-x))) < 1e-6);

%% sayisaly-deval-ode23 | ode23 deval | Third-order mesh solver with degree-three interpolation
s = ode23(@(t,y) -y, [0 1], 1, odeset('RelTol',1e-7,'AbsTol',1e-9,'MaxStep',0.025));
x = linspace(0,1,43); v = deval(s,x);
assert(max(abs(v-exp(-x))) < 2e-6);

%% sayisaly-deval-backward | ode45 deval | Descending integration mesh
s = ode45(@(t,y) -y, [1 0], exp(-1), odeset('RelTol',1e-8,'AbsTol',1e-10,'MaxStep',0.04));
x = [0 0.3 1]; v = deval(s,x);
assert(max(abs(v-exp(-x))) < 1e-7);

%% sayisaly-ode113-decay | ode113 | Local 1e-8 tolerances; global fourth-order bound with safety margin
[t,y] = ode113(@(t,y) -y, [0 5], 1, odeset('RelTol',1e-8,'AbsTol',1e-10));
assert(t(1)==0 && t(end)==5 && max(abs(y-exp(-t))) < 2e-6);

%% sayisaly-ode113-oscillator | ode113 | Ten periods; accumulated phase error included in global allowance
[t,y] = ode113(@(t,y) [y(2);-y(1)], [0 20*pi], [0;1], odeset('RelTol',1e-8,'AbsTol',1e-10));
e = y-[sin(t) cos(t)]; assert(max(abs(e(:))) < 2e-5);

%% sayisaly-ode113-logistic | ode113 | Mildly nonlinear exact solution
[t,y] = ode113(@(t,y) y.*(1-y), [0 8], 0.2, odeset('RelTol',1e-8,'AbsTol',1e-10));
assert(max(abs(y-1./(1+4*exp(-t)))) < 2e-6);

%% sayisaly-ode113-system | ode113 | Vector absolute tolerances and triangular system
[t,y] = ode113(@(t,y) [-y(1);y(1)-2*y(2)], [0 4], [1;0], odeset('RelTol',1e-8,'AbsTol',[1e-10;1e-11]));
e = y-[exp(-t) exp(-t)-exp(-2*t)]; assert(max(abs(e(:))) < 2e-6);

%% sayisaly-ode113-output-times | ode113 | Requested output times with Hermite dense output
x = [0 0.01 0.1 0.7 1 2];
[t,y] = ode113(@(t,y) -y, x, 1, odeset('RelTol',1e-8,'AbsTol',1e-10,'MaxStep',0.05));
assert(isequal(t,x') && max(abs(y-exp(-t))) < 2e-6);

%% sayisaly-ode113-structure | ode113 deval | Solution structure, selected components and dense derivative
s = ode113(@(t,y) [-y(1);-2*y(2)], [0 1], [1;2], odeset('RelTol',1e-9,'AbsTol',1e-11,'MaxStep',0.02));
x = linspace(0,1,41); [v,d] = deval(s,x,2);
assert(max(abs(v-2*exp(-2*x))) < 1e-7 && max(abs(d+4*exp(-2*x))) < 2e-5);

%% sayisaly-ode113-backward | ode113 | Backward integration and initial/max step
[t,y] = ode113(@(t,y) -y, [1 0], exp(-1), odeset('RelTol',1e-8,'AbsTol',1e-10,'InitialStep',0.001,'MaxStep',0.02));
assert(t(end)==0 && all(diff(t)<0) && abs(y(end)-1) < 1e-7);

%% sayisaly-makima-formula | makima | Modified weights and extrapolated endpoint slopes
v = makima(0:3,[0 1 1 2],[0.5 1.5 2.5]);
assert(max(abs(v-[0.615625 1 1.384375])) < 1e-12);

%% sayisaly-makima-pp | makima ppval | Piecewise polynomial and node reproduction
x = [0 0.4 1 2 3]; y = [1 2 -1 0 3]; pp = makima(x,y);
assert(max(abs(ppval(pp,x)-y)) < 1e-12);
q = linspace(0,3,29); assert(max(abs(makima(x,y,q)-ppval(pp,q))) < 1e-12);

%% sayisaly-makima-two-points | makima | Linear two-point case and polynomial extrapolation
v = makima([1 3],[4 8],[0 1 2 3 4]);
assert(max(abs(v-[2 4 6 8 10])) < 1e-12);

%% sayisaly-makima-matrix | makima | Last dimension carries samples
x = 0:3; y = [0 1 1 2; 0 2 2 4]; v = makima(x,y,[0.5 1.5]);
assert(isequal(size(v),[2 2]) && max(abs(v(1,:)-[0.615625 1])) < 1e-12);
assert(max(abs(v(2,:)-2*v(1,:))) < 1e-12);

%% sayisaly-makima-nd | makima | Array-valued interpolation
x = 0:3; y = reshape(1:24,[2 3 4]); v = makima(x,y,[0 1 3]);
assert(isequal(size(v),[2 3 3]));
e = v-y(:,:,[1 2 4]); assert(max(abs(e(:))) < 1e-12);

%% sayisaly-histcounts2-edges | histcounts2 | Edge ownership, joint out-of-range bin indices
x = [0 1 2 3 -1 NaN 0.5]; y = [0 1 2 2 1 1 5];
[N,X,Y,bx,by] = histcounts2(x,y,[0 1 2 3],[0 1 2]);
assert(isequal(N,[1 0;0 1;0 2]) && isequal(X,[0 1 2 3]) && isequal(Y,[0 1 2]));
assert(isequal(bx,[1 2 3 3 0 0 0]) && isequal(by,[1 2 2 2 0 0 0]));

%% sayisaly-histcounts2-shapes | histcounts2 | Edge orientation and input array shape
x = [0 1;2 3]; y = [0 1;0 1]; xe = [0;2;4]; ye = [0;1;2];
[N,X,Y,bx,by] = histcounts2(x,y,xe,ye);
assert(isequal(N,ones(2)) && isequal(X,xe) && isequal(Y,ye));
assert(isequal(bx,[1 1;2 2]) && isequal(by,[1 2;1 2]));

%% sayisaly-histcounts2-probability | histcounts2 | Normalization includes out-of-range and NaN samples in denominator
x = [0.5 0.5 2 NaN]; y = [0.5 0.5 0.5 0.5];
p = histcounts2(x,y,[0 1],[0 1],'Normalization','probability');
assert(abs(p-0.5) < 1e-12);
d = histcounts2(x,y,[0 2],[0 3],'Normalization','pdf');
assert(abs(d-0.125) < 1e-12);

%% sayisaly-histcounts2-density | histcounts2 | Nonuniform bin area
x = [0.5 1.5 1.5]; y = [0.5 0.5 2];
N = histcounts2(x,y,[0 1 3],[0 1 4],'Normalization','countdensity');
assert(norm(N-[1 0;0.5 1/6],'fro') < 1e-12);

%% sayisaly-histcounts2-cumulative | histcounts2 | Two-axis cumulative arithmetic
x = [0.5 1.5 1.5]; y = [0.5 0.5 1.5];
N = histcounts2(x,y,[0 1 2],[0 1 2],'Normalization','cumcount');
assert(isequal(N,[1 1;2 3]));
C = histcounts2(x,y,[0 1 2],[0 1 2],'Normalization','cdf');
assert(norm(C-N/3,'fro') < 1e-12);

%% sayisaly-smoothdata-mean | smoothdata | Odd and even windows with shrinking endpoints
v = smoothdata(1:5,'movmean',3); assert(max(abs(v-[1.5 2 3 4 4.5])) < 1e-12);
v = smoothdata(1:5,'movmean',4); assert(max(abs(v-[1.5 2 2.5 3.5 4])) < 1e-12);

%% sayisaly-smoothdata-median | smoothdata | Moving median removes a single outlier
v = smoothdata([1 100 3 4 5],'movmedian',3);
assert(max(abs(v-[50.5 3 4 4 4.5])) < 1e-12);

%% sayisaly-smoothdata-nan | smoothdata | Missing values omitted by default; all-missing remains missing
v = smoothdata([1 2 NaN 4 5],'movmean',3);
assert(max(abs(v-[1.5 1.5 3 4.5 4.5])) < 1e-12);
v = smoothdata([NaN NaN NaN 4],'movmedian',3); assert(isnan(v(1)) && isnan(v(2)) && v(3)==4);

%% sayisaly-smoothdata-dimension | smoothdata | Explicit axis and array shape
A = reshape(1:24,[2 3 4]); v = smoothdata(A,2,'movmean',3);
e = v(:,2,:)-mean(A,2); assert(isequal(size(v),size(A)) && max(abs(e(:))) < 1e-12);
[v,w] = smoothdata(single([1 2 3]),'movmean',[1 0]);
assert(isa(v,'single') && isequal(w,[1 0]) && max(abs(double(v)-[1 1.5 2.5])) < 1e-6);

%% sayisaly-smoothdata-gaussian | smoothdata | Gaussian sigma is window/5 and endpoints are renormalized
q = exp(-25/18); v = smoothdata([0 0 1 0 0],'gaussian',3);
assert(max(abs(v-[0 q 1 q 0]/(1+2*q))) < 1e-12);
v = smoothdata([1 0 0],'gaussian',3); assert(abs(v(1)-1/(1+q)) < 1e-12);
