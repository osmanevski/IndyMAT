## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows FB ordered Z pairs,FB_POS,PFB_ZL/ZR.
function result = feedback (a,b,varargin)
  if(nargin<2), b=1; endif
  if(numel(varargin)>1 || (!isempty(varargin) && !any(isequal(varargin{1},1) || isequal(varargin{1},-1)))), __indymat_zpk_scope__('call'); endif
  result=__indymat_binary__('feedback',a,b,varargin{:});
endfunction
