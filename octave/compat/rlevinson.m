function [r, U, k] = rlevinson (a, efinal)
% RLEVINSON Recover autocorrelation data from a prediction polynomial.
% Supports first-order finite real monic AR polynomials and positive final
% error; higher orders are rejected until U is measured for them.
  if (nargin != 2 || ! isnumeric (a) || ! isvector (a) || isempty (a) || ! isreal (a) || any (! isfinite (a(:))))
    error ('rlevinson: a and efinal are required finite real inputs');
  endif
  if (! isnumeric (efinal) || ! isscalar (efinal) || ! isfinite (efinal) || efinal <= 0)
    error ('rlevinson: efinal must be a positive finite scalar');
  endif
  a = a(:).'; p = numel (a) - 1;
  if (a(1) == 0), error ('rlevinson: a must have a nonzero leading coefficient'); endif
  a = a / a(1); k = poly2rc (a);
  if (p != 1), error ('rlevinson: only first-order polynomials are supported'); endif
  if (any (abs (k) >= 1)), error ('rlevinson: polynomial must be stable'); endif
  A = zeros (p, p); rhs = zeros (p, 1);
  for lag = 1:p
    rhs(lag) = 0;
    A(lag,lag) += 1;
    for q = 1:p
      idx = abs (lag - q);
      if (idx == 0), rhs(lag) -= a(q + 1);
      else, A(lag,idx) += a(q + 1); endif
    endfor
  endfor
  normalized = A \ rhs;
  r = [1; normalized];
  error_ratio = r(1) + sum (a(2:end).' .* r(2:end));
  if (! isfinite (error_ratio) || error_ratio <= 0)
    error ('rlevinson: polynomial does not define a positive autocorrelation');
  endif
  r *= efinal / error_ratio;
  U = eye (p + 1);
  for row = 1:p
    count = p + 2 - row;
    U(row,row:row+count-1) = a(1:count);
  endfor
endfunction
