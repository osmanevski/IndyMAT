function out = __mf_insert_text__ (str, position, newtext, after, name)
% Shared char/cellstr insertion engine; BMP positions count UTF-16 units.
% Empty literals and supplementary characters fail explicitly.
  values = text_cells (str, name);
  additions = expand (text_cells (newtext, name), size (values), name);
  numeric = isnumeric (position);
  if (numeric)
    if (! isreal (position) || any (! isfinite (position(:))) || any (position(:) != fix (position(:))))
      error ('%s: positions must be finite real integers', name);
    endif
    positions = expand (num2cell (position), size (values), name);
  else
    positions = expand (text_cells (position, name), size (values), name);
    if (any (cellfun (@isempty, positions(:))))
      error ('%s: empty text boundaries are not supported', name);
    endif
  endif
  out = values;
  for k = 1:numel (values)
    source = codepoints (values{k}, name);
    addition = codepoints (additions{k}, name);
    if (numeric)
      cut = positions{k} - ! after;
      if (cut < 0 || cut > numel (source))
        error ('%s: position is outside the text', name);
      endif
      result = [source(1:cut) addition source(cut+1:end)];
    else
      boundary = codepoints (positions{k}, name);
      matches = [];
      for start = 1:numel (source)-numel (boundary)+1
        if (isequal (source(start:start+numel(boundary)-1), boundary))
          matches(end+1) = start;
        endif
      endfor
      result = uint32 ([]);
      cursor = 1;
      for start = matches
        if (start < cursor), continue; endif
        stop = start + numel (boundary) - 1;
        if (after)
          result = [result source(cursor:stop) addition];
        else
          result = [result source(cursor:start-1) addition source(start:stop)];
        endif
        cursor = stop + 1;
      endfor
      result = [result source(cursor:end)];
    endif
    if (isempty (result)), out{k} = '';
    else, out{k} = native2unicode (typecast (result, 'uint8'), 'UTF-32LE');
    endif
  endfor
  if (ischar (str)), out = out{1}; endif
endfunction

function cells = text_cells (value, name)
  if (ischar (value) && (isrow (value) || isempty (value)))
    cells = {value};
  elseif (iscell (value) && all (cellfun (@(v) ischar (v) && (isrow (v) || isempty (v)), value(:))))
    cells = value;
  else
    error ('%s: text must be a character vector or cell array of character vectors', name);
  endif
endfunction

function cells = expand (cells, target, name)
  if (isscalar (cells)), cells = repmat (cells, target);
  elseif (! isequal (size (cells), target))
    error ('%s: nonscalar operands must have the same size as the input', name);
  endif
endfunction

function cp = codepoints (text, name)
  cp = reshape (typecast (unicode2native (text, 'UTF-32LE'), 'uint32'), 1, []);
  if (any (cp > 65535))
    error ('%s: supplementary Unicode characters are not supported', name);
  endif
endfunction
