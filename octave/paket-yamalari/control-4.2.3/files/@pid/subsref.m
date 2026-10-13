## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function out=subsref(sys,s)
  if(isempty(s)), out=sys; return; endif
  if(!strcmp(s(1).type,".")), error("IndyMAT:control:UnsupportedCallForm","This call form is outside the documented control representation subset."); endif
  out=get(sys,s(1).subs);
  if(numel(s)>1), out=subsref(out,s(2:end)); endif
endfunction
