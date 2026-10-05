function TF = endsWith (str, pattern, varargin)
% ENDSWITH MATLAB-compatible suffix test for char vectors and cellstr.
% Supports multiple literal patterns and the 'IgnoreCase' option.
% UTF-8 literals are supported; IgnoreCase=true is restricted to ASCII.
% First-argument strings retain package dispatch and its existing semantics.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), error ('endsWith: string inputs must dispatch to the datatypes class method'); endif
  check_text (str, 'STR');
  TF = edge_test (str, pattern, varargin);
endfunction

function check_text (x, name)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('endsWith: %s must be a character vector or cell array of character vectors', name); endif
endfunction

function TF = edge_test (str, pattern, args)
  if (ischar (pattern) && (isrow (pattern) || isempty (pattern))), pats = {pattern};
  elseif (iscell (pattern) && all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), pattern(:)))), pats = pattern(:).';
  else, error ('endsWith: PATTERN must be a character vector or cell array of character vectors'); endif
  ignoreCase = false;
  if (mod (numel (args), 2)), error ('endsWith: name-value arguments must be in pairs'); endif
  for k = 1:2:numel (args)
    if (! ischar (args{k}) || ! strcmpi (args{k}, 'IgnoreCase')), error ('endsWith: unsupported option'); endif
    ignoreCase = args{k+1};
    if (! islogical (ignoreCase) || ! isscalar (ignoreCase)), error ("endsWith: 'IgnoreCase' must be a logical scalar"); endif
  endfor
  if (ischar (str)), vals = {str}; else, vals = str; endif
  if (ignoreCase && (any (cellfun (@(v) any (uint8 (v) > 127), vals(:))) || any (cellfun (@(v) any (uint8 (v) > 127), pats(:)))))
    error ('endsWith: non-ASCII IgnoreCase is not supported');
  endif
  TF = false (size (vals));
  for i = 1:numel (vals)
    s = vals{i}; pp = pats;
    if (ignoreCase), s = lower (s); pp = cellfun (@lower, pp, 'UniformOutput', false); endif
    for p = 1:numel (pp)
      n = numel (pp{p});
      if (n == 0 || (n <= numel (s) && strcmp (s(end-n+1:end), pp{p}))), TF(i) = true; break; endif
    endfor
  endfor
  if (ischar (str)), TF = TF(1); endif
endfunction
