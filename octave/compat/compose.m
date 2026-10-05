function out = compose (formatSpec, varargin)
% COMPOSE Format real numeric/logical arrays and character vectors.
% Character/cellstr format text returns a cell array of character vectors.
% First-argument strings retain package dispatch and its existing semantics.
% Later string arguments trigger broken package dispatch; use char/cellstr.
% Supports fixed-width diouxXeEfgGcs operators, %% and basic backslash escapes.
% Rejects dynamic widths, positional/subtype operators, complex/N-D data,
% numeric-to-text coercion, and width/precision formatting of non-ASCII text.
  if (isa (formatSpec, 'string')), out = formatSpec.compose (varargin{:}); return; endif
  check_text (formatSpec);
  if (isempty (varargin))
    if (ischar (formatSpec)), out = {formatSpec}; else, out = formatSpec; endif
    for k = 1:numel (out), out{k} = render (tokenize (out{k}), {}); endfor
    return;
  endif
  if (! ischar (formatSpec)), error ('compose: formatting requires a character vector FORMAT'); endif
  tok = tokenize (formatSpec);
  nops = sum (cellfun (@(t) t.op, tok));
  if (nops == 0), error ('compose: data requires at least one supported format operator'); endif
  data = cell (size (varargin)); rows = ones (size (varargin));
  for k = 1:numel (varargin)
    a = varargin{k};
    if (ischar (a))
      if (! isrow (a) && ! isempty (a)), error ('compose: char data must be a row vector'); endif
      data{k} = {a};
    elseif ((isnumeric (a) || islogical (a)) && isreal (a) && ndims (a) == 2)
      data{k} = num2cell (a);
    else, error ('compose: data must be real 2-D numeric/logical arrays or char row vectors'); endif
    rows(k) = size (data{k}, 1);
  endfor
  nr = unique (rows(rows != 1));
  if (isempty (nr)), nr = 1; endif
  if (numel (nr) != 1), error ('compose: incompatible data row counts'); endif
  for k = 1:numel (data)
    if (rows(k) == 1), data{k} = repmat (data{k}, nr, 1); endif
  endfor
  vals = [data{:}]; nv = columns (vals);
  out = cell (nr, ceil (nv/nops));
  for r = 1:nr
    for c = 1:columns (out)
      out{r,c} = render (tok, vals(r, (c-1)*nops+1:min(c*nops,nv)));
    endfor
  endfor
endfunction

function tok = tokenize (fmt)
  tok = {}; i = 1;
  while (i <= numel (fmt))
    if (fmt(i) == '%')
      if (i < numel (fmt) && fmt(i+1) == '%')
        tok{end+1} = struct ('op', false, 'text', '%'); i += 2;
      else
        m = regexp (fmt(i:end), '^%[-+ #0]*[0-9]*(\.[0-9]+)?[diouxXeEfgGcs]', 'match', 'once');
        if (isempty (m)), error ('compose: unsupported format operator'); endif
        tok{end+1} = struct ('op', true, 'text', m); i += numel (m);
      endif
    else
      j = i;
      while (i <= numel (fmt) && fmt(i) != '%'), i += 1; endwhile
      s = fmt(j:i-1);
      % Unknown escapes must not be silently dropped by Octave's decoder.
      p = 1;
      while (p <= numel (s))
        if (s(p) == char (92))
          p += 1;
          if (p > numel (s) || ! any (s(p) == ['ntrfvab', char(92), char(39), char(34)]))
            error ('compose: unsupported escape sequence');
          endif
        endif
        p += 1;
      endwhile
      tok{end+1} = struct ('op', false, 'text', do_string_escapes (s));
    endif
  endwhile
endfunction

function out = render (tok, vals)
  pieces = cell (size (tok)); i = 1;
  for k = 1:numel (tok)
    t = tok{k};
    if (! t.op || i > numel (vals)), pieces{k} = t.text; continue; endif
    v = vals{i}; code = t.text(end);
    if (any (code == 'cs'))
      if (! ischar (v)), error ('compose: numeric-to-text format coercion is not supported'); endif
      if (any (uint8 (v) > 127) && (code == 'c' || numel (t.text) > 2))
        error ('compose: non-ASCII text requires an unqualified %%s operator');
      endif
      if (code == 'c' && numel (v) != 1), error ('compose: %%c requires one ASCII character'); endif
    elseif (! isnumeric (v) && ! islogical (v))
      error ('compose: numeric format operators require numeric data');
    elseif (any (code == 'diouxX') && (! isfinite (v) || v != fix (v)))
      error ('compose: integer format operators require finite integer values');
    endif
    pieces{k} = sprintf (t.text, v); i += 1;
  endfor
  out = ['', pieces{:}];
  if (isempty (out)), out = ''; endif
endfunction

function check_text (x)
  ok = ischar (x) && (isrow (x) || isempty (x));
  if (iscell (x)), ok = all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), x(:))); endif
  if (! ok), error ('compose: FORMAT must be a character vector or cell array of character vectors'); endif
endfunction
