## IndyMAT: native control interconnection protocol; Z04/Z06/Z07/Z08.
function value = isstaticgain (sys)
  [z,p]=zpkdata(sys,'v'); value=isempty(z) && isempty(p);
endfunction
