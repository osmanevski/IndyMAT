function out = reverse (str)
% REVERSE Reverse each char vector while preserving char/cellstr type.
% First-argument strings retain package dispatch and its existing semantics.
% UTF-8 BMP text (including Turkish) is supported. Supplementary Unicode
% characters are rejected because the package indexes code points, not UTF-16.
  if (isa (str, 'string')), out = str.reverse (); return; endif
  check_text (str);
  tmp = reverse (string (str));
  if (ischar (str)), out = char (tmp); else, out = cellstr (tmp); endif
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('reverse: STR must be a character vector or cell array of character vectors'); endif
  if (ischar (x)), vals = {x}; else, vals = x; endif
  for k = 1:numel (vals)
    cp = typecast (unicode2native (vals{k}, 'UTF-32LE'), 'uint32');
    if (any (cp > 65535)), error ('reverse: supplementary Unicode characters are not supported'); endif
  endfor
endfunction
