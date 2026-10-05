function [out, position] = __mf_textscan_pair__ (source, format, varargin)
% Parse a %f%s char-source form using the builtin's strict parser.
% Valid scans retain builtin values/shapes/positions. After a failed numeric
% conversion, ReturnOnError=true keeps only fields actually read. String
% fields cannot fail conversion; premature EOF remains builtin behavior.
  limit = Inf; options = varargin;
  if (! isempty (options) && isnumeric (options{1})), limit = options{1}; options(1) = []; endif
  stop = true; strict = options;
  for k = 1:2:numel (options)
    if (strcmpi (options{k}, 'ReturnOnError')), stop = logical (options{k+1}); strict{k+1} = false; endif
  endfor
  strict(end+1:end+2) = {'ReturnOnError', false};
  repeats = {}; if (isfinite (limit)), repeats = {limit}; endif
  try
    [out, position] = builtin ('textscan', source, format, repeats{:}, strict{:});
    return;
  catch failure
    if (! stop), rethrow (failure); endif
  end_try_catch
  out = {zeros(0,1), cell(0,1)}; position = 0; row = 0;
  while (position < numel (source) && row < limit)
    try
      [fields, used] = builtin ('textscan', source(position+1:end), format, 1, strict{:});
    catch
      break;
    end_try_catch
    if (used == 0), break; endif
    for k = 1:2, out{k} = [out{k}; fields{k}]; endfor
    position += used; row += 1;
  endwhile
endfunction
