function out = exist (varargin)
  if (builtin ('nargin') != 2 || ! ischar (varargin{1}) || ! isrow (varargin{1}) || ! strcmp (varargin{2}, 'class') || ! any (varargin{1} == '.'))
    if (builtin ('nargin') >= 1 && builtin ('nargin') <= 2 && all (cellfun (@ischar, varargin)))
      out = evalin ('caller', __mf_exist_query__ (varargin{:}));
    else
      out = builtin ('exist', varargin{:});
    endif
    return;
  endif
% EXIST Recognize qualified classdef names in the explicit 'class' form.
% All other queries go straight to the built-in in the caller's workspace,
% preserving variable precedence and private/local function visibility.
% No change to file/builtin/dir queries or Octave's legacy class semantics.
  out = builtin ('exist', varargin{:});
  if (out != 0 || isempty (regexp (varargin{1}, '^[A-Za-z]\w*(\.[A-Za-z]\w*)+$', 'once'))), return; endif
  try
    metadata = meta.class.fromName (varargin{1});
    if (! isempty (metadata) && strcmp (metadata.Name, varargin{1})), out = 8; endif
  catch
    % Native exist returns zero for an unavailable/unloadable class.
  end_try_catch
endfunction
