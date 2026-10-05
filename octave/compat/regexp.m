function varargout = regexp (varargin)
  if (nargout == 0 || nargin < 2 || ! ischar (varargin{1}) || (rows (varargin{1}) != 1 && numel (varargin{1}) != 0))
    [varargout{1:nargout}] = builtin ('regexp', varargin{:});
    return;
  endif
% REGEXP Normalize empty numeric/cell output selectors for a char vector.
% Regex syntax, options, matches, and all cell-input forms remain Octave's.
  [varargout{1:nargout}] = builtin ('regexp', varargin{:});
  for k = 1:numel (varargout)
    value = varargout{k};
    if ((isnumeric (value) || iscell (value)) && rows (value) == 1 && columns (value) == 0)
      varargout{k} = reshape (value, 0, 0);
    endif
  endfor
endfunction
