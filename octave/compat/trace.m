function varargout = trace (varargin)
% TRACE Numeric/logical matrix form requires a square matrix.
% Accepted forms preserve the original diagonal sum and output class. Other
% types and arities delegate unchanged, including overloaded object methods.
  if (nargin == 1 && (isnumeric (varargin{1}) || islogical (varargin{1})) ...
      && ismatrix (varargin{1}) && rows (varargin{1}) != columns (varargin{1}))
    error ('MATLAB:trace:square', 'Matrix must be square.');
  endif
  original = __mf_original_function__ ('trace');
  [varargout{1:nargout}] = original (varargin{:});
endfunction
