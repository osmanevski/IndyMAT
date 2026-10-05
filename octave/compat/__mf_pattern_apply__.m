function out = __mf_pattern_apply__ (name, str, pat, varargin)
% Apply scalar ASCII patterns to char vectors, cellstr and explicit strings.
% Nonoverlapping greedy matches; literal replacements; scalar editing text.
% Extract grows the first singleton dimension and requires equal match counts.
% Unicode, missing sources, nullable extraction/editing/counting and pattern
% arrays are rejected. Query options support logical scalar IgnoreCase only.
  if (! isa (pat, 'pattern') || ! isscalar (pat)), error ('%s: expected a scalar pattern', name); endif
  is_string = isa (str, 'string'); is_char = ischar (str);
  if (is_string)
    if (any (ismissing (str)(:))), error ('%s: missing source strings are unsupported for patterns', name); endif
    values = cellstr (str);
  elseif (is_char && (isrow (str) || isempty (str))), values = {str};
  elseif (iscellstr (str)), values = str;
  else, error ('%s: expected a character vector, cellstr or string array', name); endif
  if (any (cellfun (@(s) (! isrow (s) && ! isempty (s)) || any (uint8 (s) > 127), values(:))))
    error ('%s: pattern matching supports ASCII text only', name);
  endif
  query = any (strcmp (name, {'contains', 'startsWith', 'endsWith', 'count'}));
  edit = any (strcmp (name, {'replace', 'insertBefore', 'insertAfter'}));
  expression = pat.expression; options = {};
  if (query)
    if (mod (numel (varargin), 2)), error ('%s: options must be name/value pairs', name); endif
    for k = 1:2:numel (varargin)
      key = varargin{k}; flag = varargin{k+1};
      if (isa (key, 'string') && isscalar (key)), key = char (key); endif
      if (! ischar (key) || ! strcmpi (key, 'IgnoreCase') || ! islogical (flag) || ! isscalar (flag))
        error ('%s: only logical scalar IgnoreCase is supported', name);
      endif
      if (flag), options = {'ignorecase'}; else, options = {}; endif
    endfor
    if (strcmp (name, 'startsWith')), expression = ['^(?:' expression ')']; endif
    if (strcmp (name, 'endsWith')), expression = ['(?:' expression ')\z']; endif
    if (strcmp (name, 'count')), out = zeros (size (values)); else, out = false (size (values)); endif
  elseif (edit)
    if (numel (varargin) != 1), error ('%s: expected scalar replacement/insertion text', name); endif
    new = varargin{1};
    if (isa (new, 'string') && isscalar (new) && ! ismissing (new)), new = char (new); endif
    if (! ischar (new) || (! isrow (new) && ! isempty (new)) || any (uint8 (new) > 127))
      error ('%s: only scalar ASCII replacement/insertion text is supported', name);
    endif
    out = values;
  elseif (! strcmp (name, 'extract') || ! isempty (varargin))
    error ('%s: unsupported pattern operation or options', name);
  endif
  if (pat.nullable && (! query || strcmp (name, 'count')))
    error ('%s: nullable patterns are unsupported for extraction, editing and counting', name);
  endif
  matches = cell (size (values)); counts = zeros (size (values));
  for k = 1:numel (values)
    s = values{k};
    if (query && ! strcmp (name, 'count'))
      out(k) = ! isempty (builtin ('regexp', s, expression, options{:}, 'emptymatch', 'once'));
      continue;
    endif
    [starts, stops] = builtin ('regexp', s, expression, options{:});
    counts(k) = numel (starts);
    if (query), out(k) = counts(k);
    elseif (edit)
      pieces = {}; last = 1;
      for j = 1:numel (starts)
        pieces{end+1} = s(last:starts(j)-1);
        switch name
          case 'replace', pieces{end+1} = new;
          case 'insertBefore', pieces(end+1:end+2) = {new, s(starts(j):stops(j))};
          case 'insertAfter', pieces(end+1:end+2) = {s(starts(j):stops(j)), new};
        endswitch
        last = stops(j) + 1;
      endfor
      pieces{end+1} = s(last:end);
      out{k} = ['', pieces{:}];
      if (isempty (out{k})), out{k} = ''; endif
    else
      matches{k} = arrayfun (@(a,b) s(a:b), starts, stops, 'UniformOutput', false);
    endif
  endfor
  if (strcmp (name, 'extract'))
    if (! isempty (counts) && any (counts(:) != counts(1)))
      error ('extract: all source elements must have the same number of matches');
    endif
    number = 0; if (! isempty (counts)), number = counts(1); endif
    shape = size (values); dim = find (shape == 1, 1);
    if (isempty (dim)), dim = numel (shape) + 1; shape(dim) = 1; endif
    shape(dim) = number; out = cell (shape);
    % One slab per match, in the first singleton dimension of the source.
    subs = repmat ({':'}, 1, numel (shape));
    for j = 1:number
      slab = cellfun (@(m) m{j}, matches, 'UniformOutput', false);
      subs{dim} = j; out(subs{:}) = slab;
    endfor
  endif
  if (! query)
    if (is_string), out = string (out);
    elseif (is_char && edit), out = out{1}; endif
  endif
endfunction
