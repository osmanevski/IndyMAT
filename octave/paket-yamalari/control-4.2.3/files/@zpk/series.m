## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows SER ordered Z pairs,PSER_ZL/ZR; order follows second*first.
function result = series (a,b,varargin)
  if(!isempty(varargin)), __indymat_zpk_scope__('call'); endif
  result=__indymat_binary__('mul',b,a);
endfunction
