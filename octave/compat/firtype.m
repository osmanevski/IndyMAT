function type = firtype (b)
% FIRTYPE Classify a real linear-phase FIR coefficient vector (types 1-4).
% Supports vectors with exact symmetry or antisymmetry.
  if (! isnumeric (b) || ! isvector (b) || isempty (b) || ! isreal (b) || any (! isfinite (b(:))))
    error ('firtype: b must be a nonempty finite real coefficient vector');
  endif
  b = b(:).'; tol = 64 * eps * max (1, norm (b, inf));
  symmetric = norm (b - fliplr (b), inf) <= tol;
  antisymmetric = norm (b + fliplr (b), inf) <= tol;
  if (symmetric)
    if (rem (numel (b), 2)), type = 1; else, type = 2; endif
  elseif (antisymmetric)
    if (rem (numel (b), 2)), type = 3; else, type = 4; endif
  else
    error ('firtype: b must be symmetric or antisymmetric');
  endif
endfunction
