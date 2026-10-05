function [z, p, k] = tf2zpk (b, a)
% TF2ZPK Convert SISO transfer-function polynomials to zeros, poles, gain.
% Supports finite numeric polynomial vectors in descending powers only.
  if (nargin != 2 || ! isnumeric (b) || ! isnumeric (a) || ! isvector (b) || ! isvector (a) || isempty (b) || isempty (a) || any (! isfinite (b(:))) || any (! isfinite (a(:))))
    error ('tf2zpk: b and a must be nonempty finite polynomial vectors');
  endif
  b = b(:).'; a = a(:).'; b = trim_leading (b); a = trim_leading (a);
  if (a(1) == 0), error ('tf2zpk: denominator leading coefficient must be nonzero'); endif
  k = b(1) / a(1);
  if (numel (b) == 1 || all (b == 0)), z = zeros (0, 1); else, z = roots (b); endif
  if (numel (a) == 1), p = zeros (0, 1); else, p = roots (a); endif
endfunction

function v = trim_leading (v)
  first = find (v != 0, 1);
  if (isempty (first)), v = 0; else, v = v(first:end); endif
endfunction
