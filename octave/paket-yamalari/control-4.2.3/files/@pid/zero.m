## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function varargout=zero(sys,varargin)
  [varargout{1:nargout}]=zero(tf(sys),varargin{:});
endfunction
