function out = blanks (varargin)
  if (nargin != 1 || (! isnumeric (varargin{1}) && ! islogical (varargin{1})) || ! isscalar (varargin{1}) || varargin{1} != 0)
    persistent original;
    if (isempty (original)), original = __mf_text_original__ ('blanks'); endif
    out = original (varargin{:});
    return;
  endif
% BLANKS Preserve MATLAB's 1-by-0 char shape at zero; other forms delegate.
  out = reshape ('', 1, 0);
endfunction
