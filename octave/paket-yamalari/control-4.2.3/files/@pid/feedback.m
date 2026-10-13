## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function out=feedback(a,b,varargin)
  out=__pid_binary__("feedback",a,b,varargin{:});
endfunction
