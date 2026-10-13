function varargout = cummax (varargin)
% CUMMAX MATLAB has one output; all one-output forms delegate unchanged.
  if (nargout > 1)
    error ('MATLAB:maxlhs', 'Too many output arguments.');
  endif
  [varargout{1:nargout}] = builtin ('cummax', varargin{:});
endfunction
