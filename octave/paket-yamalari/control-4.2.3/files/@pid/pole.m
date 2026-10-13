## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function varargout=pole(sys,varargin)
  [varargout{1:nargout}]=pole(tf(sys),varargin{:});
endfunction
