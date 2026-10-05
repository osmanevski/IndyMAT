function [upper, lower] = envelope (x, varargin)
% ENVELOPE Compute supported numeric-vector signal envelopes.
% Supports default analytic magnitude, mean-centered truncated-window RMS,
% double-input odd-length Kaiser (beta=8) Hilbert FIR envelopes and peak splines
% with positive integer minimum separation. Peak mode supports finite signals
% with isolated extrema (constant signals also supported); plateaus error.
% Even-length Hilbert FIR phase alignment, matrices and plotting are unsupported.
  if (! isnumeric (x) || ! isvector (x) || isempty (x) || ! isreal (x))
    error ('envelope: x must be a nonempty real numeric vector');
  endif
  was_double = isa (x, 'double'); was_row = isrow (x); x = double (x(:)); n = numel (x);
  if (isempty (varargin))
    mu = mean (x); a = abs (hilbert (x - mu));
    upper = mu + a; lower = mu - a;
  elseif (numel (varargin) == 2 && ischar (varargin{2}) && strcmpi (varargin{2}, 'rms'))
    len = varargin{1};
    if (! isnumeric (len) || ! isscalar (len) || ! isfinite (len) || len < 1 || fix (len) != len)
      error ('envelope: RMS window length must be a positive integer');
    endif
    mu = mean (x); x -= mu;
    left = floor (len / 2); right = len - left - 1; upper = zeros (n,1);
    for j = 1:n
      q = max (1, j-left):min (n, j+right); upper(j) = sqrt (mean (x(q).^2));
    endfor
    lower = mu - upper; upper += mu;
  elseif (numel (varargin) == 1 || numel (varargin) == 2)
    len = varargin{1}; mode = 'analytic';
    if (numel (varargin) == 2), mode = varargin{2}; endif
    if (! isnumeric (len) || ! isreal (len) || ! isscalar (len) || ! isfinite (len) || len < 1 || fix (len) != len)
      error ('envelope: N must be a positive finite integer scalar');
    endif
    if (! ischar (mode) || ! any (strcmpi (mode, {'analytic', 'peak'})))
      error ('envelope: type must be analytic, rms or peak');
    endif
    if (! was_double || any (! isfinite (x)))
      error ('envelope: FIR analytic and peak forms require finite double data');
    endif
    if (strcmpi (mode, 'analytic'))
      if (len < 3 || mod (len,2) == 0)
        error ('envelope: FIR analytic form supports odd filter lengths at least three');
      endif
      if (len > 1e6), error ('envelope: filter length exceeds the supported allocation limit'); endif
      mu = mean (x); centered = x - mu;
      t = (-(len-1)/2:(len-1)/2)'; h = zeros (len,1);
      odd = mod (t,2) != 0; h(odd) = 2 ./ (pi*t(odd));
      h .*= kaiser (len,8);
      % Symmetric zero padding and removal of the integer group delay.
      quadrature = conv (centered,h); delay = (len-1)/2;
      a = hypot (centered,quadrature(delay+(1:n)));
      upper = mu + a; lower = mu - a;
    else
      if (all (x == x(1)))
        upper = x; lower = x;
      else
        if (any (diff (x) == 0)), error ('envelope: peak mode with flat extrema is unsupported'); endif
        upper = peak_spline (x, double (len));
        lower = -peak_spline (-x, double (len));
      endif
    endif
    if (nargout == 0), error ('envelope: plotting is unsupported; request one or two outputs'); endif
  else
    error ('envelope: expected X, optional N and analytic, rms or peak');
  endif
  if (was_row), upper = upper.'; lower = lower.'; endif
endfunction

function out = peak_spline (x, separation)
  count = numel (x);
  locations = find (x(2:end-1) > x(1:end-2) & x(2:end-1) > x(3:end)) + 1;
  % Keep the tallest first; resolve equal-height peaks in sample order.
  [unused, order] = sortrows ([-x(locations), locations], [1 2]);
  keep = zeros (0,1);
  for k = order(:)'
    candidate = locations(k);
    if (all (abs (keep-candidate) > separation)), keep(end+1,1) = candidate; endif
  endfor
  keep = sort (keep);
  % A single interior peak needs endpoint knots. With two or more peaks,
  % extend the spline to the signal boundaries without adding endpoint data.
  if (numel (keep) < 2), keep = unique ([1; keep; count]); endif
  out = spline (keep, x(keep), (1:count)');
endfunction
