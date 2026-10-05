function varargout = hamming (varargin)
  if (nargin < 1 || ! isnumeric (varargin{1}) || ! isscalar (varargin{1}) || varargin{1} != 0), [varargout{1:nargout}] = original (varargin{:}); return; endif
% HAMMING Empty windows return a 0-by-1 double column, as in MATLAB.
% Every nonzero length and all original call forms delegate unchanged.
  if (nargin > 2 || nargout > 1), error ('hamming: expected length and optional window convention'); endif
  if (nargin == 2 && (! ischar (varargin{2}) || ! isrow (varargin{2}) || ! any (strcmpi (varargin{2}, {'periodic', 'symmetric'}))))
    error ('hamming: option must be periodic or symmetric');
  endif
  varargout{1} = zeros (0,1);
endfunction

function varargout = original (varargin)
  persistent handle;
  if (isempty (handle)), handle = __mf_toolbox_original__ ('hamming'); endif
  [varargout{1:nargout}] = handle (varargin{:});
endfunction
