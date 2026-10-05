function out = makima (x, y, xq)
% MAKIMA Modified Akima cubic Hermite interpolation for finite real data.
% Weights: abs(difference of slopes) + abs(sum of slopes)/2. End slopes
% are linearly extrapolated twice; two samples produce a straight line.
% Vector Y or array Y with samples in its LAST dimension, as pchip/spline.
% Returns pp or evaluates it (including polynomial extrapolation). No
% missing data, complex values or interp1(...,'makima') integration.
  if (nargin<2 || nargin>3), error ('makima: two or three inputs required'); endif
  if (!isnumeric(x) || !isreal(x) || !isvector(x) || numel(x)<2 || any(!isfinite(x(:))))
    error ('makima: X must be a finite real vector with at least two points');
  endif
  if (!isnumeric(y) || !isreal(y) || any(!isfinite(y(:))))
    error ('makima: Y must contain finite real numeric values');
  endif
  x=double(x(:)'); n=numel(x);
  if (isvector(y))
    if (numel(y)!=n), error ('makima: vector Y must have the same length as X'); endif
    y=double(y(:)'); dims=1;
  else
    sz=size(y); dims=sz(1:end-1);
    if (sz(end)!=n), error ('makima: last dimension of Y must match length of X'); endif
    y=double(reshape(y,[],n));
  endif
  if (all(diff(x)<0)), x=fliplr(x); y=fliplr(y);
  elseif (any(diff(x)<=0)), error ('makima: X must be strictly monotone'); endif
  h=diff(x); d=diff(y,1,2)./h;
  if (n==2), slopes=[d d];
  else
    left=2*d(:,1)-d(:,2); right=2*d(:,end)-d(:,end-1);
    ext=[2*left-d(:,1) left d right 2*right-d(:,end)];
    w1=abs(ext(:,4:n+3)-ext(:,3:n+2))+abs(ext(:,4:n+3)+ext(:,3:n+2))/2;
    w2=abs(ext(:,2:n+1)-ext(:,1:n))+abs(ext(:,2:n+1)+ext(:,1:n))/2;
    total=w1+w2; slopes=zeros(rows(y),n); mask=total>0;
    num=w1.*ext(:,2:n+1)+w2.*ext(:,3:n+2);
    slopes(mask)=num(mask)./total(mask);
  endif
  a=(slopes(:,1:end-1)+slopes(:,2:end)-2*d)./(h.^2);
  b=(3*d-2*slopes(:,1:end-1)-slopes(:,2:end))./h;
  out=mkpp(x,cat(3,a,b,slopes(:,1:end-1),y(:,1:end-1)),dims);
  if (nargin==3)
    if (!isnumeric(xq) || !isreal(xq) || any(!isfinite(xq(:)))), error ('makima: XQ must contain finite real numeric values'); endif
    out=ppval(out,xq);
  endif
endfunction
