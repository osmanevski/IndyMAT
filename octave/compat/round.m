function out = round (x, varargin)
  if (nargin == 1), out = builtin ('round', x); return; endif
% ROUND Decimal/significant digit subset; one argument delegates immediately.
% Class methods retain Octave dispatch. Extended forms accept single/double.
% Integers with digits are rejected, as specified by MATLAB R2025b's
% MATLAB:round:NonfloatMultipleArgs diagnostic. One argument keeps its class.
% Ambiguous decimal ties and extreme scaling are
% rejected pending MATLAB measurement; no tolerance-based tie approximation.
  if (nargin < 2 || nargin > 3), error ('round: expected one to three arguments'); endif
  if (! isfloat (x)), error ('round: extended forms require double or single input'); endif
  n = varargin{1}; mode = 'decimals';
  if (! isnumeric (n) || ! isreal (n) || ! isscalar (n) || ! isfinite (n) || fix (n) != n)
    error ('round: digits must be a finite integer scalar');
  endif
  if (nargin == 3), mode = varargin{2}; endif
  if (! ischar (mode) || ! any (strcmp (mode, {'decimals', 'significant'})))
    error ('round: mode must be decimals or significant');
  endif
  if (strcmp (mode, 'significant') && n <= 0), error ('round: significant digits must be positive'); endif
  if (abs (double (n)) > 300), error ('round: digit counts outside [-300,300] are unsupported'); endif
  if (isreal (x)), out = component (x, double (n), mode);
  else, out = complex (component (real (x), double (n), mode), component (imag (x), double (n), mode)); endif
endfunction

function out = component (x, n, mode)
  original = class (x); work = double (x); digits = repmat (n, size (work));
  active = isfinite (work) & work != 0;
  if (strcmp (mode, 'significant'))
    magnitude = abs (work(active)); exponent = floor (log10 (magnitude));
    exponent(magnitude < 10 .^ exponent) -= 1;
    exponent(magnitude >= 10 .^ (exponent + 1)) += 1;
    digits(active) = n - 1 - exponent;
  endif
  if (any (abs (digits(active)) > 300)), error ('round: extreme decimal scaling is unsupported'); endif
  scale = 10 .^ digits; scaled = work .* scale;
  if (any (active(:) & (scaled(:) == 0 | ! isfinite (scaled(:)))))
    error ('round: scaling overflow or underflow is unsupported');
  endif
  half = abs (scaled) - floor (abs (scaled));
  tolerance = 4 .* eps (abs (scaled));
  if (isa (x, 'single')), tolerance = max (tolerance, 2 .* double (eps (abs (x))) .* scale); endif
  near = active & abs (half - 0.5) <= tolerance & abs (scaled) < flintmax ();
  % A positive-decimal half is exactly representable only when its odd
  % numerator is divisible by 5^digits. Negative-decimal halves are integers.
  exact = half == 0.5 & (digits <= 0 | mod (2 .* floor (abs (scaled)) + 1, 5 .^ max (digits, 0)) == 0);
  if (any (near(:) & ! exact(:)))
    error ('round: decimal tie requires MATLAB measurement for this input');
  endif
  out = work;
  changing = active & abs (scaled) < flintmax ();
  out(changing) = builtin ('round', scaled(changing)) ./ scale(changing);
  out = cast (out, original);
endfunction
