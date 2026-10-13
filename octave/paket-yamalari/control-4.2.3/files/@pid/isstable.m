## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function varargout=isstable(sys,varargin)
  [varargout{1:nargout}]=isstable(tf(sys),varargin{:});
endfunction
