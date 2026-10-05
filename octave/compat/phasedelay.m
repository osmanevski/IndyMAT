function [tau, w] = phasedelay (b, a, n)
% PHASEDELAY Phase delay of an IIR or FIR digital filter.
% Supports finite SISO coefficient vectors and an integer number of points
% on [0,pi]. Frequency-vector and sample-rate forms are unsupported.
% MATLAB returns NaN at zero frequency, where phase delay is undefined.
  if (nargin < 2 || nargin > 3 || ! isnumeric (b) || ! isvector (b) || isempty (b) || ! isnumeric (a) || ! isvector (a) || isempty (a))
    error ('phasedelay: b and a coefficient vectors are required');
  endif
  if (nargin < 3), n = 512; endif
  if (! isnumeric (n) || ! isscalar (n) || ! isfinite (n) || n < 2 || fix (n) != n)
    error ('phasedelay: frequency-point count must be an integer of at least two');
  endif
  [h, w] = freqz (b, a, n); phase = unwrap (angle (h)); tau = zeros (size (w));
  nz = (w != 0); tau(nz) = -phase(nz) ./ w(nz);
  tau(! nz) = NaN;
endfunction
