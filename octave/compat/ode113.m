function varargout = ode113 (fun, tspan, y0, options)
% ODE113 Nonstiff variable-step Adams-Bashforth-Moulton PECE, order FOUR.
% NOT MATLAB's variable-order 1..13 implementation. AB4 predictor + AM4
% corrector on the actual nonuniform history, two RHS evaluations per trial.
% Milne error estimate uses the two mesh-dependent remainder constants;
% RK4 step doubling supplies the first three accepted steps. Local errors
% are controlled componentwise by max(AbsTol,RelTol*max(abs(y),abs(ynew))).
% Global error is fourth order; requested tolerance is LOCAL, not a global
% error guarantee. Dense output is cubic Hermite (fourth-order values).
% Supports real finite vectors, monotone output times, RelTol, AbsTol,
% InitialStep, MaxStep, NormControl='off'. Other nonempty options error.
% No events, callbacks, mass matrices, stiff/DAE or complex problems.
  if (nargin<3 || nargin>4 || nargout<1 || nargout>2)
    error ('ode113: use sol=ode113(...) or [t,y]=ode113(...) with three or four inputs');
  endif
  if (!isa(fun,'function_handle')), error ('ode113: ODEFUN must be a function handle'); endif
  if (!isnumeric(tspan) || !isreal(tspan) || !isvector(tspan) || numel(tspan)<2 || any(!isfinite(tspan(:))))
    error ('ode113: TSPAN must be a finite strictly monotone vector');
  endif
  tspan=double(tspan(:)'); direction=sign(tspan(end)-tspan(1));
  if (direction==0 || any(direction*diff(tspan)<=0)), error ('ode113: TSPAN must be strictly monotone'); endif
  if (!isnumeric(y0) || !isreal(y0) || !isvector(y0) || isempty(y0) || any(!isfinite(y0(:))))
    error ('ode113: Y0 must be a nonempty finite real vector');
  endif
  y=double(y0(:)); n=numel(y); span=abs(tspan(end)-tspan(1));
  if (!isfinite(span)), error ('ode113: integration interval is too large'); endif
  if (nargin<4 || isempty(options)), options=struct(); endif
  if (!isstruct(options) || !isscalar(options)), error ('ode113: OPTIONS must be an odeset structure'); endif
  supported={'RelTol','AbsTol','InitialStep','MaxStep','NormControl'};
  keys=fieldnames(options);
  for k=1:numel(keys)
    if (!isempty(options.(keys{k})) && !any(strcmp(keys{k},supported)))
      error ('ode113: option %s is unsupported',keys{k});
    endif
  endfor
  rt=option(options,'RelTol',1e-3); at=option(options,'AbsTol',1e-6);
  if (!isnumeric(rt) || !isreal(rt) || !isscalar(rt) || !isfinite(rt) || rt<100*eps || rt>=1)
    error ('ode113: RelTol must be a finite scalar in [100*eps,1)');
  endif
  if (!isnumeric(at) || !isreal(at) || !(isscalar(at) || isvector(at) && numel(at)==n) || any(!isfinite(at(:)) | at(:)<=0))
    error ('ode113: AbsTol must be positive finite scalar or one value per component');
  endif
  at=double(at(:)); nc=option(options,'NormControl','off');
  if (!ischar(nc) || !strcmpi(nc,'off')), error ('ode113: only NormControl=''off'' is supported'); endif
  maxstep=option(options,'MaxStep',span/10); initial=option(options,'InitialStep',min(maxstep,span/100));
  if (!isnumeric(maxstep) || !isreal(maxstep) || !isscalar(maxstep) || !isfinite(maxstep) || maxstep<=0 || ...
      !isnumeric(initial) || !isreal(initial) || !isscalar(initial) || !isfinite(initial) || initial<=0)
    error ('ode113: InitialStep and MaxStep must be positive finite scalars');
  endif
  h=direction*min([initial,maxstep,span]); t=tspan(1); f=rhs(fun,t,y,n);
  tx=t; yy=y; ff=f; evaluations=1; rejected=0; attempts=0;
  while (direction*(tspan(end)-t)>0)
    attempts+=1;
    if (attempts>100000), error ('ode113: step limit exceeded; problem may be stiff'); endif
    h=direction*min([abs(h),maxstep,abs(tspan(end)-t)]);
    if (t+h==t), error ('ode113: step size underflow; requested accuracy cannot be achieved'); endif
    % Snap to endpoint only when its floating-point distance is this step.
    tn=t+h;
    if (abs(h)==abs(tspan(end)-t)), tn=tspan(end); h=tn-t; endif
    if (numel(tx)<4)
      full=rk4(fun,t,y,h,f,n);
      half=rk4(fun,t,y,h/2,f,n);
      middle=rhs(fun,t+h/2,half,n);
      yn=rk4(fun,t+h/2,half,h/2,middle,n);
      evaluations+=10; err=(yn-full)/15;
    else
      ids=numel(tx):-1:numel(tx)-3;
      [wp,cp]=__mf_sayisal_adams_weights__((tx(ids)-t)/h);
      [wc,cc]=__mf_sayisal_adams_weights__([1 (tx(ids(1:3))-t)/h]);
      pred=y+h*ff(:,ids)*wp'; fp=rhs(fun,tn,pred,n);
      yn=y+h*[fp ff(:,ids(1:3))]*wc';
      err=(yn-pred)*(cc/(cp-cc)); evaluations+=1;
    endif
    scale=max(at,rt*max(abs(y),abs(yn))); ratio=max(abs(err)./scale);
    if (!isfinite(ratio)), error ('ode113: nonfinite local error estimate'); endif
    if (ratio<=1)
      fn=rhs(fun,tn,yn,n); evaluations+=1;
      t=tn; y=yn; f=fn; tx(end+1)=t; yy(:,end+1)=y; ff(:,end+1)=f;
      factor=min(1.3,max(0.5,0.9*max(ratio,1e-12)^(-1/5)));
    else
      rejected+=1; factor=min(0.8,max(0.1,0.9*ratio^(-1/5)));
    endif
    h*=factor;
  endwhile
  sol=struct('x',tx,'y',yy,'solver','ode113','__mf_dy__',ff, ...
    '__mf_stats__',struct('order',4,'nfevals',evaluations,'nsteps',numel(tx)-1,'nfailed',rejected));
  if (nargout==1), varargout{1}=sol;
  elseif (numel(tspan)==2), varargout{1}=tx(:); varargout{2}=yy.';
  else, varargout{1}=tspan(:); varargout{2}=deval(sol,tspan).'; endif
endfunction

function value = option (options, name, fallback)
  if (isfield(options,name) && !isempty(options.(name))), value=options.(name); else, value=fallback; endif
endfunction

function f = rhs (fun, t, y, n)
  f=fun(t,y);
  if (!isnumeric(f) || !isreal(f) || !isvector(f) || numel(f)!=n || any(!isfinite(f(:))))
    error ('ode113: ODEFUN must return a finite real vector matching Y0');
  endif
  f=double(f(:));
endfunction

function yn = rk4 (fun, t, y, h, f, n)
  k2=rhs(fun,t+h/2,y+h*f/2,n);
  k3=rhs(fun,t+h/2,y+h*k2/2,n);
  k4=rhs(fun,t+h,y+h*k3,n);
  yn=y+h*(f+2*k2+2*k3+k4)/6;
endfunction
