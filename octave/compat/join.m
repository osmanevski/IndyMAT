function out = join (str, varargin)
% JOIN Combine cellstr elements with a literal delimiter along one dimension.
% Character input is treated as one text element; output is always cellstr.
% UTF-8 delimiters (including '') are supported; empty delimiter arrays are not.
% First-argument strings retain package dispatch and its existing semantics.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), out = str.join (varargin{:}); return; endif
  check_text (str);
  if (numel (varargin) > 2), error ('join: too many input arguments'); endif
  if (! isempty (varargin))
    if (isnumeric (varargin{1}) && numel (varargin) == 1)
      check_dim (varargin{1});
    else
      check_text (varargin{1});
      % The package confuses '' with an omitted delimiter. A scalar cell
      % carries the explicit empty delimiter without changing its meaning.
      if (ischar (varargin{1})), varargin{1} = {varargin{1}};
      elseif (isempty (varargin{1})), error ('join: empty delimiter arrays are not supported'); endif
    endif
    if (numel (varargin) == 2), check_dim (varargin{2}); endif
  endif
  out = cellstr (join (string (str), varargin{:}));
endfunction

function check_dim (d)
  if (! isnumeric (d) || ! isscalar (d) || ! isreal (d) || ! isfinite (d) || d < 1 || d != fix (d))
    error ('join: DIM must be a finite positive integer scalar');
  endif
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('join: STR must be a character vector or cell array of character vectors'); endif
endfunction
