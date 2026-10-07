function varargout = __mf_string_regexp__ (operation, source, expression, varargin)
% __MF_STRING_REGEXP__ ASCII string dispatch for regexp/regexpi.
% Numeric indices, match/split strings, token strings and named-token structs
% retain documented scalar/array layouts. Missing text, non-ASCII positions,
% dynamic MATLAB expressions and engine-specific regexp syntax are not added.
  source_string = isa (source, 'string');
  args = [{source, expression}, varargin];
  for k = 1:numel (args)
    if (isa (args{k}, 'string'))
      if (k != 1 && any (ismissing (args{k})(:)))
        error ('%s: missing string arguments are not supported', operation);
      endif
      if (isscalar (args{k})), args{k} = char (args{k});
      else, args{k} = cellstr (args{k}); endif
    endif
    if (k <= 2)
      text = args{k};
      if (ischar (text)), text = {text}; endif
      if (! iscellstr (text) || any (cellfun (@(s) any (uint8 (s) > 127), text)(:)))
        error ('%s: string dispatch requires ASCII text and expressions', operation);
      endif
    endif
  endfor
  keys = {'start','end','tokenExtents','match','tokens','names','split'};
  selected = {};
  for k = 3:numel (args)
    if (ischar (args{k}) && any (strcmpi (args{k}, keys)))
      selected{end+1} = lower (args{k});
    endif
  endfor
  if (isempty (selected)), selected = keys; endif
  count = max (1, nargout);
  if (count > numel (selected)), error ('%s: too many output arguments', operation); endif
  [varargout{1:count}] = builtin (operation, args{:});
  if (! source_string), return; endif
  array_result = iscell (args{1}) || iscell (args{2});
  once = any (strcmpi (args(3:end), 'once'));
  for k = 1:count
    key = lower (selected{k});
    if (! any (strcmp (key, {'match','split','tokens'}))), continue; endif
    values = varargout{k};
    if (array_result), outer = values; else, outer = {values}; endif
    for j = 1:numel (outer)
      value = outer{j};
      if (strcmp (key, 'tokens') && ! once)
        for t = 1:numel (value), value{t} = string (value{t}); endfor
      else
        value = string (value);
      endif
      if (isempty (value)), value = reshape (value, 0, 0); endif
      outer{j} = value;
    endfor
    if (array_result), varargout{k} = outer; else, varargout{k} = outer{1}; endif
  endfor
endfunction
