## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows PAR ordered Z pairs,PPAR_ZL/ZR.
function result = parallel (a,b,varargin)
  if(!isempty(varargin)), __indymat_zpk_scope__('call'); endif
  result=__indymat_binary__('add',a,b);
endfunction
