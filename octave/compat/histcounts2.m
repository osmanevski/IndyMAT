function [n, xedges, yedges, binx, biny] = histcounts2 (x, y, varargin)
% HISTCOUNTS2 Two-dimensional counts with EXPLICIT finite increasing edges.
% Supports count, countdensity, probability, percentage, pdf, cumcount, cdf.
% Probability/pdf/cdf divide by numel(X), including missing/outside points.
% Bins are left-closed/right-open except the final bin includes its endpoint.
% Bin indices are zero in BOTH axes when either coordinate is outside/NaN.
% Edge and data shapes are preserved. Automatic rules and numeric NBINS
% require MATLAB binpicker measurement and currently raise an explicit error.
% Floating-point/logical data only; no integer classes or infinite/duplicate edges.
  if (nargin<2), error ('histcounts2: X and Y are required'); endif
  if (!(isfloat(x) || islogical(x)) || !(isfloat(y) || islogical(y)) || ...
      !isreal(x) || !isreal(y) || !isequal(size(x),size(y)))
    error ('histcounts2: X and Y must be real floating-point or logical arrays of the same size');
  endif
  if (numel(varargin)<2 || !isnumeric(varargin{1}) || !isnumeric(varargin{2}))
    error ('histcounts2: explicit Xedges and Yedges are required; automatic bins and NBINS are unsupported');
  endif
  xedges=varargin{1}; yedges=varargin{2}; check_edges(xedges); check_edges(yedges);
  args=varargin(3:end); mode='count';
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
