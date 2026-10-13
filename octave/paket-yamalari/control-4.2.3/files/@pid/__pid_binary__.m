## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function out=__pid_binary__(op,a,b,varargin)
  for x={a,b}
    v=x{1};
    if(isa(v,"lti"))
      if(!isequal(size(v),[1 1])), error("IndyMAT:control:UnsupportedMIMO","This representation supports SISO models only."); endif
      if(get(v,"tsam")!=0), error("IndyMAT:control:UnsupportedDiscreteTime","This representation supports continuous-time models with Ts equal to zero only."); endif
    elseif(!isnumeric(v)||!isscalar(v)||!isreal(v)||!isfinite(v))
      error("IndyMAT:control:UnsupportedCallForm","This call form is outside the documented control representation subset.");
    endif
  endfor
  left=isa(a,"pid"); right=isa(b,"pid");
  if(xor(left,right) && (isnumeric(a)||isnumeric(b)) && !strcmp(op,"feedback"))
    if(left), p=a; n=b; else, p=b; n=a; endif
    switch op
      case {"plus","parallel"}, out=set(p,"Kp",p.Kp+n); return;
      case "minus"
        if(left), out=set(p,"Kp",p.Kp-n); else, out=pid(n-p.Kp,-p.Ki,-p.Kd,p.Tf); endif
        return;
      case {"mtimes","series"}, out=pid(n*p.Kp,n*p.Ki,n*p.Kd,p.Tf); return;
      case "mrdivide"
        if(left), out=pid(p.Kp/n,p.Ki/n,p.Kd/n,p.Tf); return; endif
    endswitch
  endif
  wantss=isa(a,"ss")||isa(b,"ss"); wantz=isa(a,"zpk")||isa(b,"zpk");
  if(isa(a,"lti")), a=tf(a); endif
  if(isa(b,"lti")), b=tf(b); endif
  out=feval(op,a,b,varargin{:});
  if(wantss), out=ss(out); elseif(wantz), out=zpk(out); endif
endfunction
