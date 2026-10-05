function out = erase (str, match)
% ERASE MATLAB-compatible literal deletion for char vectors and cellstr.
% MATCH may be a character vector or cellstr.
% Matches are non-overlapping UTF-8 literals; empty patterns are unsupported.
% First-argument strings retain package dispatch and its existing semantics.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), out = str.erase (match); return; endif
  check_text (str, 'STR');
  out = replace (str, match, '');
endfunction

function check_text (x, name)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('erase: %s must be a character vector or cell array of character vectors', name); endif
endfunction
