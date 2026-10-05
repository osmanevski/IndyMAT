function p = __mf_frequency_psd__ (x, window, f, fs)
% __MF_FREQUENCY_PSD__ Two-sided windowed PSD at explicit frequencies.
% Finite double/single vectors and real windows only; no range conversion.
  if (! isfloat (x) || ! isvector (x) || isempty (x) || any (! isfinite (x(:))))
    error ('periodogram: x must be a nonempty finite double or single vector');
  endif
  if (! isnumeric (f) || ! isreal (f) || ! isvector (f) || any (! isfinite (f(:))))
    error ('periodogram: frequencies must be a finite real vector');
  endif
  if (! isnumeric (fs) || ! isreal (fs) || ! isscalar (fs) || ! isfinite (fs) || fs <= 0)
    error ('periodogram: sample rate must be a positive finite scalar');
  endif
  single_input = isa (x, 'single'); x = double (x(:));
  if (isempty (window)), window = ones (size (x)); endif
  if (! isnumeric (window) || ! isreal (window) || ! isvector (window) || numel (window) != numel (x) || any (! isfinite (window(:))))
    error ('periodogram: window must be a finite real vector matching x');
  endif
  window = double (window(:)); energy = sum (window.^2);
  if (energy == 0), error ('periodogram: window must have nonzero energy'); endif
  x .*= window; t = (0:numel(x)-1)'; p = zeros (size (f));
  for j = 1:numel (f)
    p(j) = abs (sum (x .* exp (-2i*pi*f(j)/fs*t)))^2 / (fs*energy);
  endfor
  if (single_input), p = single (p); endif
endfunction
