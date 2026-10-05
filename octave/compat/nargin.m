function out = nargin (varargin)
% NARGIN Built-in metadata subset: sin, str2double, sum and size.
% Native metadata and errors pass through, including anonymous/user functions.
% Zero-argument calls inspect the caller, rather than this wrapper's frame.
% Other built-ins retain Octave's unavailable-metadata error.
  try
    if (builtin ('nargin') == 0), out = evalin ('caller', "builtin ('nargin')");
    else, out = builtin ('nargin', varargin{:}); endif
    return;
  catch err
    out = [];
    if (builtin ('nargin') == 1), out = __mf_builtin_metadata__ (varargin{1}, 'inputs'); endif
    if (isempty (out)), rethrow (err); endif
  end_try_catch
endfunction
