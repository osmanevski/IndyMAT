function varargout = polyfit (varargin)
  if (nargout < 3), [varargout{1:nargout}] = original () (varargin{:}); return; endif
% POLYFIT Return column MU in the three-output form; fitting remains Octave's.
% Octave-only degree masks and its S fields are retained, not MATLAB extensions.
  [varargout{1:nargout}] = original () (varargin{:});
  varargout{3} = varargout{3}(:);
endfunction

function f = original ()
  persistent handle;
  if (isempty (handle)), handle = __mf_original_function__ ('polyfit'); endif
  f = handle;
endfunction
