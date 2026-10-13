% MATLAB R2025b measured bounds; general delta-method formula also documented by MathWorks.
%% wave26-norminv-confidence | norminv
[x,lo,hi]=norminv([.1 .5 .9],2,3,eye(2));
assert(max(abs(x-[-1.8446546966338015 2 5.8446546966338015]))<1e-12);
assert(max(abs(lo-[-5.030651009351967 .040036015459945506 2.6586583839156357]))<1e-12);
assert(max(abs(hi-[1.3413416160843643 3.9599639845400545 9.030651009351967]))<1e-12);
%% wave26-norminv-confidence-covariance | norminv
[x,lo,hi]=norminv([.1 .5 .9],2,3,[4 1;1 9]);
assert(max(abs(lo-[-9.7378038341543736 -1.919927969080109 -3.2103943683732155]))<1e-12);
assert(max(abs(hi-[6.0484944408867714 5.919927969080109 14.899703761640819]))<1e-12);
[x,lo,hi]=norminv([.1 .5 .9],2,3,zeros(2)); assert(isequal(x,lo)&&isequal(x,hi));
%% wave26-norminv-confidence-nonpsd | norminv
failed=false; try, [x,lo,hi]=norminv([.1 .5 .9],2,3,[1 2;2 1]); catch e, failed=strcmp(e.identifier,'stats:norminv:BadCovarianceSymPos'); end; assert(failed);
