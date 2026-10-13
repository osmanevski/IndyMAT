## IndyMAT SISO continuous-time PID subset. MATLAB row provenance in tests/test_control_pid26.py.
function sys = pid (varargin)
  superiorto ("tf", "double");
  if (nargin == 1 && isa(varargin{1}, "pid")), sys = varargin{1}; return; endif
  v = {1, 0, 0, 0};
  if (nargin == 1 && isa(varargin{1}, "lti"))
    src = varargin{1};
    if (!isa(src,"tf") && !isa(src,"ss")), scope("CallForm"); endif
    if (!isequal(size(src),[1 1])), scope("MIMO"); endif
    if (get(src,"tsam") != 0), scope("DiscreteTime"); endif
    [n,d] = tfdata(tf(src),"vector");
    n = trim(n); d = trim(d); n /= d(1); d /= d(1);
    ## PC08: ss-to-tf reconstruction has floating-point pole residuals.
    if (isa(src,"ss"))
      tol=64*eps*max(1,norm(d,inf));
      d(abs(d)<tol)=0;
    endif
    v = {0,0,0,0};
    if (numel(d)==1 && numel(n)==1), v{1}=n;
    elseif (isequal(d,[1 0]) && numel(n)<=2)
      n=[zeros(1,2-numel(n)),n]; v={n(1),n(2),0,0};
    elseif (numel(d)==2 && d(2)>0 && numel(n)<=2)
      n=[zeros(1,2-numel(n)),n]; f=1/d(2); kp=n(2)*f; v={kp,0,(n(1)-kp)*f,f};
    elseif (numel(d)==3 && d(3)==0 && d(2)>0 && numel(n)<=3)
      n=[zeros(1,3-numel(n)),n]; f=1/d(2); ki=n(3)*f; kp=(n(2)-ki)*f; v={kp,ki,(n(1)-kp)*f,f};
    else
      error("IndyMAT:control:UnsupportedPIDConversion", "This transfer function is outside the supported exact PID conversion forms.");
    endif
  else
    if (nargin>5), scope("CallForm"); endif
    for k=1:min(nargin,4), v{k}=varargin{k}; endfor
    if (nargin==5 && !isequal(varargin{5},0)), scope("DiscreteTime"); endif
  endif
  sys=class(struct("Kp",v{1},"Ki",v{2},"Kd",v{3},"Tf",v{4},"modelname",""),"pid",lti(1,1,0));
  sys=__pid_validate__(sys);
endfunction
function x=trim(x)
  i=find(x!=0,1); if(isempty(i)), x=0; else, x=x(i:end); endif
endfunction
function scope(kind)
  switch kind
    case "MIMO", msg="This representation supports SISO models only.";
    case "DiscreteTime", msg="This representation supports continuous-time models with Ts equal to zero only.";
    otherwise, msg="This call form is outside the documented control representation subset.";
  endswitch
  error(["IndyMAT:control:Unsupported",kind],msg);
endfunction
