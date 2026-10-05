function [out, window] = smoothdata (a, varargin)
% SMOOTHDATA Explicit-window movmean/movmedian and scalar-window gaussian.
% First nonsingleton dimension by default, or (A,dim,method,window).
% Moving windows shrink at endpoints; even scalar windows include one more
% point before than after. NaNs omitted by default; explicit omit/include
% nan/missing flags supported. Gaussian sigma=window/5 with endpoint and
% NaN weight renormalization; its kernel length is capped at 2*axis length.
% Real numeric/logical arrays; single preserved, integers/logicals -> double.
% Automatic window heuristic, sgolay, SamplePoints and other options error.
  if (nargin<1), error ('smoothdata: A is required'); endif
  if (!(isnumeric(a) || islogical(a)) || !isreal(a) || any(isinf(a(:))))
    error ('smoothdata: A must be real numeric or logical data without infinities');
  endif
  args=varargin; dim=find(size(a)!=1,1); if (isempty(dim)), dim=1; endif
  if (!isempty(args) && isnumeric(args{1}))
    dim=args{1}; args(1)=[];
    if (!isreal(dim) || !isscalar(dim) || !isfinite(dim) || dim<1 || dim!=fix(dim))
      error ('smoothdata: DIM must be a positive finite integer');
    endif
  endif
  if (numel(args)<2)
    error ('smoothdata: an explicit method and window are required; automatic window selection is unsupported');
  endif
  method=args{1}; window=args{2}; args(1:2)=[];
  if (!ischar(method) || !any(strcmpi(method,{'movmean','movmedian','gaussian'})))
    error ('smoothdata: only movmean, movmedian and gaussian are supported; sgolay requires MATLAB endpoint measurement');
  endif
  method=lower(method);
  if (!isnumeric(window) || !isreal(window) || !isvector(window) || !any(numel(window)==[1 2]) || ...
      any(!isfinite(window(:)) | window(:)<0 | window(:)!=fix(window(:))) || isscalar(window) && window==0)
    error ('smoothdata: window must be a positive integer or [backward forward] nonnegative integers');
  endif
  if (strcmp(method,'gaussian') && !isscalar(window))
    error ('smoothdata: gaussian currently requires a scalar window');
  endif
  omit=true;
  if (!isempty(args))
    if (numel(args)!=1 || !ischar(args{1}) || !any(strcmpi(args{1},{'omitnan','includenan','omitmissing','includemissing'})))
      error ('smoothdata: only an optional exact NaN/missing flag is supported after window');
    endif
    omit=any(strcmpi(args{1},{'omitnan','omitmissing'}));
  endif
  single_input=isa(a,'single'); sparse_input=issparse(a); out=double(full(a));
  if (!isempty(a) && dim<=ndims(a) && size(a,dim)>1)
    order=[dim 1:dim-1 dim+1:ndims(a)]; perm=permute(out,order); sz=size(perm); values=reshape(perm,sz(1),[]);
    count=rows(values); result=NaN(size(values));
    if (isscalar(window))
      width=double(window);
      if (strcmp(method,'gaussian')), width=min(width,2*count); endif
      back=floor(width/2); forward=floor((width-1)/2);
    else, back=double(window(1)); forward=double(window(2)); endif
    for i=1:count
      ids=max(1,i-back):min(count,i+forward);
      for j=1:columns(values)
        data=values(ids,j); offsets=ids(:)-i;
        if (omit), keep=!isnan(data); data=data(keep); offsets=offsets(keep); endif
        if (isempty(data) || any(isnan(data))), result(i,j)=NaN;
        elseif (strcmp(method,'movmean')), result(i,j)=mean(data);
        elseif (strcmp(method,'movmedian')), result(i,j)=median(data);
        else
          weights=exp(-0.5*(offsets/(double(window)/5)).^2);
          result(i,j)=sum(weights.*data)/sum(weights);
        endif
      endfor
    endfor
    out=ipermute(reshape(result,sz),order);
  endif
  if (single_input), out=single(out); endif
  if (sparse_input), out=sparse(out); endif
endfunction
