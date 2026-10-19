function varargout = add_input_event_hook(varargin)
  % Let Octave validate the unchanged public call and issue its own identifier.
  % The locked registry limits dispatch to native program input waits and
  % reports that subset once per session without changing native ids.
  id = builtin('add_input_event_hook', varargin{:});
  builtin('remove_input_event_hook', id);
  (@__mf_input_hooks__)('add', id, varargin{:});
  varargout{1} = id;
endfunction
