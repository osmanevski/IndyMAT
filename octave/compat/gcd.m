function varargout = gcd (varargin)
% GCD Two numeric arrays must have matching sizes unless one is scalar.
% The builtin computes the gcd and optional Bezout coefficients unchanged.
% Other types/arities retain native validation and extension behaviour.
  if (nargin == 2 && isnumeric (varargin{1}) && isnumeric (varargin{2}) ...
      && ! isscalar (varargin{1}) && ! isscalar (varargin{2}) ...
      && ! isequal (size (varargin{1}), size (varargin{2})))
    error ('MATLAB:gcd:InputSizeMismatch', 'Inputs must be the same size.');
  endif
  [varargout{1:nargout}] = builtin ('gcd', varargin{:});
endfunction
