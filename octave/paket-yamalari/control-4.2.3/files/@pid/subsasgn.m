## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function sys=subsasgn(sys,s,v)
  if(numel(s)!=1 || !strcmp(s(1).type,".")), error("IndyMAT:control:UnsupportedModelArray","This representation does not support arrays of models."); endif
  sys=set(sys,s(1).subs,v);
endfunction
