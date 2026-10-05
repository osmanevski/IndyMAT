function yes = __mf_textscan_pair_form__ (args, outputs)
% Identify the supported %f%s char-source form; invalid/other forms delegate.
% Options deliberately exclude custom whitespace, comments, empty-value rules,
% end-of-line rules, literal/skipped/width conversions and file sources.
  yes = false;
  if (numel (args) < 2 || outputs > 2 || ! ischar (args{1}) || ! isrow (args{1}) || ...
      ! ischar (args{2}) || ! strcmp (args{2}, '%f%s')), return; endif
  first = 3;
  if (numel (args) >= 3 && isnumeric (args{3}))
    n = args{3};
    if (! isreal (n) || ! isscalar (n) || ! isfinite (n) || n < 1 || fix (n) != n), return; endif
    first = 4;
  endif
  if (mod (numel (args) - first + 1, 2)), return; endif
  for k = first:2:numel (args)
    name = args{k}; value = args{k+1};
    if (! ischar (name)), return; endif
    if (strcmpi (name, 'Delimiter'))
      if (! ischar (value) || numel (value) != 1 || any (value == sprintf (' \t\r\n\v\f'))), return; endif
    elseif (any (strcmpi (name, {'CollectOutput', 'ReturnOnError'})))
      if ((! islogical (value) && ! isnumeric (value)) || ! isscalar (value) || ! isreal (value) || ! any (value == [0 1])), return; endif
    else, return;
    endif
  endfor
  yes = true;
endfunction
