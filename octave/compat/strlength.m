function out = strlength (str)
% STRLENGTH Return text lengths for a char vector or cellstr array.
% Counts UTF-16 code units, as MATLAB does, after decoding Octave UTF-8.
% First-argument strings retain package dispatch and its existing semantics.
  if (isa (str, 'string')), out = str.strlength (); return; endif
  check_text (str);
  if (ischar (str)), vals = {str}; else, vals = str; endif
  out = cellfun (@(s) numel (unicode2native (s, 'UTF-16LE')) / 2, vals);
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('strlength: STR must be a character vector or cell array of character vectors'); endif
endfunction
