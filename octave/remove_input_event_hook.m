function varargout = remove_input_event_hook(varargin)
  % A wrapped id has already left the native list. Avoid a false not-found
  % warning, while native validation and unknown-id warnings remain intact.
  known = nargin == 1 && ischar(varargin{1}) && (@__mf_input_hooks__)('known', varargin{1});
  if !known, builtin('remove_input_event_hook', varargin{:}); endif
  if known, (@__mf_input_hooks__)('remove', varargin{:}); endif
  if nargout, error('value on right hand side of assignment is undefined'); endif
endfunction
