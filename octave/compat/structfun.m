function varargout = structfun (varargin)
% STRUCTFUN Require scalar structures, retaining the original field mapping.
% Empty and nonempty structure arrays are rejected before invoking callbacks.
  if (nargin >= 2 && isstruct (varargin{2}) && ! isscalar (varargin{2}))
    error ('MATLAB:structfun:NonScalar', 'Inputs to STRUCTFUN must be scalar structures.');
  endif
  persistent original;
  if (isempty (original)), original = __mf_original_function__ ('structfun'); endif
  [varargout{1:nargout}] = original (varargin{:});
endfunction
