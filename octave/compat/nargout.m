function out = nargout (varargin)
% NARGOUT Built-in metadata subset: sum, size and max.
% Native metadata and errors pass through, including anonymous/user functions.
% Zero-argument calls inspect the caller, rather than this wrapper's frame.
% Other built-ins retain Octave's unavailable-metadata error.
  try
    if (builtin ('nargin') == 0), out = evalin ('caller', "builtin ('nargout')");
    else, out = builtin ('nargout', varargin{:}); endif
    return;
  catch err
    out = [];
    if (builtin ('nargin') == 1), out = __mf_builtin_metadata__ (varargin{1}, 'outputs'); endif
    if (isempty (out)), rethrow (err); endif
  end_try_catch
endfunction
