function out = extractBetween (str, start, stop, varargin)
% EXTRACTBETWEEN Extract spans between patterns or inclusive positions.
% Supports the 'Boundaries' option; non-string input returns cellstr.
% Empty boundary patterns are unsupported; positions must be finite integers.
% First-argument strings retain package dispatch and its existing semantics.
% UTF-8 BMP text (including Turkish) is supported. Supplementary Unicode
% characters are rejected because the package indexes code points, not UTF-16.
% Later string arguments trigger broken package dispatch; use char/cellstr.
  if (isa (str, 'string')), out = str.extractBetween (start, stop, varargin{:}); return; endif
  check_text (str);
  if (isnumeric (start) && isnumeric (stop))
    if (! isreal (start) || ! isreal (stop) || any (! isfinite (start(:))) || any (! isfinite (stop(:))))
      error ('extractBetween: positions must be finite real integers');
    endif
  elseif (! isnumeric (start) && ! isnumeric (stop))
    check_text (start); check_text (stop);
    if (ischar (start)), a = {start}; else, a = start; endif
    if (ischar (stop)), b = {stop}; else, b = stop; endif
    if (any (cellfun (@isempty, [a(:); b(:)]))), error ('extractBetween: empty boundary patterns are not supported'); endif
  else, error ('extractBetween: both boundaries must be numeric or both text'); endif
  if (mod (numel (varargin), 2)), error ('extractBetween: options must be name-value pairs'); endif
  for k = 1:2:numel (varargin)
    if (! ischar (varargin{k}) || ! isrow (varargin{k}) || ! strcmpi (varargin{k}, 'Boundaries') || ! ischar (varargin{k+1}) || ! isrow (varargin{k+1}) || ! any (strcmpi (varargin{k+1}, {'inclusive','exclusive'})))
      error ('extractBetween: only Boundaries inclusive/exclusive is supported');
    endif
  endfor
  out = cellstr (extractBetween (string (str), start, stop, varargin{:}));
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('extractBetween: STR must be a character vector or cell array of character vectors'); endif
  if (ischar (x)), vals = {x}; else, vals = x; endif
  for k = 1:numel (vals)
    cp = typecast (unicode2native (vals{k}, 'UTF-32LE'), 'uint32');
    if (any (cp > 65535)), error ('extractBetween: supplementary Unicode characters are not supported'); endif
  endfor
endfunction
