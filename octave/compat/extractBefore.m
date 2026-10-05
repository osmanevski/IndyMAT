function out = extractBefore (str, pat)
% EXTRACTBEFORE Extract text before a literal pattern or numeric position.
% STR may be a char vector or cellstr; PAT/POS is scalar or size-matched.
% Empty patterns are unsupported; numeric positions must be finite integers.
% First-argument strings retain package dispatch and its existing semantics.
% UTF-8 BMP text (including Turkish) is supported. Supplementary Unicode
% characters are rejected because the package indexes code points, not UTF-16.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), out = str.extractBefore (pat); return; endif
  check_text (str);
  if (isnumeric (pat))
    if (! isreal (pat) || any (! isfinite (pat(:))) || any (pat(:) != fix (pat(:))))
      error ('extractBefore: positions must be finite real integers');
    endif
  else
    check_text (pat);
    if (ischar (pat)), pats = {pat}; else, pats = pat; endif
    if (any (cellfun (@isempty, pats(:)))), error ('extractBefore: empty patterns are not supported'); endif
  endif
  tmp = extractBefore (string (str), pat);
  if (ischar (str)), out = char (tmp); else, out = cellstr (tmp); endif
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('extractBefore: STR must be a character vector or cell array of character vectors'); endif
  if (ischar (x)), vals = {x}; else, vals = x; endif
  for k = 1:numel (vals)
    cp = typecast (unicode2native (vals{k}, 'UTF-32LE'), 'uint32');
    if (any (cp > 65535)), error ('extractBefore: supplementary Unicode characters are not supported'); endif
  endfor
endfunction
