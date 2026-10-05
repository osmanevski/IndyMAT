function rgb = __mf_graphics_parse_colors__(value)
% Parse numeric RGB rows or MATLAB named/hex colour lists (documented subset).
  if isnumeric(value)
    if ~isreal(value) || ndims(value) ~= 2 || columns(value) ~= 3 || isempty(value) || any(~isfinite(value(:))) || any(value(:) < 0) || any(value(:) > 1)
      error('__mf_graphics_parse_colors__: expected a nonempty n-by-3 RGB matrix in [0,1]');
    endif
    rgb = double(value); return;
  endif
  if ischar(value)
    if rows(value) > 1, names = cellstr(value); else names = {value}; endif
  elseif (exist('isstring','builtin') || exist('isstring','file')) && isstring(value)
    names = cellstr(value(:))';
  elseif iscell(value)
    names = value(:)';
  else
    error('__mf_graphics_parse_colors__: expected RGB matrix, character list, or cell array');
  endif
  if isempty(names), error('__mf_graphics_parse_colors__: colour list cannot be empty'); endif
  rgb = zeros(numel(names), 3);
  for k = 1:numel(names)
    name = names{k};
    if ~ischar(name) || rows(name) ~= 1, error('__mf_graphics_parse_colors__: each colour must be a character row'); endif
    name = lower(strtrim(name));
    switch name
      case {'y','yellow'}, rgb(k,:) = [1 1 0];
      case {'m','magenta'}, rgb(k,:) = [1 0 1];
      case {'c','cyan'}, rgb(k,:) = [0 1 1];
      case {'r','red'}, rgb(k,:) = [1 0 0];
      case {'g','green'}, rgb(k,:) = [0 1 0];
      case {'b','blue'}, rgb(k,:) = [0 0 1];
      case {'w','white'}, rgb(k,:) = [1 1 1];
      case {'k','black'}, rgb(k,:) = [0 0 0];
      otherwise
        if numel(name) == 7 && name(1) == '#' && all(ismember(name(2:end),'0123456789abcdef'))
          hex = sscanf(name(2:end), '%2x%2x%2x');
          if numel(hex) ~= 3, error('__mf_graphics_parse_colors__: invalid hex colour %s', name); endif
          rgb(k,:) = double(hex(:)') / 255;
        else
          error('__mf_graphics_parse_colors__: unsupported colour %s', name);
        endif
    endswitch
  endfor
endfunction
