## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function h=__freqresp__(sys,w,varargin)
  h=freqresp(tf(sys),w);
endfunction
