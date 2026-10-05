function varargout = spectrogram (x, varargin)
% SPECTROGRAM Short-time Fourier transform and power estimates.
% Supports vector input, symmetric Hamming integer/default windows, explicit
% windows, overlap, FFT length, sample rate, and the centered option.
  if (nargout == 0), error ('spectrogram: plotting form is unsupported'); endif
  if (! isnumeric (x) || ! isvector (x) || isempty (x))
    error ('spectrogram: x must be a nonempty numeric vector');
  endif
  centered = false; scale = 'psd';
  while (! isempty (varargin) && ischar (varargin{end}))
    option = lower (varargin{end}); varargin(end) = [];
    if (strcmp (option, 'centered'))
      centered = true;
    elseif (any (strcmp (option, {'psd', 'power'})))
      scale = option;
    else
      error ('spectrogram: unsupported option');
    endif
  endwhile
  if (numel (varargin) > 4), error ('spectrogram: unsupported input form'); endif
  fs = 2*pi;
  if (isempty (varargin) || isempty (varargin{1}))
    L = max (1, floor (numel (x) / 4.5));
    win = hamming (L);
  else
    winarg = varargin{1};
    if (isscalar (winarg) && isnumeric (winarg))
      if (! isfinite (winarg) || winarg < 1 || fix (winarg) != winarg)
        error ('spectrogram: window length must be a positive integer');
      endif
      win = hamming (winarg);
    elseif (isnumeric (winarg) && isvector (winarg) && ! isempty (winarg))
      win = winarg(:);
    else
      error ('spectrogram: window must be a positive integer or numeric vector');
    endif
    L = numel (win);
  endif
  noverlap = floor (L / 2); nfft = max (256, 2^nextpow2 (L));
  if (numel (varargin) >= 2 && ! isempty (varargin{2})), noverlap = varargin{2}; endif
  if (numel (varargin) >= 3 && ! isempty (varargin{3})), nfft = varargin{3}; endif
  if (numel (varargin) >= 4 && ! isempty (varargin{4})), fs = varargin{4}; endif
  if (! isnumeric (noverlap) || ! isscalar (noverlap) || ! isfinite (noverlap) || fix (noverlap) != noverlap || noverlap < 0 || noverlap >= L)
    error ('spectrogram: overlap must be an integer from 0 to window length minus one');
  endif
  if (! isnumeric (nfft) || ! isscalar (nfft) || ! isfinite (nfft) || fix (nfft) != nfft || nfft < L)
    error ('spectrogram: nfft must be an integer at least the window length');
  endif
  if (! isnumeric (fs) || ! isscalar (fs) || ! isfinite (fs) || fs <= 0)
    error ('spectrogram: sample rate must be a positive finite scalar');
  endif
  x = x(:); step = L - noverlap; starts = 1:step:(numel (x) - L + 1);
  if (isempty (starts)), error ('spectrogram: x must contain at least one complete segment'); endif
  complex_input = ! isreal (x);
  if (complex_input)
    rows_out = nfft;
    f = (0:nfft-1)' * (fs / nfft);
    if (centered)
      f = ((-floor ((nfft-1)/2)):floor (nfft/2))' * (fs / nfft);
    endif
  else
    rows_out = floor (nfft/2) + 1; f = (0:rows_out-1)' * (fs / nfft);
  endif
  s = zeros (rows_out, numel (starts)); ps = zeros (rows_out, numel (starts));
  if (complex_input), s = complex (s); endif
  for k = 1:numel (starts)
    frame = x(starts(k):starts(k)+L-1) .* win; raw = fft (frame, nfft);
    if (complex_input && centered), raw = circshift (raw, floor ((nfft-1)/2)); endif
    if (! complex_input), raw = raw(1:rows_out); endif
    s(:,k) = raw; raw_ps = abs (raw).^2;
    if (strcmp (scale, 'psd'))
      ps(:,k) = raw_ps / (fs * sum (abs (win).^2));
    else
      ps(:,k) = raw_ps / (sum (abs (win))^2);
    endif
    if (! complex_input)
      if (rem (nfft, 2) == 0)
        if (rows_out > 2), ps(2:end-1,k) *= 2; endif
      elseif (rows_out > 1), ps(2:end,k) *= 2; endif
    endif
  endfor
  t = (starts - 1 + L/2) / fs;
  out = {s, f, t, ps};
  for k = 1:min (nargout, 4), varargout{k} = out{k}; endfor
  if (nargout > 4), error ('spectrogram: at most four outputs are supported'); endif
endfunction
