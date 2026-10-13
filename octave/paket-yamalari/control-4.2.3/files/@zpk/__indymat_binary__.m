## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows ADD/SUB/MUL/DIV/FB ordered Z pairs;PADD/PSUB/PMUL/PDIV/PFB_ZL/ZR.
function result = __indymat_binary__ (op,a,b,varargin)
  __indymat_zpk_scope__(a); __indymat_zpk_scope__(b);
  if(isa(a,'ss') || isa(b,'ss'))
    aa=ss(a); bb=ss(b);
    switch(op)
      case 'add', result=aa+bb;
      case 'sub', result=aa-bb;
      case 'mul', result=aa*bb;
      case 'div', result=aa/bb;
      case 'feedback', result=feedback(aa,bb,varargin{:});
    endswitch
    return;
  endif
  a=zpk(a); b=zpk(b); [za,pa,ka]=zpkdata(a,'v'); [zb,pb,kb]=zpkdata(b,'v');
  switch(op)
    case 'mul', result=zpk([za;zb],[pa;pb],ka*kb);
    case 'div', result=zpk([za;pb],[pa;zb],ka/kb);
    otherwise
      aa=tf(a); bb=tf(b);
      switch(op)
        case 'add', value=aa+bb;
        case 'sub', value=aa-bb;
        case 'feedback', value=feedback(aa,bb,varargin{:});
      endswitch
      result=zpk(value);
      if(any(strcmp(op,{'add','sub'}))), result=__set__(result,'P',{[pa;pb]}); endif
  endswitch
endfunction
