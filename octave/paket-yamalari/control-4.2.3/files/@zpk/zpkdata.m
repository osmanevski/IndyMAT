## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows D01–D21.
function [z,p,k,tsam] = zpkdata (sys,form)
  z=sys.data.Z; p=sys.data.P; k=sys.data.K; tsam=0;
  if(nargin>1)
    if(!ischar(form) || !any(strcmpi(form,{'v','vector'}))), __indymat_zpk_scope__('call'); endif
    if(!isempty(k)), z=z{1}; p=p{1}; endif
  endif
endfunction
