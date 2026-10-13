## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function sys=__pid_validate__(sys)
  for key={"Kp","Ki","Kd","Tf"}
    v=sys.(key{1});
    if (islogical(v))
      error("Control:ltiobject:pidSet1", 'The value of the "%s" property must be a finite and real number.',key{1});
    endif
    if (!isnumeric(v) || !isscalar(v))
      error("IndyMAT:control:UnsupportedModelArray","This representation does not support arrays of models.");
    endif
    if (!isreal(v) || !isfinite(v))
      error("Control:ltiobject:pidSet1", 'The value of the "%s" property must be a finite and real number.',key{1});
    endif
    sys.(key{1})=double(v);
  endfor
  if (sys.Tf<0), error("Control:ltiobject:pidSet2", 'The value of the "Tf" property must be greater than or equal to 0.'); endif
endfunction
