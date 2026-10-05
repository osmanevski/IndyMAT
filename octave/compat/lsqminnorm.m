function x = lsqminnorm (a, b, varargin)
% LSQMINNORM Dense single/double minimum-norm least squares using COD.
% Pivoted QR of A (A' when wide), then QR of the retained R rows' adjoint.
% Rank threshold: min(max(size(A))*eps(class(A)),sqrt(eps(class(A))))
% times abs(R(1,1)); complex A uses 10*max(size(A)) in the first factor.
% This is MATLAB R2025b DenseCOD's default, not the SVD rank tolerance.
% Optional scalar nonnegative tolerance and exact 'warn'/'nowarn' flags.
% Sparse SPQR semantics, regularization and abbreviated flags unsupported.
  if (nargin<2 || numel(varargin)>2), error ('lsqminnorm: use (A,B[,tol][,''warn''|''nowarn''])'); endif
  if (!(isa(a,'double') || isa(a,'single')) || !(isa(b,'double') || isa(b,'single')) || ...
      ndims(a)>2 || ndims(b)>2 || any(!isfinite(a(:))) || any(!isfinite(b(:))))
    error ('lsqminnorm: A and B must be finite single or double matrices');
  endif
  if (issparse(a) || issparse(b)), error ('lsqminnorm: sparse SPQR factorization is unsupported; use full inputs explicitly'); endif
  [m,n]=size(a);
  if (rows(b)!=m), error ('lsqminnorm: A and B must have the same number of rows'); endif
  tol=[]; rankwarn=false; args=varargin;
  if (!isempty(args) && isnumeric(args{1}))
    tol=args{1}; args(1)=[];
    if (!(isa(tol,'double') || isa(tol,'single')) || !isreal(tol) || !isscalar(tol) || isnan(tol) || tol<0)
      error ('lsqminnorm: tolerance must be a nonnegative real floating-point scalar');
    endif
  endif
  if (!isempty(args))
    if (numel(args)!=1 || !ischar(args{1}) || !any(strcmp(args{1},{'warn','nowarn'})))
      error ('lsqminnorm: only exact ''warn'' and ''nowarn'' rank warning flags are supported');
    endif
    rankwarn=strcmp(args{1},'warn');
  endif
  want_single=isa(a,'single') || isa(b,'single'); b=cast(b,class(a));
  if (m==0 || n==0), x=zeros(n,columns(b),class(a)); if (want_single), x=single(x); endif; return; endif
  wide=m<n;
  if (wide), work=a'; else, work=a; endif
  [q,r,p]=qr(work,0); % Octave economy QR returns a permutation VECTOR.
  if (isempty(tol))
    unit=eps(class(a)); factor=max(m,n);
    if (!isreal(a)), factor*=10; endif
    tol=min(factor*unit,sqrt(unit))*abs(r(1,1));
  endif
  diagonal=abs(diag(r)); first=find(diagonal<=tol,1);
  if (isempty(first)), rank_a=numel(diagonal); else, rank_a=first-1; endif
  x=zeros(n,columns(b),class(a));
  if (rank_a>0)
    [z,t]=qr(r(1:rank_a,:)',0);
    if (wide)
      x=q(:,1:rank_a)*(t\(z'*b(p,:)));
    else
      xp=z*(t'\(q(:,1:rank_a)'*b)); x(p,:)=xp;
    endif
  endif
  if (want_single), x=single(x); endif
  if (rankwarn && rank_a<min(m,n))
    warning ('MATLAB:rankDeficientMatrix','lsqminnorm: matrix is rank deficient (rank %d, tolerance %.6g)',rank_a,tol);
  endif
endfunction
