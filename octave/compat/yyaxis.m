function yyaxis(varargin)
% YYAXIS Implicit yyaxis left/right for 2-D plots on figure-owned axes.
% Use a common x domain on both sides, or set xlim explicitly after plotting.
% Two physical axes: use untargeted plot/hold/ylim/ylabel after switching.
% Legends require explicit line handles from both sides; implicit legend is unsupported.
% yyaxis(ax,side) is rejected: explicit-target plotting cannot be redirected.
% Cached physical handles address only that side; direct reset(ax), custom axes
% backgrounds, and reparenting are unsupported. Use cla reset to end the pair.
  if nargout>0, error('yyaxis: no output is supported'); endif
  if nargin==2, error('yyaxis: explicit axes targets are unsupported; use implicit yyaxis left/right and untargeted plotting'); endif
  if nargin~=1 || ~ischar(varargin{1}) || ~any(strcmpi(varargin{1},{'left','right'}))
    error('yyaxis: expected yyaxis left or yyaxis right');
  endif
  side=lower(varargin{1}); ax=gca(); f=get(ax,'parent');
  if ~isfigure(f) || ~isequal(get(ax,'view'),[0 90]), error('yyaxis: only 2-D figure-owned axes are supported'); endif
  if isappdata(ax,'__mf_yyaxis_state__'), error('yyaxis: legacy overlay state is unsupported; use a new figure'); endif
  pair=local_pair(ax);
  if isempty(pair)
    left=ax; oldcolor=get(left,'color'); units=get(left,'units'); pos=get(left,'position');
    if ~isequal(oldcolor,get(f,'defaultaxescolor')) && ~isequal(oldcolor,'none')
      error('yyaxis: custom axes backgrounds are unsupported');
    endif
    right=axes('parent',f,'units',units,'position',pos,'color','none',...
      'yaxislocation','right','xticklabel',[],'tag','__mf_yyaxis_right__');
    set(right,'xlim',get(left,'xlim'),'xscale',get(left,'xscale'),...
      'fontname',get(left,'fontname'),'fontsize',get(left,'fontsize'));
    pair=[left;right];
    for h=pair'
      setappdata(h,'__mf_yyaxis_pair__',pair);
      setappdata(h,'__mf_yyaxis_background__',oldcolor);
      % newplot's replacechildren preserves listeners and rulers but replaces data.
      set(h,'color','none','positionconstraint','innerposition');
      if ishold(left), mode='add'; else mode='replacechildren'; endif
      set(h,'nextplot',mode);
      sentinel=hggroup('parent',h,'handlevisibility','off','tag','__mf_yyaxis_guard__');
      set(sentinel,'deletefcn',{@local_cleanup,h});
      addlistener(h,'nextplot',@local_hold);
      addlistener(h,'xlim',@local_xlim);
      addlistener(h,'units',@local_position);
      addlistener(h,'position',@local_position);
    endfor
  endif
  if strcmp(side,'left'), axes(pair(1)); else axes(pair(2)); endif
endfunction

function pair=local_pair(ax)
  pair=[];
  if ~isappdata(ax,'__mf_yyaxis_pair__'), return; endif
  stored=getappdata(ax,'__mf_yyaxis_pair__');
  if isnumeric(stored) && numel(stored)==2 && all(isgraphics(stored,'axes')) && ...
      all(arrayfun(@(h) isequal(get(h,'parent'),get(ax,'parent')) && isappdata(h,'__mf_yyaxis_pair__') && isequal(getappdata(h,'__mf_yyaxis_pair__'),stored),stored))
    pair=stored(:);
  else
    local_cleanup([],[],ax);
  endif
endfunction

function local_hold(ax,~)
  pair=local_pair(ax); if isempty(pair), return; endif
  if strcmp(get(ax,'nextplot'),'add'), mode='add'; else mode='replacechildren'; endif
  for h=pair'
    if ~strcmp(get(h,'nextplot'),mode), set(h,'nextplot',mode); endif
  endfor
endfunction

function local_xlim(ax,~)
  pair=local_pair(ax); if isempty(pair), return; endif
  other=pair(pair~=ax); limits=get(ax,'xlim');
  if ~isequal(get(other,'xlim'),limits), set(other,'xlim',limits); endif
endfunction

function local_position(ax,~)
  pair=local_pair(ax); if isempty(pair), return; endif
  other=pair(pair~=ax);
  if ~strcmp(get(other,'units'),get(ax,'units')), set(other,'units',get(ax,'units')); endif
  if ~isequal(get(other,'position'),get(ax,'position')), set(other,'position',get(ax,'position')); endif
endfunction

function local_cleanup(trigger,~,ax)
  if ~isgraphics(ax,'axes') || ~isappdata(ax,'__mf_yyaxis_pair__'), return; endif
  pair=getappdata(ax,'__mf_yyaxis_pair__');
  if ~isnumeric(pair), pair=ax; endif
  pair=pair(:);
  background=getappdata(ax,'__mf_yyaxis_background__');
  if isempty(background), background=get(get(ax,'parent'),'defaultaxescolor'); endif
  % Recover a peer even when one side's stored handle has become stale.
  for candidate=findall(get(ax,'parent'),'type','axes')'
    if isappdata(candidate,'__mf_yyaxis_pair__')
      peerstate=getappdata(candidate,'__mf_yyaxis_pair__');
      if isnumeric(peerstate) && any(peerstate(:)==ax), pair=[pair(:);candidate]; endif
    endif
  endfor
  live=unique(pair(isgraphics(pair,'axes')));
  % Remove shared state before deleting the peer, so its sentinel cannot recurse.
  for h=live'
    if isappdata(h,'__mf_yyaxis_pair__'), rmappdata(h,'__mf_yyaxis_pair__'); endif
    if isappdata(h,'__mf_yyaxis_background__'), rmappdata(h,'__mf_yyaxis_background__'); endif
    dellistener(h,'nextplot',@local_hold); dellistener(h,'xlim',@local_xlim);
    dellistener(h,'units',@local_position); dellistener(h,'position',@local_position);
  endfor
  if isempty(trigger)
    guards=findall(ax,'type','hggroup','tag','__mf_yyaxis_guard__');
    for guard=guards(:)', set(guard,'deletefcn',[]); delete(guard); endfor
  endif
  other=live(live~=ax);
  if ~isempty(other), delete(other); endif
  if strcmp(get(ax,'beingdeleted'),'off')
    set(ax,'color',background,'yaxislocation','left','xticklabelmode','auto','nextplot','replace');
  endif
endfunction
