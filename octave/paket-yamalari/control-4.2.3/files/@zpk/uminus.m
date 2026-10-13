## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows SUB_ZZ–SUB_NZ.
function sys = uminus (sys)
  sys=__set__(sys,'K',-sys.data.K);
endfunction
