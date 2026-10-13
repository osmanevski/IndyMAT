## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows ZS01–ZS12;ZP19–ZP20 reject query form.
function result = set (sys,varargin)
  if(isempty(varargin) || mod(numel(varargin),2)), __indymat_zpk_scope__('call'); endif
  result=sys;
  for i=1:2:numel(varargin), result=__set__(result,varargin{i},varargin{i+1}); endfor
  result=__validate_sizes__(result);
  if(nargout==0)
    name=inputname(1); if(isempty(name)), __indymat_zpk_scope__('call'); endif
    assignin('caller',name,result);
  endif
endfunction
