function [v, dv] = deval (a, b, idx)
% DEVAL Evaluate ode45/ode23/ode15s mesh structures or IndyMAT ode113.
% Octave retains x,y,solver only, not MATLAB extdata/idata or the RHS.
% Native structures use local degree-5 polynomials (degree 3 for ode23),
% with interpolation error O(H^6) / O(H^4) on smooth, well-spaced meshes,
% plus mesh-solution error; H is the selected stencil span. Derivatives
% lose one order. This is NOT MATLAB's solver-specific dense extension.
% ode113 uses stored derivatives and cubic Hermite: O(h^4), d/dt O(h^3).
% No extrapolation, DAE derivative reconstruction or arbitrary structures.
  if (nargin < 2 || nargin > 3), error ('deval: two or three inputs required'); endif
  if (isstruct (a)), sol=a; q=b; else, sol=b; q=a; endif
  if (! isstruct(sol) || ! isscalar(sol) || ! all(isfield(sol,{'x','y','solver'})))
    error ('deval: an ODE solution structure with x, y and solver is required');
  endif
  x=sol.x(:)'; y=sol.y;
  if (! isnumeric(x) || ! isreal(x) || numel(x)<2 || any(!isfinite(x)) || ...
      ! isnumeric(y) || ndims(y)>2 || columns(y)!=numel(x) || any(!isfinite(y(:))))
    error ('deval: invalid solution mesh or values');
  endif
  if (all(diff(x)<0)), x=fliplr(x); y=fliplr(y); reverse=true;
  elseif (all(diff(x)>0)), reverse=false;
  else, error ('deval: solution mesh must be strictly monotone'); endif
  if (! isnumeric(q) || ! isreal(q) || ! isvector(q) && ! isempty(q) || any(!isfinite(q(:))))
    error ('deval: evaluation points must be a finite real vector');
  endif
  q=q(:)';
  if (any(q<x(1) | q>x(end))), error ('deval: evaluation points must lie within the solution interval'); endif
  if (nargin<3), idx=1:rows(y); endif
  if (! isnumeric(idx) || ! isreal(idx) || ! isvector(idx) || any(idx!=fix(idx) | idx<1 | idx>rows(y)))
    error ('deval: component indices must be valid positive integers');
  endif
  y=y(idx,:); v=zeros(rows(y),numel(q)); dv=v;
  if (strcmp(sol.solver,'ode113') && isfield(sol,'__mf_dy__'))
    f=sol.__mf_dy__;
    if (!isequal(size(f),size(sol.y)) || any(!isfinite(f(:)))), error('deval: invalid stored derivatives'); endif
    if (reverse), f=fliplr(f); endif
    f=f(idx,:);
    for k=1:numel(q)
      j=find(x<=q(k),1,'last'); j=min(j,numel(x)-1); h=x(j+1)-x(j); s=(q(k)-x(j))/h;
      v(:,k)=(2*s^3-3*s^2+1)*y(:,j)+(s^3-2*s^2+s)*h*f(:,j)+ ...
        (-2*s^3+3*s^2)*y(:,j+1)+(s^3-s^2)*h*f(:,j+1);
      dv(:,k)=(6*s^2-6*s)/h*y(:,j)+(3*s^2-4*s+1)*f(:,j)+ ...
        (-6*s^2+6*s)/h*y(:,j+1)+(3*s^2-2*s)*f(:,j+1);
    endfor
  else
    if (strcmp(sol.solver,'ode23')), n=4;
    elseif (any(strcmp(sol.solver,{'ode45','ode15s'}))), n=6;
    else, error ('deval: unsupported solver or missing ode113 derivative data'); endif
    if (numel(x)<n), error ('deval: native solver mesh has too few points for the stated interpolation order'); endif
    for k=1:numel(q)
      j=find(x<=q(k),1,'last'); first=max(1,min(j-floor((n-2)/2),numel(x)-n+1)); ids=first:first+n-1;
      z=(x(ids)-q(k))/(x(ids(end))-x(ids(1))); span=x(ids(end))-x(ids(1));
      % Lagrange basis evaluated at zero; analytic derivative, also at nodes.
      for r=1:n
        others=[1:r-1 r+1:n]; den=prod(z(r)-z(others));
        v(:,k)+=y(:,ids(r))*prod(-z(others))/den;
        der=0;
        for s=others, der+=prod(-z(others(others!=s))); endfor
        dv(:,k)+=y(:,ids(r))*der/(den*span);
      endfor
    endfor
  endif
endfunction
