function p = bandpower (x, varargin)
% BANDPOWER Estimate signal power or integrate a supplied PSD.
% Supports real numeric vectors/matrices by columns with a Hamming-window
% periodogram, and evenly spaced one-sided PSD vectors/matrices by rectangle
% integration. Other spectrum formats and frequency grids are unsupported.
  if (! isnumeric (x) || isempty (x) || ndims (x) > 2)
    error ('bandpower: input must be a nonempty numeric vector or matrix');
  endif
  psd_mode = false; fs = 1; freqrange = [];
  if (numel (varargin) >= 1 && ischar (varargin{end}) && strcmpi (varargin{end}, 'psd'))
    psd_mode = true; varargin(end) = [];
  endif
  if (psd_mode)
    if (numel (varargin) < 1 || numel (varargin) > 2)
      error ('bandpower: PSD form is bandpower(pxx,f[,freqrange],psd)');
    endif
    f = varargin{1};
    if (numel (varargin) == 2), freqrange = varargin{2}; endif
    p = integrate_psd (x, f, freqrange);
    return;
  endif
  if (numel (varargin) > 2), error ('bandpower: unsupported input form'); endif
  if (numel (varargin) >= 1 && ! isempty (varargin{1})), fs = varargin{1}; endif
  if (numel (varargin) == 2), freqrange = varargin{2}; endif
  if (! isreal (x)), error ('bandpower: complex input is unsupported'); endif
  if (! isnumeric (fs) || ! isscalar (fs) || ! isfinite (fs) || fs <= 0)
    error ('bandpower: sample rate must be a positive finite scalar');
  endif
  if (isempty (freqrange)), freqrange = [0 fs/2]; endif
  validate_range (freqrange, [0 fs/2]);
  data = double (x); if (isvector (data)), data = data(:); endif
  n = rows (data); nfft = max (256, 2^nextpow2 (n)); win = hamming (n, 'symmetric');
  bins = floor (nfft/2) + 1; out = zeros (1, columns (data)); f = (0:bins-1)' * (fs/nfft);
  for k = 1:columns (data)
    raw = fft (data(:,k) .* win, nfft); q = abs (raw(1:bins)).^2 / (fs * sum (win.^2));
    if (rem (nfft, 2) == 0), q(2:end-1) *= 2; else, q(2:end) *= 2; endif
    out(k) = integrate_psd (q, f, freqrange);
  endfor
  if (isvector (x)), p = out(1); else, p = out; endif
endfunction

function p = integrate_psd (pxx, f, freqrange)
  if (! isnumeric (pxx) || ! isvector (f) || numel (f) < 2 || ! isnumeric (f) || ! isreal (f))
    error ('bandpower: pxx and f must be numeric PSD data and a frequency vector');
  endif
  f = f(:); if (any (! isfinite (f)) || any (diff (f) <= 0))
    error ('bandpower: f must be finite and strictly increasing');
  endif
  delta = diff (f); if (any (abs (delta - delta(1)) > 1e-10 * max (1, abs (delta(1)))))
    error ('bandpower: f must be evenly spaced');
  endif
  if (rows (pxx) != numel (f) && columns (pxx) == numel (f)), pxx = pxx.'; endif
  if (rows (pxx) != numel (f)), error ('bandpower: PSD rows must match f'); endif
  if (isempty (freqrange)), freqrange = [f(1) f(end)]; endif
  validate_range (freqrange, [f(1) f(end)]);
  idx = f >= freqrange(1) & f <= freqrange(2);
  if (! any (idx)), p = zeros (1, columns (pxx)); else, p = sum (pxx(idx,:), 1) * delta(1); endif
  if (isvector (pxx)), p = p(1); endif
endfunction

function validate_range (range, bounds)
  if (! isnumeric (range) || ! isvector (range) || numel (range) != 2 || any (! isfinite (range)) || range(1) < bounds(1) || range(2) > bounds(2) || range(1) >= range(2))
    error ('bandpower: frequency range must be increasing and inside the supported frequencies');
  endif
endfunction
