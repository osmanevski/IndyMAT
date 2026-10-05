function [p, f] = __mf_pwelch_frequency__ (x, window, overlap, f, fs = 2*pi)
% __MF_PWELCH_FREQUENCY__ Welch two-sided PSD at explicit frequencies.
% Double/single vectors, explicit scalar Hamming lengths or real windows,
% and sample overlap are supported. Default windows and package mode/options
% are unsupported by this new form; existing scalar-FFT forms remain separate.
  if (! isfloat (x) || ! isvector (x) || isempty (x) || any (! isfinite (x(:))))
    error ('pwelch: x must be a nonempty finite double or single vector');
  endif
  x = x(:);
  if (isempty (window)), error ('pwelch: frequency-vector form requires an explicit window'); endif
  if (isscalar (window))
    if (! isnumeric (window) || ! isreal (window) || ! isfinite (window) || window < 1 || fix (window) != window)
      error ('pwelch: window length must be a positive integer');
    endif
    window = hamming (double (window));
  endif
  if (! isnumeric (window) || ! isreal (window) || ! isvector (window) || isempty (window) || any (! isfinite (window(:))))
    error ('pwelch: window must be a finite real vector');
  endif
  n = numel (window);
  if (n > numel (x)), error ('pwelch: window must not exceed the input length'); endif
  if (isempty (overlap)), overlap = floor (n/2); endif
  if (! isnumeric (overlap) || ! isreal (overlap) || ! isscalar (overlap) || ! isfinite (overlap) || overlap < 0 || overlap >= n || fix (overlap) != overlap)
    error ('pwelch: overlap must be an integer smaller than the window length');
  endif
  if (isempty (fs)), fs = 2*pi; endif
  starts = 1:n-overlap:numel(x)-n+1; p = zeros (size (f));
  for j = starts
    p += __mf_frequency_psd__ (x(j:j+n-1), window, f, fs);
  endfor
  p /= numel (starts);
  if (isa (x, 'single')), p = single (p); endif
endfunction
