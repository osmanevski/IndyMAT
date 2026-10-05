function [out, match] = split (str, varargin)
% SPLIT Split char vectors/cellstr on literal delimiters without collapsing.
% Supports multiple delimiters, optional DIM, and a delimiter output.
% Delimiters are UTF-8 literals; empty delimiters are unsupported.
% First-argument strings retain package dispatch and its existing semantics.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string'))
    if (nargout > 1), [out, match] = str.split (varargin{:}); else, out = str.split (varargin{:}); endif
    return;
  endif
  check_text (str);
  if (numel (varargin) > 2), error ('split: too many input arguments'); endif
  if (! isempty (varargin))
    check_text (varargin{1});
    if (ischar (varargin{1})), d = {varargin{1}}; else, d = varargin{1}; endif
    if (any (cellfun (@isempty, d(:)))), error ('split: empty delimiters are not supported'); endif
  endif
  if (numel (varargin) == 2)
    d = varargin{2};
    if (! isnumeric (d) || ! isscalar (d) || ! isreal (d) || ! isfinite (d) || d < 1 || d != fix (d))
      error ('split: DIM must be a finite positive integer scalar');
    endif
  endif
  if (nargout > 1)
    [tmp, mt] = split (string (str), varargin{:});
    out = cellstr (tmp); match = cellstr (mt);
  else
    out = cellstr (split (string (str), varargin{:}));
  endif
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('split: STR must be a character vector or cell array of character vectors'); endif
endfunction
