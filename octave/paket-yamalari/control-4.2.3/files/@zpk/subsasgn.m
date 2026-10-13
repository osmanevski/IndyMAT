## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows ZA01–ZA10.
function sys = subsasgn (sys,idx,value)
  if(!strcmp(idx(1).type,'.')), __indymat_zpk_scope__('array'); endif
  key=idx(1).subs;
  if(numel(idx)>1), old=get(sys,key); value=subsasgn(old,idx(2:end),value); endif
  sys=__set__(sys,key,value);
  sys=__validate_sizes__(sys);
endfunction
