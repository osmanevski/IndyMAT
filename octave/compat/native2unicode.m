function out = native2unicode (varargin)
  if (nargin < 1 || nargin > 2 || ! isnumeric (varargin{1}) || ! isempty (varargin{1}) || ndims (varargin{1}) != 2 || (! isequal (size (varargin{1}), [0 0]) && ! any (size (varargin{1}) == 1)))
    persistent original;
    if (isempty (original)), original = __mf_text_original__ ('native2unicode'); endif
    out = original (varargin{:});
    return;
  endif
% NATIVE2UNICODE Accept empty numeric byte arrays; other forms delegate.
% Nonempty output still uses Octave UTF-8, not MATLAB UTF-16 char storage.
  codepage = '';
  if (nargin == 2), codepage = varargin{2}; endif
  if (! ischar (codepage) || (! isrow (codepage) && ! isempty (codepage)))
    error ('native2unicode: CODEPAGE must be a character vector');
  endif
  __native2unicode__ (uint8 (0), codepage);
  out = '';
endfunction
