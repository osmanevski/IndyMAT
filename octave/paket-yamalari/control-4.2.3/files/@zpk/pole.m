## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows AN04.
function value = pole (sys)
  [~,value]=zpkdata(sys,'v');
endfunction
