function varargout = bitcmp (varargin)
% BITCMP Numeric bit-width is an Octave extension, not a MATLAB assumed type.
  if (nargin == 2 && (isnumeric (varargin{2}) || islogical (varargin{2})))
    error ('MATLAB:bitcmp:unknownTypeString', 'ASSUMEDTYPE must be an integer type name.');
  endif
  original = __mf_original_function__ ('bitcmp');
  [varargout{1:nargout}] = original (varargin{:});
endfunction
