function [upper, lower] = envelope (x, varargin)
% ENVELOPE Compute supported numeric-vector signal envelopes.
% Supports default analytic magnitude and centered truncated-window RMS.
% FIR analytic and peak spline modes remain unsupported pending derivation.
  if (! isnumeric (x) || ! isvector (x) || isempty (x) || ! isreal (x))
    error ('envelope: x must be a nonempty real numeric vector');
  endif
  was_row = isrow (x); x = double (x(:)); n = numel (x);
  if (isempty (varargin))
    mu = mean (x); a = abs (hilbert (x - mu));
    upper = mu + a; lower = mu - a;
  elseif (numel (varargin) == 2 && ischar (varargin{2}) && strcmpi (varargin{2}, 'rms'))
    len = varargin{1};
    if (! isnumeric (len) || ! isscalar (len) || ! isfinite (len) || len < 1 || fix (len) != len)
      error ('envelope: RMS window length must be a positive integer');
    endif
    left = floor ((len - 1) / 2); right = ceil ((len - 1) / 2); upper = zeros (n,1);
    for j = 1:n
      q = max (1, j-left):min (n, j+right); upper(j) = sqrt (mean (x(q).^2));
    endfor
    lower = -upper;
  else
    error ('envelope: only default analytic and RMS forms are supported');
  endif
  if (was_row), upper = upper.'; lower = lower.'; endif
endfunction
