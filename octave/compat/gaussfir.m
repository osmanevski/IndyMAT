function b = gaussfir (bt, nt = 3, of = 2)
% GAUSSFIR Unit-DC-gain Gaussian FIR pulse shaping filter.
% Supports double inputs: positive finite BT, positive integer NT and OF.
% NT is the one-sided span in symbols; defaults are NT=3 and OF=2.
% Fractional spans and additional arguments/outputs are unsupported.
  if (nargin < 1 || nargin > 3), error ('gaussfir: expected BT and optional NT, OF'); endif
  if (! isa (bt, 'double') || ! isreal (bt) || ! isscalar (bt) || ! isfinite (bt) || bt <= 0)
    error ('gaussfir: BT must be a positive finite real double scalar');
  endif
  for value = {nt, of}
    v = value{1};
    if (! isa (v, 'double') || ! isreal (v) || ! isscalar (v) || ! isfinite (v) || v < 1 || fix (v) != v)
      error ('gaussfir: NT and OF must be positive finite integer double scalars');
    endif
  endfor
  span = double (nt) * double (of);
  if (! isfinite (span) || span > 1e6), error ('gaussfir: filter span exceeds the supported allocation limit'); endif
  t = (-span:span) / double (of);
  % A Gaussian has power down by 3 dB at BT cycles per symbol.
  % Normalize the sampled, truncated impulse response, not its continuous area.
  b = exp (-2 * (pi * (double (bt) * t)).^2 / log (2));
  b /= sum (b);
endfunction
