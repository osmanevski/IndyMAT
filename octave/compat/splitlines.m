function out = splitlines (str)
% SPLITLINES Split char vectors/cellstr at MATLAB newline boundaries.
% Every array element must contain the same number of line boundaries.
% First-argument strings retain package dispatch and its existing semantics.
  if (isa (str, 'string')), out = str.splitlines (); return; endif
  check_text (str);
  out = cellstr (splitlines (string (str)));
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('splitlines: STR must be a character vector or cell array of character vectors'); endif
endfunction
