function tf = islinphase (b)
% ISLINPHASE Test whether real FIR coefficients have exact linear phase.
% Supports finite real coefficient vectors; tolerance is 64 machine eps.
  if (! isnumeric (b) || ! isvector (b) || isempty (b) || ! isreal (b) || any (! isfinite (b(:))))
    error ('islinphase: b must be a nonempty finite real coefficient vector');
  endif
  b = b(:).'; tol = 64 * eps * max (1, norm (b, inf));
  tf = norm (b - fliplr (b), inf) <= tol || norm (b + fliplr (b), inf) <= tol;
endfunction
