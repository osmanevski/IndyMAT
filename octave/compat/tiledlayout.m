function layout = tiledlayout(varargin)
% TILEDLAYOUT Fixed m-by-n grid with normalized axes positions and spacing subset.
% 'flow', layout labels, and arbitrary layout properties are unsupported.
  if isempty(varargin), error('tiledlayout: rows and columns are required'); endif
  f = gcf(); args=varargin;
  if numel(args)>=3 && isnumeric(args{3}) && isnumeric(args{1}) && isscalar(args{1}) && isgraphics(args{1},'figure')
    f=args{1}; args(1)=[];
  endif
  if numel(args)<2 || ~isnumeric(args{1}) || ~isnumeric(args{2})
    error('tiledlayout: supported form is tiledlayout(m,n) or tiledlayout(figure,m,n)');
  endif
  m=args{1}; n=args{2}; args(1:2)=[]; spacing='compact'; padding='compact';
  if mod(numel(args),2) ~= 0, error('tiledlayout: options must be name/value pairs'); endif
  for k=1:2:numel(args)
    key=args{k}; val=args{k+1};
    if ~ischar(key) || ~ischar(val), error('tiledlayout: option names and values must be character rows'); endif
    switch lower(key)
      case 'tilespacing', spacing=lower(val);
      case 'padding', padding=lower(val);
      otherwise, error('tiledlayout: unsupported option %s', key);
    endswitch
  endfor
  positions=__mf_tiledlayout_positions__(m,n,spacing,padding);
  % Replace ordinary axes and auxiliary/hidden axes, including paired rulers.
  old=findall(f,'Type','axes');
  for h=old(:)', if isgraphics(h,'axes'), delete(h); endif; endfor
  generation=tempname(); % Unique even after clear functions or figure-number reuse.
  state=struct('generation',generation,'figure',f,'rows',m,'columns',n,'positions',positions,'next',1,'axes',zeros(m*n,1),'spacing',spacing,'padding',padding);
  setappdata(f,'__mf_tiledlayout_state__',state);
  layout=struct('Figure',f,'Generation',generation,'Rows',m,'Columns',n);
endfunction
