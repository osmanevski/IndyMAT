## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows ZI01–ZI03,ZP,ZA10.
function value = subsref (sys,idx)
  switch(idx(1).type)
    case '.', value=get(sys,idx(1).subs);
    case '()'
      for i=1:numel(idx(1).subs)
        sub=idx(1).subs{i}; if(!isequal(sub,1) && !isequal(sub,':')), __indymat_zpk_scope__('array'); endif
      endfor
      if(numel(idx(1).subs)>2), __indymat_zpk_scope__('array'); endif
      value=sys;
    otherwise, __indymat_zpk_scope__('call');
  endswitch
  if(numel(idx)>1), value=subsref(value,idx(2:end)); endif
endfunction
