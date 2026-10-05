function varargout = bandwidth (varargin)
  if (nargin == 0 || ! isa (varargin{1}, 'lti')), [varargout{1:nargout}] = original (varargin{:}); return; endif
% BANDWIDTH First attenuation crossing for continuous real rational SISO LTI.
% Default drop is -3 dB, rather than the half-power approximation. Discrete,
% FRD, complex and MIMO models remain unsupported. Numeric matrix calls retain
% Octave's original lower/upper matrix-bandwidth behavior.
  if (nargin > 2 || nargout > 1), error ('bandwidth: expected one model and optional dB drop'); endif
  sys = varargin{1}; drop = -3;
  if (nargin == 2), drop = varargin{2}; endif
  if (! isnumeric (drop) || ! isscalar (drop) || ! isreal (drop) || ! isfinite (drop) || drop >= 0)
    error ('bandwidth: dB drop must be a negative finite scalar');
  endif
  if (! issiso (sys) || ! isct (sys) || isa (sys, 'frd'))
    error ('bandwidth: only continuous rational SISO models are supported');
  endif
  [num, den] = tfdata (sys, 'v');
  if (! isreal (num) || ! isreal (den))
    error ('bandwidth: complex coefficients are unsupported');
  endif
  gain = abs (dcgain (sys));
  if (! isfinite (gain)), result = NaN;
  elseif (gain == 0), result = 0;
  else
    target = gain * 10^(drop/20);
    np = (numel (num)-1):-1:0; dp = (numel (den)-1):-1:0;
    a = real (conv (num .* (1i).^np, num .* (-1i).^np));
    b = real (conv (den .* (1i).^dp, den .* (-1i).^dp));
    equation = a - target^2*b;
    solutions = roots (equation(1:2:end));
    candidates = sqrt (real (solutions(abs (imag (solutions)) <= 100*eps .* max (1,abs (solutions)) & real (solutions) > 0)));
    if (isempty (candidates)), result = Inf; else, result = min (candidates); endif
  endif
  varargout{1} = result;
endfunction

function varargout = original (varargin)
  persistent handle;
  if (isempty (handle)), handle = __mf_toolbox_original__ ('bandwidth'); endif
  [varargout{1:nargout}] = handle (varargin{:});
endfunction
