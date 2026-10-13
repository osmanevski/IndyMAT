## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows AN07,AN09–AN11.
function value = isstable (sys,varargin)
  if(!isempty(varargin)), __indymat_zpk_scope__('call'); endif
  value=all(real(pole(sys))<0);
endfunction
