## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function [out,parent]=__sys2ss__(sys)
  [out,parent]=__sys2ss__(tf(sys));
  parent=sys.lti;
endfunction
