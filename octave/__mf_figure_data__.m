function data = __mf_figure_data__(fig)
  mlock();
  limit=2000;
  data=struct('version',2,'supported',true,'decimated',false,'point_limit',limit,'axes',{{}},'reason','');
  axes_handles=findall(fig,'type','axes');
  for k=numel(axes_handles):-1:1
    ax=axes_handles(k); tag=__mf_prop__(ax,'tag','');
    if any(strcmp(tag,{'legend','colorbar'})),continue;endif
    item=__mf_axes_data__(ax,fig,limit);
    data.axes{end+1}=item;
    data.decimated=data.decimated||item.decimated;
    if ~item.supported
      data.supported=false;
      if isempty(data.reason),data.reason=item.reason;endif
    endif
  endfor
  if isempty(data.axes)
    data.supported=false; data.reason=__mf_text__('No interactive axes were found.', 'Etkileşimli eksen bulunamadı.');
  endif
endfunction

function item = __mf_axes_data__(ax,fig,limit)
  item=struct('supported',true,'reason','','position',[],'xlim',double(get(ax,'xlim')),'ylim',double(get(ax,'ylim')),...
    'xscale',get(ax,'xscale'),'yscale',get(ax,'yscale'),'xdir',get(ax,'xdir'),'ydir',get(ax,'ydir'),...
    'xlabel',__mf_graphics_text__(get(ax,'xlabel')),'ylabel',__mf_graphics_text__(get(ax,'ylabel')),'title',__mf_graphics_text__(get(ax,'title')),...
    'grid',struct('x',strcmp(get(ax,'xgrid'),'on'),'y',strcmp(get(ax,'ygrid'),'on')),'legend',__mf_legend__(ax),...
    'decimated',false,'series',{{}});
  units=get(ax,'units');
  unwind_protect
    set(ax,'units','normalized'); item.position=double(get(ax,'position'));
  unwind_protect_cleanup
    set(ax,'units',units);
  end_unwind_protect
  kids=get(ax,'children'); kids=kids(:).';
  baselines=[];
  for h=kids
    if strcmp(__mf_prop__(h,'type',''),'hggroup') && ~isempty(__mf_prop__(h,'basevalue',[]))
      baseline=__mf_prop__(h,'baseline',[]);
      if ~isempty(baseline),baselines(end+1)=baseline;endif
    endif
  endfor
  for h=fliplr(kids)
    if any(h==baselines)||strcmp(__mf_prop__(h,'visible','on'),'off'),continue;endif
    [ok,series,reason]=__mf_series_data__(h,ax,fig,limit);
    if ~ok
      item.supported=false; item.reason=reason; item.series={}; return;
    endif
    item.series{end+1}=series; item.decimated=item.decimated||series.decimated;
  endfor
endfunction

function [ok,series,reason] = __mf_series_data__(h,ax,fig,limit)
  ok=false; series=[]; reason=''; type=__mf_prop__(h,'type',''); kind='';
  if strcmp(type,'line')
    if ~isempty(__mf_prop__(h,'zdata',[])),reason=__mf_text__('A 3-D line can only be displayed as a PNG.', 'Üç boyutlu çizgi yalnızca PNG olarak gösterilebilir.');return;endif
    kind='line';
  elseif strcmp(type,'scatter')
    if ~isempty(__mf_prop__(h,'zdata',[])),reason=__mf_text__('A 3-D scatter plot can only be displayed as a PNG.', 'Üç boyutlu saçılım yalnızca PNG olarak gösterilebilir.');return;endif
    kind='scatter';
  elseif strcmp(type,'hggroup')
    if ~isempty(__mf_prop__(h,'zdata',[])),reason=__mf_text__('A 3-D series can only be displayed as a PNG.', 'Üç boyutlu seri yalnızca PNG olarak gösterilebilir.');return;endif
    children=__mf_prop__(h,'children',[]); creator='';
    try creator=getappdata(h,'__creator__');catch end_try_catch
    if strcmp(creator,'__stem__')
      kind='stem';
    elseif numel(children)==1 && strcmp(__mf_prop__(children(1),'type',''),'line') && ~isempty(__mf_prop__(h,'xdata',[]))
      kind='stairs';
    else
      reason=__mf_text__('This graphics object is not supported in the interactive view: hggroup.', 'Bu grafik nesnesi etkileşimli görünümde desteklenmiyor: hggroup.');return;
    endif
  else
    reason=sprintf(__mf_text__('This graphics object is not supported in the interactive view: %s.', 'Bu grafik nesnesi etkileşimli görünümde desteklenmiyor: %s.'),type);return;
  endif
  x=__mf_prop__(h,'xdata',[]); y=__mf_prop__(h,'ydata',[]);
  if ~isreal(x)||~isreal(y)||~isvector(x)||~isvector(y)||numel(x)~=numel(y)
    reason=__mf_text__('Series data must be real 2-D x/y vectors.', 'Seri verisi iki boyutlu gerçek x/y vektörleri değil.');return;
  endif
  x=double(x(:).'); y=double(y(:).'); original=numel(x);
  [x,y,decimated]=__mf_decimate__(x,y,limit);
  [line_color,edge_color,face_color,color_ok]=__mf_colors__(h,ax,kind);
  if ~color_ok,reason=__mf_text__('This color or transparency setting can only be displayed as a PNG.', 'Bu renk veya saydamlık ayarı yalnızca PNG olarak gösterilebilir.');return;endif
  marker=__mf_prop__(h,'marker','none');
  if ~any(strcmp(marker,{'none','o','s','square','^','v','>','<','.','+','x','*'}))
    reason=__mf_text__('This marker style can only be displayed as a PNG.', 'Bu işaretçi biçimi yalnızca PNG olarak gösterilebilir.');return;
  endif
  series=struct('kind',kind,'x',x,'y',y,'line_color',line_color,'marker_edge_color',edge_color,...
    'marker_face_color',face_color,'marker_face_auto',strcmp(__mf_prop__(h,'markerfacecolor','none'),'auto'),...
    'line_style',__mf_prop__(h,'linestyle','none'),...
    'marker',__mf_prop__(h,'marker','none'),'display_name',__mf_prop__(h,'displayname',''),...
    'base_value',double(__mf_prop__(h,'basevalue',0)),'decimated',decimated,'original_points',original);
  ok=true;
