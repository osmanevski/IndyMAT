function [a, efinal] = rc2poly (k, r0)
% RC2POLY Convert real reflection coefficients to a prediction polynomial.
% Supports a real finite vector with |k| < 1 using step-up recursion.
% Optional r0 is initial prediction error; efinal = r0*prod(1-k.^2).
  if (! isnumeric (k) || ! isvector (k) || ! isreal (k) || any (! isfinite (k(:))) || any (abs (k(:)) >= 1))
    error ('rc2poly: k must be a finite real vector with magnitude below one');
  endif
  if (nargin < 2), r0 = 1; endif
  if (! isnumeric (r0) || ! isscalar (r0) || ! isfinite (r0) || r0 <= 0)
    error ('rc2poly: r0 must be a positive finite scalar');
  endif
  k = k(:).'; a = 1;
  for m = 1:numel (k)
    a = [a 0] + k(m) * [0 fliplr(a)];
  endfor
  efinal = r0 * prod (1 - k.^2);
endfunction
