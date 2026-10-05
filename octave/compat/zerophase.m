function [h, w, phi] = zerophase (b, a, n)
% ZEROPHASE Return the zero-phase response of a digital filter.
% Supports finite real symmetric FIR numerators, scalar denominators, and
% integer frequency-point counts. Two-input form treats the second input as a.
  if (nargin < 1 || nargin > 3 || ! isnumeric (b) || ! isvector (b) || isempty (b) || ! isreal (b) || ! islinphase (b) || norm (b(:).' - fliplr (b(:).'), inf) > 64 * eps * max (1, norm (b(:).', inf)))
    error ('zerophase: b must be a real symmetric FIR vector');
  endif
  if (nargin == 1), a = 1; n = 512;
  elseif (nargin == 2), a = a; n = 512;
  endif
  if (! isnumeric (a) || ! isscalar (a) || ! isfinite (a) || a == 0)
    error ('zerophase: a must be a nonzero finite scalar');
  endif
  if (! isnumeric (n) || ! isscalar (n) || ! isfinite (n) || n < 2 || fix (n) != n)
    error ('zerophase: frequency-point count must be an integer of at least two');
  endif
  [response, w] = freqz (b, a, n);
  phi = -w * (numel (b) - 1) / 2;
  h = real (response .* exp (-1i * phi));
endfunction
