## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function varargout=dcgain(sys,varargin)
  [varargout{1:nargout}]=dcgain(tf(sys),varargin{:});
endfunction
