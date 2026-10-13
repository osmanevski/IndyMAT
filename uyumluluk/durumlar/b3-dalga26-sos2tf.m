% MATLAB R2025b olc26-signal-next: 2/3 sections, exactly one final zero removed.
%% wave26-sos2tf-two-sections | sos2tf
s=[1 2 3 1 -.5 .1;1 0 0 1 -.2 .05]; [b,a]=sos2tf(s);
assert(isequal(b,[1 2 3 0])); assert(max(abs(a-[1 -.7 .25 -.045 .005]))<1e-14);
%% wave26-sos2tf-three-sections | sos2tf
s=[1 2 3 1 -.5 .1;1 0 0 1 -.2 .05;1 0 0 1 -.2 .05]; [b,a]=sos2tf(s);
assert(isequal(b,[1 2 3 0 0 0])); assert(max(abs(a-[1 -.9 .44 -.13 .0265 -.00325 .00025]))<1e-14);
%% wave26-sos2tf-gains-delay | sos2tf
s=[1 2 3 1 -.5 .1;1 0 0 1 -.2 .05];
for g={0,2,[2 3]}, [b,a]=sos2tf(s,g{1}); assert(isequal(b,prod(g{1})*[1 2 3 0])); end
[b,a]=sos2tf([0 1 0 1 -.5 .1;1 0 0 1 -.2 .05]); assert(isequal(b,[0 1 0 0]));
