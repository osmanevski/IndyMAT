## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows AN03;AN08 default time grid remains Octave algorithm.
function [y,t,x] = step (sys,varargin)
  if(nargout==0), __indymat_zpk_scope__('call'); endif
  [y,t,x]=step(tf(sys),varargin{:});
endfunction
