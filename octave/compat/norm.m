function varargout = norm (varargin)
% NORM Measured real floating vector p=0,-1,-2,-Inf subset (MATLAB R2025b).
% Includes 1x0/0x1 vectors, Inf and NaN; [] remains a matrix error.
% Other inputs, options and output counts delegate unchanged to the built-in.
  if (nargout <= 1 && nargin == 2 && isfloat (varargin{1}) ...
      && isreal (varargin{1}) && ismatrix (varargin{1}) ...
      && isnumeric (varargin{2}) && isreal (varargin{2}) ...
      && isscalar (varargin{2}) && any (varargin{2} == [0 -1 -2 -Inf]))
    x = varargin{1}; p = varargin{2};
    vector = rows (x) == 1 || columns (x) == 1;
    if (! vector)
      error ('MATLAB:norm:unknownNorm', ...
             ['When the first input is a matrix, the second input must be 1, 2, inf, or ''fro''.  ' ...
              'When the first input is a vector, the second input must be a positive real-valued scalar, inf, -inf, or ''fro''.']);
    endif
    if (any (isnan (x(:))))
      value = NaN;
    elseif (p == 0)
      if (isempty (x))
        value = 0;
      elseif (isscalar (x))
        value = 1;
      else
        value = Inf;
      endif
    elseif (p == -Inf)
      if (isempty (x))
        value = 0;
      else
        value = min (abs (x(:)));
      endif
    else
      a = abs (x(:));
      if (isempty(a) || all(isinf(a)))
        value = Inf;
      elseif (any(a == 0))
        value = 0;
      else
        % For negative powers the smallest magnitude bounds scaled terms
        % by one. Inf contributes zero; the final value cannot exceed scale.
        scale = min(a);
        value = scale * sum ((a / scale) .^ p) .^ (1 / p);
      endif
    endif
    varargout{1} = cast (value, class (x));
    return;
  endif
  [varargout{1:nargout}] = builtin ('norm', varargin{:});
endfunction
