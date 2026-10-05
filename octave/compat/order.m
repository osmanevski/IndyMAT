function out = order (sys)
% ORDER Number of states in SS models or denominator degree of a SISO TF.
% MIMO transfer functions and FRD models are unsupported; no minimalization
% is performed, so a cancelled pole is still counted in an unreduced TF.
  if (nargin != 1 || ! isa (sys, 'lti')), error ('order: expected an LTI model'); endif
  if (isa (sys, 'ss'))
    [a, b, c, d] = ssdata (sys); out = rows (a);
  elseif (isa (sys, 'tf') && issiso (sys))
    [num, den] = tfdata (sys, 'v'); first = find (den != 0, 1);
    out = numel (den) - first;
  else
    error ('order: only state-space and SISO transfer-function models are supported');
  endif
endfunction
