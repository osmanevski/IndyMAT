function out = count (str, pattern, varargin)
% COUNT MATLAB-compatible literal occurrence count for char vectors/cellstr.
% Supports multiple literal patterns and the 'IgnoreCase' option.
% UTF-8 literals are supported; IgnoreCase=true is restricted to ASCII.
% Empty patterns are unsupported and raise an error.
% First-argument strings retain package dispatch and its existing semantics.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), out = str.count (pattern, varargin{:}); return; endif
  check_text (str, 'STR');
  pats = normalize_patterns (pattern);
  if (any (cellfun (@isempty, pats(:)))), error ('count: empty patterns are not supported'); endif
  ignoreCase = parse_options (varargin);
  if (ischar (str)), vals = {str}; else, vals = str; endif
  if (ignoreCase && (any (cellfun (@(v) any (uint8 (v) > 127), vals(:))) || any (cellfun (@(v) any (uint8 (v) > 127), pats(:)))))
    error ('count: non-ASCII IgnoreCase is not supported');
  endif
  out = zeros (size (vals));
  for k = 1:numel (vals)
    s = vals{k}; pp = pats;
    if (ignoreCase), s = lower (s); pp = cellfun (@lower, pp, 'UniformOutput', false); endif
    i = 1;
    while (i <= numel (s))
      hit = 0;
      for p = 1:numel (pp)
        n = numel (pp{p});
        if (n > 0 && i + n - 1 <= numel (s) && strcmp (s(i:i+n-1), pp{p}))
          hit = n; out(k) += 1; break;
        endif
      endfor
      if (hit), i += hit; else, i += 1; endif
    endwhile
  endfor
  if (ischar (str)), out = out(1); endif
endfunction

function check_text (x, name)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('count: %s must be a character vector or cell array of character vectors', name); endif
endfunction

function pats = normalize_patterns (pattern)
  if (isa (pattern, 'string')), pats = cellstr (pattern);
  elseif (ischar (pattern) && (isrow (pattern) || isempty (pattern))), pats = {pattern};
  elseif (iscell (pattern) && all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), pattern(:)))), pats = pattern(:).';
  else, error ('count: PATTERN must be a character vector, string array, or cell array of character vectors');
  endif
endfunction

function value = parse_options (args)
  value = false;
  if (mod (numel (args), 2)), error ('count: name-value arguments must be in pairs'); endif
  for k = 1:2:numel (args)
    name = args{k};
    if (isa (name, 'string') && isscalar (name)), name = char (name); endif
    if (! ischar (name) || ! strcmpi (name, 'IgnoreCase')), error ('count: unsupported option'); endif
    value = args{k+1};
    if (! islogical (value) || ! isscalar (value)), error ("count: 'IgnoreCase' must be a logical scalar"); endif
  endfor
endfunction
