function TF = contains (str, pattern, varargin)
% CONTAINS MATLAB-compatible literal search for char vectors and cellstr.
% Supports multiple literal patterns and the 'IgnoreCase' option.
% UTF-8 literals are supported; IgnoreCase=true is restricted to ASCII.
% First-argument strings retain package dispatch and its existing semantics.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), error ('contains: string inputs must dispatch to the datatypes class method'); endif
  check_text (str, 'STR');
  pats = normalize_patterns (pattern);
  ignoreCase = parse_options (varargin);
  if (ischar (str)), vals = {str}; else, vals = str; endif
  if (ignoreCase && (any (cellfun (@(v) any (uint8 (v) > 127), vals(:))) || any (cellfun (@(v) any (uint8 (v) > 127), pats(:)))))
    error ('contains: non-ASCII IgnoreCase is not supported');
  endif
  TF = false (size (vals));
  for k = 1:numel (vals)
    s = vals{k}; pp = pats;
    if (ignoreCase), s = lower (s); pp = cellfun (@lower, pp, 'UniformOutput', false); endif
    for p = 1:numel (pp)
      if (isempty (pp{p}) || ! isempty (strfind (s, pp{p}))), TF(k) = true; break; endif
    endfor
  endfor
  if (ischar (str)), TF = TF(1); endif
endfunction

function check_text (x, name)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('contains: %s must be a character vector or cell array of character vectors', name); endif
endfunction

function pats = normalize_patterns (pattern)
  if (ischar (pattern) && (isrow (pattern) || isempty (pattern))), pats = {pattern};
  elseif (iscell (pattern) && all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), pattern(:)))), pats = pattern(:).';
  else, error ('contains: PATTERN must be a character vector or cell array of character vectors');
  endif
endfunction

function value = parse_options (args)
  value = false;
  if (mod (numel (args), 2)), error ('contains: name-value arguments must be in pairs'); endif
  for k = 1:2:numel (args)
    if (! ischar (args{k}) || ! strcmpi (args{k}, 'IgnoreCase')), error ('contains: unsupported option'); endif
    value = args{k+1};
    if (! islogical (value) || ! isscalar (value)), error ("contains: 'IgnoreCase' must be a logical scalar"); endif
  endfor
endfunction
