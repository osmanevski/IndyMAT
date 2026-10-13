% MATLAB R2025b olc26-signal-next rows1–13: validation, identity and class only.
%% wave26-interp-filter-thresholds | interp
for n=1:3
 failed=false; try, interp(1:(2*n),2,n,.5); catch e, failed=strcmp(e.identifier,'signal:interp:InvalidDimensions'); end; assert(failed);
 assert(isequal(size(interp(1:(2*n+1),2,n,.5)),[1 2*(2*n+1)]));
end
%% wave26-interp-rate-one-identity | interp
assert(isequal(interp(1:9,1),1:9)); assert(isequal(interp(single(1:9),1),single(1:9)));
%% wave26-interp-single-measured-forms | interp
assert(isa(interp(single(1:9),3),'single')); assert(isa(interp(single(1:9),2,4),'single'));
assert(isa(interp(single((1:9)'),2),'single')); assert(isa(interp(single(1:3),2,1),'single'));
