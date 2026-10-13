function varargout = strncmpi (varargin)
% STRNCMPI Reject vector match lengths; supported forms use Octave's builtin.
% MATLAB requires a scalar character count, including for cellstr inputs.
% This measured subset adds no validation of scalar counts or text types.
  if (nargin == 3 && (isnumeric (varargin{3}) || islogical (varargin{3})) && ! isscalar (varargin{3}))
    error ('MATLAB:strcmp:SizeMustBeInt', 'The number of characters to match must be an integer.');
  endif
  [varargout{1:nargout}] = builtin ('strncmpi', varargin{:});
endfunction
