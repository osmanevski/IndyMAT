function args = __mf_quantile_options__ (args)
% __MF_QUANTILE_OPTIONS__ Translate the supported trailing Method pair.
% No approximated, weighted, or other named methods are implemented.
  index = find (cellfun (@(v) ischar (v) && strcmpi (v, 'Method'), args));
  if (isempty (index)), return; endif
  if (numel (index) != 1 || index != numel (args)-1 || index < 3 || index > 4)
    error ('quantile: Method must be one trailing name/value pair');
  endif
  value = args{end};
  if (! ischar (value)), error ('quantile: Method must be a character vector'); endif
  switch (lower (value))
    case 'midpoint', method = 5;
    case 'inclusive', method = 7;
    case 'exclusive', method = 6;
    otherwise, error ('quantile: unsupported Method %s', value);
  endswitch
  args(end-1:end) = [];
  if (numel (args) == 2 && isempty (args{1})), return; endif
  if (numel (args) == 2)
    dim = find (size (args{1}) > 1, 1);
    if (isempty (dim)), dim = 1; endif
    args{3} = dim;
  endif
  args{4} = method;
endfunction
