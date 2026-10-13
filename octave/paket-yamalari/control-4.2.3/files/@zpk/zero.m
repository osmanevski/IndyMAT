## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows AN05.
function [value,gain] = zero (sys,varargin)
  if(!isempty(varargin)), __indymat_zpk_scope__('call'); endif
  [value,~,gain]=zpkdata(sys,'v');
endfunction
