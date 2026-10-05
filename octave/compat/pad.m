function out = pad (str, varargin)
% PAD Pad char vectors/cellstr to a requested or maximum text length.
% Supports SIDE 'left', 'right', or 'both' and one pad character.
% First-argument strings retain package dispatch and its existing semantics.
% UTF-8 BMP text (including Turkish) is supported. Supplementary Unicode
% characters are rejected because the package indexes code points, not UTF-16.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), out = str.pad (varargin{:}); return; endif
  check_text (str);
  for k = 1:numel (varargin)
    if (k == 1 && isnumeric (varargin{k}))
      n = varargin{k};
      if (! isscalar (n) || ! isreal (n) || ! isfinite (n) || n < 0 || n != fix (n)), error ('pad: length must be a finite nonnegative integer scalar'); endif
    else
      if (! ischar (varargin{k})), error ('pad: SIDE and PADCHAR must be character vectors'); endif
      check_text (varargin{k});
    endif
  endfor
  tmp = pad (string (str), varargin{:});
  if (ischar (str)), out = char (tmp); else, out = cellstr (tmp); endif
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('pad: STR must be a character vector or cell array of character vectors'); endif
  if (ischar (x)), vals = {x}; else, vals = x; endif
  for k = 1:numel (vals)
    cp = typecast (unicode2native (vals{k}, 'UTF-32LE'), 'uint32');
    if (any (cp > 65535)), error ('pad: supplementary Unicode characters are not supported'); endif
  endfor
endfunction
