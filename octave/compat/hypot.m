function varargout = hypot (varargin)
% HYPOT Two real floating inputs propagate NaN even when paired with Inf.
% The native function retains scaling, broadcasting, class and validation.
% Other call forms (including Octave's multi-input extension) delegate unchanged.
  if (nargout <= 1 && nargin == 2 && isfloat (varargin{1}) ...
      && isfloat (varargin{2}) && isreal (varargin{1}) && isreal (varargin{2}))
    varargout{1} = builtin ('hypot', varargin{:});
    missing = isnan (varargin{1}) | isnan (varargin{2});
    varargout{1}(missing) = NaN;
  else
    [varargout{1:nargout}] = builtin ('hypot', varargin{:});
  endif
endfunction
