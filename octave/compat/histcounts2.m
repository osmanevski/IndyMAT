function [n, xedges, yedges, binx, biny] = histcounts2 (x, y, varargin)
% HISTCOUNTS2 Automatic bins or explicit finite increasing edges.
% Supports count, countdensity, probability, percentage, pdf, cumcount, cdf.
% Probability/pdf/cdf divide by numel(X), including missing/outside points.
% Bins are left-closed/right-open except the final bin includes its endpoint.
% Bin indices are zero in BOTH axes when either coordinate is outside/NaN.
% Automatic bins use integer bins for integer-valued axes spanning <=50,
% otherwise Scott's 2-D rule (n^(-1/4)), with rounded, aligned widths.
% Automatic axes needing >1024 bins or unrepresentable edges are rejected.
% Explicit edge and data shapes are preserved.
% Numeric NBINS and bin-selection name/value options remain unsupported.
% Floating-point/logical data only; no integer classes or infinite/duplicate edges.
  if (nargin<2), error ('histcounts2: X and Y are required'); endif
  if (!(isfloat(x) || islogical(x)) || !(isfloat(y) || islogical(y)) || ...
      !isreal(x) || !isreal(y) || !isequal(size(x),size(y)))
    error ('histcounts2: X and Y must be real floating-point or logical arrays of the same size');
  endif
  if (isempty(varargin) || ischar(varargin{1}))
    xedges=auto_edges(x); yedges=auto_edges(y); args=varargin;
  elseif (numel(varargin)>=2 && isnumeric(varargin{1}) && isnumeric(varargin{2}))
    xedges=varargin{1}; yedges=varargin{2}; args=varargin(3:end);
  else
    error ('histcounts2: numeric NBINS is unsupported; provide two edge vectors');
  endif
  check_edges(xedges); check_edges(yedges); mode='count';
  if (mod(numel(args),2)), error ('histcounts2: options must be name/value pairs'); endif
  for k=1:2:numel(args)
    if (!ischar(args{k}) || !strcmpi(args{k},'Normalization') || !ischar(args{k+1}))
      error ('histcounts2: only the Normalization name/value option is supported');
    endif
    mode=lower(args{k+1});
  endfor
  if (!any(strcmp(mode,{'count','countdensity','probability','percentage','pdf','cumcount','cdf'})))
    error ('histcounts2: unsupported Normalization value');
  endif
  xe=double(xedges(:)'); ye=double(yedges(:)');
  nx=numel(xe)-1; ny=numel(ye)-1;
  binx=bin_index(double(x),xe); biny=bin_index(double(y),ye);
  invalid=binx==0 | biny==0; binx(invalid)=0; biny(invalid)=0;
  valid=!invalid;
  if (any(valid(:))), n=accumarray([binx(valid)(:) biny(valid)(:)],1,[nx ny]);
  else, n=zeros(nx,ny); endif
  area=diff(xe)'*diff(ye);
  switch mode
    case 'count'
    case 'countdensity', n=n./area;
    case 'probability', n=n/numel(x);
    case 'percentage', n=100*n/numel(x);
    case 'pdf', n=n/numel(x)./area;
    case 'cumcount', n=cumsum(cumsum(n,1),2);
    case 'cdf', n=cumsum(cumsum(n/numel(x),1),2);
  endswitch
endfunction

function edges = auto_edges (x)
  values=double(full(x(:))); values=values(isfinite(values));
  if (isempty(values)), edges=[0 1];
  else
    lo=min(values); hi=max(values);
    if (all(values==fix(values)) && hi-lo<=50)
      edges=(lo:hi+1)-0.5;
    elseif (lo==hi)
      edges=[lo-0.5 hi+0.5];
    else
      width=3.5*std(values)*numel(values)^(-1/4);
      % Same decimal width ladder as the numeric histcounts bin picker.
      scale=10^floor(log10(width)); relative=width/scale;
      if (relative<1.5), width=scale;
      elseif (relative<2.5), width=2*scale;
      elseif (relative<4), width=3*scale;
      elseif (relative<7.5), width=5*scale;
      else, width=10*scale; endif
      left=width*floor(lo/width); count=max(1,ceil((hi-left)/width));
      if (count>1024)
        error ('histcounts2: automatic bin count above 1024 is unsupported');
      endif
      edges=left+(0:count)*width;
    endif
  endif
  if (isa(x,'single')), edges=single(edges); endif
  if (any(!isfinite(edges)) || any(diff(edges)<=0))
    error ('histcounts2: automatic bins at this floating-point scale are unsupported');
  endif
endfunction

function check_edges (e)
  if (!isfloat(e) || !isreal(e) || !isvector(e) || numel(e)<2 || any(!isfinite(e(:))) || any(diff(double(e(:)))<=0))
    error ('histcounts2: edges must be finite strictly increasing floating-point vectors');
  endif
endfunction

function bins = bin_index (x, edges)
  bins=reshape(lookup(edges,x(:)),size(x));
  bins(x==edges(end))=numel(edges)-1;
  bins(!isfinite(x) | x<edges(1) | x>edges(end))=0;
  bins=double(bins);
endfunction
