## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows AN02 explicit-frequency output subset.
function [mag,phase,w] = bode (sys,w)
  if(nargout==0 || nargin!=2), __indymat_zpk_scope__('call'); endif
  h=squeeze(freqresp(sys,w)); mag=abs(h); phase=unwrap(angle(h))*180/pi; w=w(:);
endfunction
