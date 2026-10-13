function varargout = strrep (varargin)
% STRREP Reject multirow char inputs; other forms use Octave's builtin.
% Character row vectors and cellstr inputs retain their original behavior.
  if (nargin >= 3)
    for k = 1:3
      if (ischar (varargin{k}) && rows (varargin{k}) > 1)
        error ('MATLAB:strrep:MultiRowInput', 'Char inputs must be row vectors.');
      endif
    endfor
  endif
  [varargout{1:nargout}] = builtin ('strrep', varargin{:});
endfunction
