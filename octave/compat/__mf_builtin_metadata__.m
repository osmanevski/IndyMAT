function out = __mf_builtin_metadata__ (target, direction)
% __MF_BUILTIN_METADATA__ Recorded R2025b signatures for selected built-ins.
% Called only after native metadata fails. Never supplies user-function counts
% or guesses signatures for unlisted built-ins; an empty result means defer.
  out = [];
  if (ischar (target) && isrow (target))
    if (! any (strcmp (target, {'sin', 'str2double', 'sum', 'size', 'max'}))), return; endif
    target = str2func (target);
  endif
  if (! isa (target, 'function_handle')), return; endif
  info = functions (target);
  if (! strcmp (info.type, 'simple') || ! isempty (info.file)), return; endif
  if (strcmp (direction, 'inputs'))
    switch info.function
      case {'sin', 'str2double'}, out = 1;
      case 'sum', out = 4;
      case 'size', out = -1;
    endswitch
  elseif (strcmp (direction, 'outputs'))
    switch info.function
      case 'sum', out = 1;
      case 'size', out = -1;
      case 'max', out = 2;
    endswitch
  endif
endfunction
