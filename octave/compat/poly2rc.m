function [k, r0] = poly2rc (a, efinal)
% POLY2RC Convert a stable real prediction polynomial to reflection values.
% Supports finite monic real polynomials whose step-down recursion remains
% nonsingular and has |k| < 1. Optional efinal sets the prediction error;
% r0 is efinal divided by prod(1-k.^2).
  if (! isnumeric (a) || ! isvector (a) || isempty (a) || ! isreal (a) || any (! isfinite (a(:))))
    error ('poly2rc: a must be a nonempty finite real polynomial vector');
  endif
  a = a(:).';
  if (a(1) == 0), error ('poly2rc: leading coefficient must be nonzero'); endif
  if (nargin < 2), efinal = 1; endif
  if (! isnumeric (efinal) || ! isscalar (efinal) || ! isfinite (efinal) || efinal <= 0)
    error ('poly2rc: efinal must be a positive finite scalar');
  endif
  a = a / a(1); k = zeros (max (0, numel (a) - 1), 1);
  for m = numel (a):-1:2
    km = a(m); if (abs (km) >= 1), error ('poly2rc: polynomial is not stable'); endif
    k(m-1) = km; prev = (a(1:m-1) - km * fliplr (a(2:m))) / (1 - km^2);
    a = prev;
  endfor
  r0 = efinal / prod (1 - k.^2);
endfunction
