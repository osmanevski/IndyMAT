function out = replace (str, old, new)
% REPLACE MATLAB-compatible literal replacement for char vectors and cellstr.
% OLD may contain several substrings; NEW is scalar or paired with OLD.
% UTF-8 literal matching is non-overlapping; empty OLD is unsupported.
% First-argument strings retain package dispatch and its existing semantics.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), out = str.replace (old, new); return; endif
  check_text (str, 'STR');
  check_text (old, 'OLD'); check_text (new, 'NEW');
  if (ischar (old)), old = {old}; endif
  if (ischar (new)), new = {new}; endif
  if (any (cellfun (@isempty, old(:)))), error ('replace: empty OLD patterns are not supported'); endif
  if (isscalar (new)), new = repmat (new, size (old));
  elseif (! isequal (size (old), size (new))), error ('replace: NEW must be scalar or the same size as OLD'); endif
  if (ischar (str)), vals = {str}; else, vals = str; endif
  out = vals;
  for k = 1:numel (vals)
    s = vals{k}; i = 1; last = 1; pieces = {};
    while (i <= numel (s))
      hit = 0;
      for p = 1:numel (old)
        n = numel (old{p});
        if (i+n-1 <= numel (s) && strcmp (s(i:i+n-1), old{p}))
          pieces(end+1:end+2) = {s(last:i-1), new{p}};
          hit = n; break;
        endif
      endfor
      if (hit), i += hit; last = i; else, i += 1; endif
    endwhile
    pieces{end+1} = s(last:end);
    out{k} = ['', pieces{:}];
    if (isempty (out{k})), out{k} = ''; endif
  endfor
  if (ischar (str)), out = out{1}; endif
endfunction

function check_text (x, name)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('replace: %s must be a character vector or cell array of character vectors', name); endif
endfunction