endfunction

function [xout,yout,changed] = __mf_decimate__(x,y,limit)
  n=numel(x); changed=n>limit;
  if ~changed,xout=x;yout=y;return;endif
  bucket_count=floor((limit-2)/5); edges=round(linspace(2,n,bucket_count+1)); idx=[1,n];
  for k=1:bucket_count
    range=edges(k):max(edges(k),edges(k+1)-1); chosen=[];
    valid=range(isfinite(x(range))); if ~isempty(valid),[~,lo]=min(x(valid));[~,hi]=max(x(valid));chosen=[chosen,valid(lo),valid(hi)];endif
    valid=range(isfinite(y(range))); if ~isempty(valid),[~,lo]=min(y(valid));[~,hi]=max(y(valid));chosen=[chosen,valid(lo),valid(hi)];endif
    invalid=range(~isfinite(x(range))|~isfinite(y(range))); if ~isempty(invalid),chosen=[chosen,invalid(1)];endif
    if isempty(chosen),chosen=range(1);endif
    idx=[idx,chosen];
  endfor
  idx=sort(unique(idx));
  if numel(idx)>limit,idx=idx(1:limit);endif
  xout=x(idx); yout=y(idx);
endfunction

function [line,edge,face,ok] = __mf_colors__(h,ax,kind)
  ok=false; line='none'; edge='none'; face='none';
  % Never approximate transparency, per-point colours or flat/interp modes.
  if ~__mf_opaque__(h),return;endif
  if strcmp(kind,'scatter')
    c=__mf_prop__(h,'cdata',[]);
    if ~isempty(c)&&~(isnumeric(c)&&(isscalar(c)||isequal(size(c),[1 3]))),return;endif
    base=c;
  else
    [line,valid]=__mf_rgb_or_none__(__mf_prop__(h,'color','none'));
    if ~valid,return;endif
    base=line;
  endif
  edge=__mf_prop__(h,'markeredgecolor','auto');
  face=__mf_prop__(h,'markerfacecolor','none');
  if ischar(edge)&&strcmp(edge,'auto'),edge=base;endif
  if ischar(face)&&strcmp(face,'auto'),face=__mf_prop__(ax,'color','none');endif
  [edge,edge_ok]=__mf_rgb_or_none__(edge);
  [face,face_ok]=__mf_rgb_or_none__(face);
  ok=edge_ok&&face_ok;
endfunction

function ok = __mf_opaque__(h)
  ok=false;
  for name={'markeredgealpha','markerfacealpha','edgealpha','facealpha','alphadata'}
    value=__mf_prop__(h,name{1},1);
    if ~isnumeric(value)||~isscalar(value)||~isreal(value)||value~=1,return;endif
  endfor
  for child=__mf_prop__(h,'children',[])(:).'
    if ~__mf_opaque__(child),return;endif
  endfor
  ok=true;
endfunction

function [color,ok] = __mf_rgb_or_none__(color)
  ok=ischar(color)&&strcmp(color,'none');
  if isnumeric(color)&&isreal(color)&&numel(color)==3&&all(isfinite(color(:)))&&all(color(:)>=0&color(:)<=1)
    color=double(color(:).'); ok=true;
  endif
endfunction

function legend_data = __mf_legend__(ax)
  legend_data=struct('visible',false,'location','','labels',{{}}); props=get(ax);
  if ~isfield(props,'__legend_handle__'),return;endif
  h=props.__legend_handle__;
  if isempty(h)||~isgraphics(h),return;endif
  legend_data.visible=strcmp(__mf_prop__(h,'visible','off'),'on');
  legend_data.location=__mf_prop__(h,'location',''); labels=__mf_prop__(h,'string',{});
  if ischar(labels),labels={labels};endif
  legend_data.labels=labels(:).';
endfunction

function text = __mf_graphics_text__(h)
  text=__mf_prop__(h,'string','');
  if iscell(text)
    parts={}; for k=1:numel(text),if ischar(text{k}),parts{end+1}=text{k};else parts{end+1}=num2str(text{k});endif;endfor
    text=strjoin(parts,' ');
  elseif isnumeric(text),text=num2str(text);endif
endfunction

function value = __mf_prop__(h,name,fallback)
  try value=get(h,name);catch value=fallback;end_try_catch
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
