## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows INV_Z.
function result = inv (sys)
  [z,p,k]=zpkdata(sys,'v'); result=zpk(p,z,1/k);
endfunction
