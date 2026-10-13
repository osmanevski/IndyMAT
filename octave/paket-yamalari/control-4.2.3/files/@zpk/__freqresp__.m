## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows AN01–AN02,D19–D20 direct product frequency evaluation.
function h = __freqresp__ (sys,w,varargin)
  if(isempty(sys.data.K)), h=zeros(0,0,numel(w)); return; endif
  s=1i*w(:).'; h=repmat(sys.data.K,size(s));
  for v=sys.data.Z{1}.', h=h.*(s-v); endfor
  for v=sys.data.P{1}.', h=h./(s-v); endfor
  if(sys.data.K==0), h(:)=0; endif
  h=reshape(h,1,1,[]);
endfunction
