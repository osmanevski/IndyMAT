function varargout = pause(varargin)
% PAUSE Timed pauses use Octave; untimed pauses request an Enter in the IDE.
  if nargin==0
    input('Devam etmek için Enter: ','s');
  elseif nargout
    [varargout{1:nargout}]=builtin('pause',varargin{:});
  else
    builtin('pause',varargin{:});
  endif
endfunction
