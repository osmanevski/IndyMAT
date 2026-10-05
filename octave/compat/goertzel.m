function y = goertzel (x, k)
% GOERTZEL Evaluate the DFT at integer bin indices.
% Supports numeric vectors and MATLAB's one-based DFT bin indices 1:N.
% Output orientation follows x (measured in MATLAB R2025b). The bins are taken
% from a full FFT: same DFT values, without the Goertzel recursion's economy.
  if (! isnumeric (x) || ! isvector (x) || isempty (x))
    error ('goertzel: x must be a nonempty numeric vector');
  endif
  n = numel (x);
  if (nargin < 2), k = 1:n; endif
  if (! isnumeric (k) || ! isvector (k) || any (! isfinite (k(:))) || any (k(:) != fix (k(:))) || any (k(:) < 1) || any (k(:) > n))
    error ('goertzel: k must contain integer one-based bins from 1 to N');
  endif
  values = fft (x(:), n); y = values(k(:));
  if (isrow (x)), y = y.'; endif
endfunction
