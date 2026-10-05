function varargout = lu (varargin)
  if (! (nargin >= 2 && nargout >= 3 && ischar (varargin{end}) && strcmp (varargin{end}, 'vector')))
    [varargout{1:nargout}] = builtin ('lu', varargin{:}); return;
  endif
% LU Return row permutation vectors for the 'vector' form, including sparse LU.
% Factors and all other forms are the original Octave implementation.
  [varargout{1:nargout}] = builtin ('lu', varargin{:});
  varargout{3} = varargout{3}(:).';
  if (nargout >= 4), varargout{4} = varargout{4}(:).'; endif
endfunction
