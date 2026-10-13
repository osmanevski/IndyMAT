## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function [out, parent]=__sys2tf__(sys)
  p=sys.Kp; i=sys.Ki; d=sys.Kd; f=sys.Tf;
  if(d==0)
    if(i==0), out=tf(p); else, out=tf([p i],[1 0]); endif
  elseif(i==0)
    if(f==0), out=tf([d p],1); else, out=tf([p+d/f p/f],[1 1/f]); endif
  else
    if(f==0), out=tf([d p i],[1 0]); else, out=tf([p+d/f p/f+i i/f],[1 1/f 0]); endif
  endif
  parent=sys.lti;
  out=set(out,"name",sys.modelname);
endfunction
