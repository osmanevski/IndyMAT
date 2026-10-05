function out = jsonencode (varargin)
  if (nargin < 3 || mod (nargin, 2) != 1 || ! any (cellfun (@(v) ischar (v) && strcmpi (v, 'PrettyPrint'), varargin(2:2:end)) & cellfun (@(v) islogical (v) && isscalar (v) && v, varargin(3:2:end))))
    out = builtin ('jsonencode', varargin{:});
    return;
  endif
% JSONENCODE PrettyPrint on builds without RapidJSON's pretty writer.
% Encoding and validation remain builtin; indentation is applied to JSON
% punctuation outside quoted strings. Exact MATLAB whitespace is not promised.
  pretty = false;
  args = varargin;
  for k = 2:2:numel (args)
    if (ischar (args{k}) && strcmpi (args{k}, 'PrettyPrint'))
      value = args{k+1};
      if (! islogical (value) || ! isscalar (value))
        out = builtin ('jsonencode', varargin{:});
        return;
      endif
      pretty = value;
      args{k+1} = false;
    endif
  endfor
  out = builtin ('jsonencode', args{:});
  if (! pretty), return; endif
  pieces = cell (1, numel (out));
  depth = 0;
  quoted = false;
  escaped = false;
  for k = 1:numel (out)
    c = out(k);
    if (quoted)
      pieces{k} = c;
      if (escaped), escaped = false;
      elseif (c == char (92)), escaped = true;
      elseif (c == char (34)), quoted = false;
      endif
    elseif (c == char (34))
      quoted = true;
      pieces{k} = c;
    elseif (c == '{' || c == '[')
      depth += 1;
      pieces{k} = c;
      if (k < numel (out) && out(k+1) != '}' && out(k+1) != ']')
        pieces{k} = [c sprintf('\n') repmat(' ', 1, 2*depth)];
      endif
    elseif (c == '}' || c == ']')
      depth -= 1;
      pieces{k} = c;
      if (k > 1 && out(k-1) != '{' && out(k-1) != '[')
        pieces{k} = [sprintf('\n') repmat(' ', 1, 2*depth) c];
      endif
    elseif (c == ',')
      pieces{k} = [c sprintf('\n') repmat(' ', 1, 2*depth)];
    elseif (c == ':')
      pieces{k} = ': ';
    else
      pieces{k} = c;
    endif
  endfor
  out = [pieces{:}];
endfunction
