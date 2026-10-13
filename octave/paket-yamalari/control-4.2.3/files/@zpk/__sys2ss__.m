## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows CV03.
function [result,parent] = __sys2ss__ (sys)
  result=ss(tf(sys)); parent=sys.lti;
endfunction
