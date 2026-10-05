function h = histogram(data, varargin)
% HISTOGRAM Basic numeric histogram for IndyMAT / GNU Octave.
% Supports numeric data, bin count/edges, Normalization (count, probability,
% pdf, countdensity, cumcount, cdf), Orientation and bar styling properties.
% Returns an Octave bar handle; supports the subset documented below.
  if ~isnumeric(data) && ~islogical(data), error('histogram: numeric data required'); endif
  data=double(data(:)); data=data(isfinite(data));
  bins=[]; normmode='count'; orientation='vertical'; props={};
  if ~isempty(varargin) && isnumeric(varargin{1}), bins=varargin{1}; varargin(1)=[]; endif
  if mod(numel(varargin),2), error('histogram: properties must be name/value pairs'); endif
  for k=1:2:numel(varargin)
    key=lower(varargin{k});value=varargin{k+1};
    switch key
      case 'normalization', normmode=lower(value);
      case 'orientation', orientation=lower(value);
      case {'numbins','binedges'}, bins=value;
      case {'facecolor','edgecolor','facealpha','linewidth','linestyle'}, props=[props varargin(k:k+1)];
      otherwise, error('histogram: unsupported option %s',varargin{k});
    endswitch
  endfor
  if isempty(bins), bins=min(50,max(1,ceil(sqrt(numel(data))))); endif
  if isscalar(bins)
    if ~isfinite(bins)||bins<1||fix(bins)~=bins,error('histogram: positive integer bin count required');endif
    if isempty(data), lo=0;hi=1;else lo=min(data);hi=max(data);endif
    if lo==hi,lo=lo-.5;hi=hi+.5;endif
    edges=linspace(lo,hi,bins+1);
  else
    edges=bins(:)';
    if numel(edges)<2||any(~isfinite(edges))||any(diff(edges)<=0),error('histogram: finite increasing edges required');endif
  endif
  counts=histc(data,edges)';
  if isempty(data),counts=zeros(1,numel(edges));endif
  counts(end-1)=counts(end-1)+counts(end);counts(end)=[];
  width=diff(edges); values=counts;
  switch normmode
    case 'count'
    case 'probability',values=counts/max(1,numel(data));
    case 'pdf',values=counts./(max(1,numel(data))*width);
    case 'countdensity',values=counts./width;
    case 'cumcount',values=cumsum(counts);
    case 'cdf',values=cumsum(counts)/max(1,numel(data));
    otherwise,error('histogram: unsupported normalization %s',normmode);
  endswitch
  centers=(edges(1:end-1)+edges(2:end))/2;
  if any(abs(width-width(1))>max(abs(width))*1e-10)
    error('histogram: this compatibility function currently requires equal-width bins');
  endif
  if strcmp(orientation,'horizontal'), h=barh(centers,values,1,props{:});
  elseif strcmp(orientation,'vertical'),h=bar(centers,values,1,props{:});
  else,error('histogram: orientation must be horizontal or vertical');endif
endfunction
