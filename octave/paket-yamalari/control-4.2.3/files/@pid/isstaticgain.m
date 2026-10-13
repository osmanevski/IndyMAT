## Native LTI dispatch hook; gain branch measured in PID01/02/06.
function out=isstaticgain(sys)
  out=(sys.Ki==0 && sys.Kd==0);
endfunction
