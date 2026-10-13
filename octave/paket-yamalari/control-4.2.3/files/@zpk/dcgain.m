## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows AN06,AN09–AN11.
function value = dcgain (sys)
  value=dcgain(tf(sys));
endfunction
