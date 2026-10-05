function [f,copies]=__mf_export_snapshot__(target,bgcolor)
% Static snapshot of figure-owned axes and primitive graphics only.
% Includes hidden annotations, legends and colorbars as rendered primitives;
% no callbacks, application data, live links, or reconstruction of chart objects.
  if nargin<2, bgcolor=[]; endif
  source=ancestor(target,'figure');
  if isfigure(target), source=target; items=allchild(source);
  else
    if ~isequal(get(target,'parent'),source), error('exportgraphics: nested axes containers are unsupported'); endif
    items=target;
    if isappdata(target,'__mf_yyaxis_pair__')
      pair=getappdata(target,'__mf_yyaxis_pair__');
      if numel(pair)~=2 || ~all(isgraphics(pair,'axes')), error('exportgraphics: invalid yyaxis pair'); endif
      items=pair;
    endif
    for ax=items(:)'
      for name={'__legend_handle__','__colorbar_handle__'}
        if isprop(ax,name{1})
          decor=get(ax,name{1}); items=[items(:);decor(isgraphics(decor,'axes'))(:)];
        endif
      endfor
    endfor
    items=unique(items,'stable');
  endif
  % Exclude menu/toolbar chrome; reject unsupported plotted content before cloning.
  kept=[];
  for h=items(:)'
    if any(strcmp(get(h,'type'),{'uimenu','uitoolbar','uicontextmenu'})), continue; endif
    if ~isgraphics(h,'axes'), error('exportgraphics: only figure-owned axes and annotations are supported'); endif
    local_validate(h); kept(end+1)=h;
  endfor
  items=kept;
  if isempty(items), error('exportgraphics: no axes to export'); endif
  drawnow('expose');
  positions=zeros(numel(items),4); insets=positions;
  for k=1:numel(items)
    p=get(items(k),'position'); positions(k,:)=getpixelposition(items(k));
    if any(p(3:4)<=0), error('exportgraphics: axes must have positive dimensions'); endif
    scale=positions(k,3:4)./p(3:4);
    insets(k,:)=get(items(k),'tightinset').*scale([1 2 1 2]);
  endfor
  if isfigure(target)
    fp=getpixelposition(target); bounds=[1 1 fp(3:4)];
  else bounds=__mf_export_bounds__(positions,insets); endif
  previous=get(0,'currentfigure'); f=[]; complete=false; copies=[];
  unwind_protect
    f=figure('visible','off','units','pixels','position',[100 100 bounds(3:4)],...
      'createfcn',[],'deletefcn',[],'closerequestfcn',[],'resizefcn',[],...
      'sizechangedfcn',[],'inverthardcopy','off');
    % Explicit per-figure defaults suppress inherited callbacks during construction.
    defaults=__mf_export_defaults__(); set(f,defaults{:});
    if isempty(bgcolor), bgcolor=get(source,'color'); endif
    set(f,'color',bgcolor,'colormap',get(source,'colormap'));
    for k=numel(items):-1:1
      c=local_clone(items(k),f); copies(k)=c;
      p=positions(k,:); p(1:2)=p(1:2)-bounds(1:2)+1;
      set(c,'units','pixels','positionconstraint','innerposition','position',p);
    endfor
    ppi=get(0,'screenpixelsperinch'); inches=bounds(3:4)/ppi;
    set(f,'paperunits','inches','papersize',inches,'paperposition',[0 0 inches],'paperpositionmode','manual');
    complete=true;
  unwind_protect_cleanup
    if ~complete && isfigure(f), delete(f); endif
    if isempty(previous) || isfigure(previous), set(0,'currentfigure',previous); endif
  end_unwind_protect
endfunction

function local_validate(h)
  supported={'axes','line','text','patch','surface','image','hggroup','light'};
  if ~any(strcmp(get(h,'type'),supported)), error('exportgraphics: unsupported graphics object type %s',get(h,'type')); endif
  for child=allchild(h)', local_validate(child); endfor
endfunction

function dest=local_clone(source,parent)
  kind=get(source,'type'); args={'parent',parent,'createfcn',[],'deletefcn',[]};
  switch kind
    case 'axes', dest=axes(args{:},'nextplot','add');
    case 'line', dest=line(args{:});
    case 'text', dest=text(args{:});
    case 'patch'
      dest=patch(args{:},'faces',get(source,'faces'),'vertices',get(source,'vertices'),...
        'facevertexcdata',get(source,'facevertexcdata'));
    case 'surface', dest=surface(args{:});
    case 'image', dest=image(args{:});
    case 'hggroup', dest=hggroup(args{:});
    case 'light', dest=light(args{:});
    otherwise, error('exportgraphics: unsupported graphics object type %s',kind);
  endswitch
  labels=[]; newlabels=[];
  if strcmp(kind,'axes')
    for name={'title','xlabel','ylabel','zlabel'}
      labels(end+1)=get(source,name{1}); newlabels(end+1)=get(dest,name{1});
    endfor
  endif
  children=allchild(source);
  for k=numel(children):-1:1
    label=find(labels==children(k),1);
    if isempty(label), local_clone(children(k),dest);
    else local_properties(children(k),newlabels(label)); endif
  endfor
  local_properties(source,dest);
endfunction

function local_properties(source,dest)
  props=__mf_graphics_snapshot_props__(get(source),fieldnames(set(dest)));
  if strcmp(get(source,'type'),'patch')
    for name={'xdata','ydata','zdata','cdata'}, if isfield(props,name{1}), props=rmfield(props,name{1}); endif; endfor
  endif
  for name={'units','fontunits'}
    if isfield(props,name{1}), set(dest,name{1},props.(name{1})); props=rmfield(props,name{1}); endif
  endfor
  if ~isempty(fieldnames(props)), set(dest,props); endif
endfunction
