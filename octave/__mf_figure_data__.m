function data = __mf_figure_data__(fig, source = struct('job','','figure',0))
  % Snapshot only: no set(), callbacks, evalin(), workspace or appdata writes.
  mlock();
  % Read-only pixel conversion, independent of the figure's units.
  if isgraphics(fig,'figure')
    figure_position=__get_position__(fig,'pixels');
    source.figure_size=__mf_array__(figure_position(3:4));
  endif
  limits=struct('samples',2000,'surface_vertices',40000,'patch_vertices',40000,...
    'patch_triangles',80000,'vertices',100000,'triangles',200000,'axes',16,...
    'series',128,'colormap',4096,'json_bytes',8388608,'objects',1024,'depth',16,...
    'source_samples',1000000,'ticks',4096,'global_objects',12289,...
    'text_lines',256,'text_chars',65536,'image_pixels',262144);
  data=struct('version',2,'supported',true,'decimated',false,'point_limit',2000,...
    'axes',{{}},'reason','','reason_code','','reason_args',struct(),...
    'source',source,'limits',limits,'vertex_count',0,'triangle_count',0,'pixel_count',0);
  [objects,tree,code,args]=__mf_walk__(fig,limits);
  if ~isempty(code),data=__mf_fail__(data,code,args);return;endif
  axes_handles=[]; bars=[];
  for h=objects
    if strcmp(__mf_prop__(h,'type',''),'axes')
      tag=__mf_prop__(h,'tag','');
      if strcmp(tag,'colorbar'),bars(end+1)=h;
      elseif ~strcmp(tag,'legend'),axes_handles(end+1)=h;endif
    endif
  endfor
  % Same bottom-to-top axes order as the old findall/reverse traversal.
  axes_handles=fliplr(axes_handles);
  % The annotation overlay has a hidden handle, so its place in the child list
  % is not its stacking position: Octave draws annotations over the plots.
  overlay=false(size(axes_handles));
  for k=1:numel(axes_handles),overlay(k)=strcmp(__mf_prop__(axes_handles(k),'tag',''),'scribeoverlay');endfor
  axes_handles=[axes_handles(~overlay),axes_handles(overlay)];
  if numel(axes_handles)>limits.axes,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('axes',numel(axes_handles),limits.axes));return;endif
  if isempty(axes_handles),data=__mf_fail__(data,'no_axes',struct());return;endif
  if ~isempty(bars),data.version=3;endif
  % Preflight aggregate upper bounds before conversion, grid expansion or sampling.
  vertices=0;triangles=0;pixels=0;count=0;leaves=cell(1,numel(axes_handles));
  for k=1:numel(axes_handles)
    ax=axes_handles(k);
    if ~isequal(get(ax,'view')(:).',[0 90]),data.version=3;endif
    if strcmp(__mf_prop__(ax,'visible','on'),'off'),data.version=3;endif
    [leaves{k},code,args,flattened]=__mf_leaves__(ax,tree);
    if flattened,data.version=3;endif
    if ~isempty(code),data=__mf_fail__(data,code,args);return;endif
    for leaf=leaves{k}
      h=leaf.h;type=__mf_prop__(h,'type','');
      count+=1;z=__mf_prop__(h,'zdata',[]);
      if ~isempty(z),data.version=3;endif
      if strcmp(type,'surface')
        data.version=3;n=numel(z);
        if n>limits.surface_vertices,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('surface_vertices',n,limits.surface_vertices));return;endif
        vertices+=n;sz=size(z);if numel(sz)==2,triangles+=2*max(0,sz(1)-1)*max(0,sz(2)-1);endif
      elseif strcmp(type,'patch')
        % Shape checks only: nothing is converted before the budgets pass.
        data.version=3;n=rows(__mf_prop__(h,'vertices',[]));f=size(__mf_prop__(h,'faces',[]));
        if n>limits.patch_vertices,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('patch_vertices',n,limits.patch_vertices));return;endif
        fan=f(1)*max(0,f(2)-2);
        if fan>limits.patch_triangles,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('patch_triangles',fan,limits.patch_triangles));return;endif
        vertices+=n;
      elseif strcmp(type,'image')
        data.version=3;c=__mf_prop__(h,'cdata',[]);sz=size(c);n=sz(1)*sz(2);pixels+=n;
        if pixels>limits.image_pixels,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('image_pixels',pixels,limits.image_pixels));return;endif
      elseif strcmp(type,'text')
        data.version=3;
      else
        vertices+=min(numel(__mf_prop__(h,'xdata',[])),limits.samples);
      endif
    endfor
  endfor
  if count>limits.series,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('series',count,limits.series));return;endif
  if vertices>limits.vertices,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('vertices',vertices,limits.vertices));return;endif
  if triangles>limits.triangles,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('triangles',triangles,limits.triangles));return;endif
  count=0;
  for k=1:numel(axes_handles)
    ax=axes_handles(k);
    [item,code,args]=__mf_axes__(ax,k-1,limits,tree,leaves{k});
    if item.dimension==3,data.version=3;endif
    if ~isempty(code),data=__mf_fail__(data,code,args);return;endif
    count+=numel(item.series);
    if count>limits.series,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('series',count,limits.series));return;endif
    for j=1:numel(item.series)
      s=item.series{j};
      data.vertex_count+=s.rendered_points;
      if strcmp(s.kind,'image'),data.pixel_count+=s.pixel_count;endif
      if strcmp(s.kind,'surface'),data.triangle_count+=s.triangle_count;data.version=3;endif
    endfor
    if data.vertex_count>limits.vertices,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('vertices',data.vertex_count,limits.vertices));return;endif
    if data.triangle_count>limits.triangles,data=__mf_fail__(data,'budget_exceeded',__mf_budget__('triangles',data.triangle_count,limits.triangles));return;endif
    data.axes{end+1}=item;
    data.decimated=data.decimated||item.decimated;
  endfor
  for h=bars
    if strcmp(__mf_prop__(h,'visible','on'),'off'),continue;endif
    peer=__mf_prop__(h,'__axes_handle__',[]);
    if ~isscalar(peer)||~isgraphics(peer,'axes'),data=__mf_fail__(data,'unsupported_colorbar',struct('property','__axes_handle__'));return;endif
    index=find(axes_handles==peer,1);
    if isempty(index),data=__mf_fail__(data,'unsupported_colorbar',struct('property','__axes_handle__'));return;endif
    [bar,code,args]=__mf_colorbar__(h,index-1,limits,tree);
    if ~isempty(code),data=__mf_fail__(data,code,args);return;endif
    data.axes{index}.colorbars{end+1}=bar;
  endfor
  % Figure-level UI/annotation containers would otherwise be silently omitted.
  for h=__mf_children__(fig,tree)
    type=__mf_prop__(h,'type','');
    if ~strcmp(type,'axes')&&~any(strcmp(type,{'uimenu','uicontextmenu','uitoolbar'}))&&strcmp(__mf_prop__(h,'visible','on'),'on')
      data=__mf_fail__(data,'unsupported_object',struct('type',type));return;
    endif
  endfor
endfunction

function [objects,tree,code,args]=__mf_walk__(fig,limits)
  objects=[];code='';args=struct();queue=fig;depths=0;
  % allchild/findall temporarily SET root ShowHiddenHandles. The internal
  % inventory includes hidden handles without any property writes/listeners.
  handles=__go_handles__(true);handles=handles(:).';
  tree=struct('handles',handles,'parents',[]);
  if numel(handles)>limits.global_objects
    code='budget_exceeded';args=__mf_budget__('global_objects',numel(handles),limits.global_objects);return;
  endif
  parents=get(handles,'parent');if ~iscell(parents),parents={parents};endif
  tree.parents=NaN(size(handles));
  for k=1:numel(handles)
    if ~isempty(parents{k}),tree.parents(k)=parents{k};endif
  endfor
  while ~isempty(queue)
    h=queue(1);depth=depths(1);queue(1)=[];depths(1)=[];
    if any(objects==h),continue;endif
    objects(end+1)=h;
    if numel(objects)>limits.objects,code='budget_exceeded';args=__mf_budget__('objects',numel(objects),limits.objects);return;endif
    children=__mf_children__(h,tree);n=numel(children);
    if n+numel(objects)+numel(queue)>limits.objects,code='budget_exceeded';args=__mf_budget__('objects',n+numel(objects)+numel(queue),limits.objects);return;endif
    if n&&depth>=limits.depth,code='budget_exceeded';args=__mf_budget__('depth',depth+1,limits.depth);return;endif
    queue=[queue,children];depths=[depths,repmat(depth+1,1,n)];
  endwhile
  keep=ismember(tree.handles,objects);tree.handles=tree.handles(keep);tree.parents=tree.parents(keep);
endfunction

function children=__mf_children__(h,tree)
  % Preserve the native visible-handle stacking order. Hidden children are
  % appended in inventory order; no source handle is sent to the browser.
  visible=get(h,'children');visible=visible(:).';
  all=tree.handles(tree.parents==h);
  children=[visible,all(~ismember(all,visible))];
endfunction

function children=__mf_axes_children__(ax,tree)
  children=__mf_children__(ax,tree);
  labels=[get(ax,'xlabel'),get(ax,'ylabel'),get(ax,'zlabel'),get(ax,'title')];
  children=children(~ismember(children,labels));
endfunction

function [a,code,args]=__mf_axes__(ax,id,limits,tree,leaves)
  code='';args=struct();
  a=struct('id',id,'dimension',2,'supported',true,'reason','','position',[],...
    'xlabel',__mf_graphics_text__(get(ax,'xlabel')),'ylabel',__mf_graphics_text__(get(ax,'ylabel')),...
    'zlabel',__mf_graphics_text__(get(ax,'zlabel')),'title',__mf_graphics_text__(get(ax,'title')),...
    'grid',struct(),'legend',__mf_legend__(ax),'decimated',false,'series',{{}},'colorbars',{{}});
  a.visible=strcmp(__mf_prop__(ax,'visible','on'),'on');
  a.interpreters=struct('title',__mf_prop__(get(ax,'title'),'interpreter','tex'),...
    'xlabel',__mf_prop__(get(ax,'xlabel'),'interpreter','tex'),'ylabel',__mf_prop__(get(ax,'ylabel'),'interpreter','tex'),...
    'ticks',__mf_prop__(ax,'ticklabelinterpreter','tex'));
  [a.position,ok]=__mf_position__(ax);
  if ~ok,code='unsupported_units';args=struct('property','units');return;endif
  % Export only the project's validated layout coordinates, never appdata.
  if isappdata(ax,'__mf_sgtitle_layout__') && strcmp(get(ax,'units'),'normalized')
    layout=getappdata(ax,'__mf_sgtitle_layout__');
    if isstruct(layout)&&isscalar(layout)&&isfield(layout,'base')&&isfield(layout,'last')&&...
        ~isfield(layout,'manual')&&__mf_real__(layout.base)&&isvector(layout.base)&&numel(layout.base)==4&&...
        all(isfinite(layout.base))&&all(layout.base(3:4)>0)&&isequal(layout.last,get(ax,'position'))
      a.title_layout_position=__mf_array__(layout.base);
    endif
  endif
  for axis='xyz'
    key=axis(1);
    a.([key 'lim'])=__mf_array__(get(ax,[key 'lim']));
    a.([key 'scale'])=get(ax,[key 'scale']);a.([key 'dir'])=get(ax,[key 'dir']);
    ticks=get(ax,[key 'tick']);
    if numel(ticks)>limits.ticks,code='budget_exceeded';args=__mf_budget__('ticks',numel(ticks),limits.ticks);return;endif
    a.([key 'tick'])=__mf_array__(ticks);
    a.([key 'ticklabel'])=__mf_labels__(get(ax,[key 'ticklabel']));
    a.([key 'tickmode'])=__mf_prop__(ax,[key 'tickmode'],'auto');
    a.([key 'ticklabelmode'])=__mf_prop__(ax,[key 'ticklabelmode'],'auto');
    a.grid.(key)=strcmp(get(ax,[key 'grid']),'on');
  endfor
  kids=__mf_axes_children__(ax,tree);baselines=__mf_baselines__(kids);
  view_value=get(ax,'view');
  if ~isequal(view_value(:).',[0 90]),a.dimension=3;endif
  for h=kids
    if strcmp(__mf_prop__(h,'type',''),'surface')||~isempty(__mf_prop__(h,'zdata',[])),a.dimension=3;endif
  endfor
  a.view=__mf_array__(view_value);
  a.camera=struct('projection',get(ax,'projection'),'position',{__mf_array__(get(ax,'cameraposition'))},...
    'target',{__mf_array__(get(ax,'cameratarget'))},'up_vector',{__mf_array__(get(ax,'cameraupvector'))},...
    'view_angle',get(ax,'cameraviewangle'));
  for pair={'position','target','upvector','viewangle'}
    name=pair{1};a.camera.([name '_mode'])=get(ax,['camera' name 'mode']);
  endfor
  a.data_aspect_ratio=__mf_array__(get(ax,'dataaspectratio'));
  a.data_aspect_ratio_mode=get(ax,'dataaspectratiomode');
  a.plot_box_aspect_ratio=__mf_array__(get(ax,'plotboxaspectratio'));
  a.plot_box_aspect_ratio_mode=get(ax,'plotboxaspectratiomode');
  a.clim=__mf_array__(get(ax,'clim'));
  map=get(ax,'colormap');
  if rows(map)>limits.colormap,code='budget_exceeded';args=__mf_budget__('colormap',rows(map),limits.colormap);return;endif
  if ~__mf_real__(map)||columns(map)~=3||any(~isfinite(map(:)))||any(map(:)<0|map(:)>1),code='invalid_data';args=struct('property','colormap');return;endif
  a.colormap=__mf_descriptor__(map);
  if a.dimension==3
    if any(strcmp({a.xscale,a.yscale,a.zscale},'log')),code='log_3d';return;endif
    if ~strcmp(a.camera.projection,'orthographic'),code='perspective';return;endif
    for name={'position_mode','target_mode','upvector_mode','viewangle_mode'}
      if ~strcmp(a.camera.(name{1}),'auto'),code='manual_camera';args=struct('property',name{1});return;endif
    endfor
  endif
  lights=[];
  for h=kids
    if strcmp(__mf_prop__(h,'type',''),'light')&&strcmp(__mf_prop__(h,'visible','on'),'on'),lights(end+1)=h;endif
  endfor
  fixed_aspect=strcmp(a.data_aspect_ratio_mode,'manual')||strcmp(a.plot_box_aspect_ratio_mode,'manual');
  for leaf=leaves
    type=__mf_prop__(leaf.h,'type','');
    if strcmp(type,'image')
      if a.dimension~=2||any(strcmp({a.xscale,a.yscale},'log')),code='unsupported_object';args=struct('type','image');return;endif
    endif
    if any(strcmp(type,{'patch','text'}))
      % Filled shapes and free text exist only in the 2D viewer.
      if a.dimension==3,code='unsupported_object';args=struct('type',type);return;endif
      if strcmp(type,'patch')
        if any(strcmp({a.xscale,a.yscale},'log')),code='unsupported_patch';args=struct('property','scale');return;endif
        % The 2D viewer stretches axes to their box; a fixed aspect ratio
        % (pie, axis equal) would visibly distort filled shapes.
        if fixed_aspect&&~strcmp(leaf.role,'textbox'),code='unsupported_patch';args=struct('property','dataaspectratio');return;endif
      endif
    endif
    [s,code,args]=__mf_series__(leaf,ax,limits,~isempty(lights),tree);
    if ~isempty(code),return;endif
    s.id=numel(a.series);a.series{end+1}=s;a.decimated=a.decimated||s.decimated;
  endfor
endfunction

function [leaves,code,args,flattened]=__mf_leaves__(ax,tree)
  % Drawable leaves in Octave's drawing order (last child first). Known
  % groups are flattened into their real children; nothing is recomputed.
  leaves=struct('h',{},'role',{},'group',{});code='';args=struct();flattened=false;
  kids=__mf_axes_children__(ax,tree);baselines=__mf_baselines__(kids);
  overlay=strcmp(__mf_prop__(ax,'tag',''),'scribeoverlay');
  for h=fliplr(kids)
    type=__mf_prop__(h,'type','');
    if any(h==baselines)||strcmp(__mf_prop__(h,'visible','on'),'off')||strcmp(type,'light'),continue;endif
    kind='';creator='';
    if strcmp(type,'hggroup')
      try creator=getappdata(h,'__creator__');catch end_try_catch
      if ~ischar(creator),creator='';endif
      if __mf_has__(h,'bargroup'),kind='bar';allowed={'patch','line'};
      elseif __mf_has__(h,'areagroup'),kind='area';allowed={'patch'};
      elseif strcmp(creator,'__errplot__'),kind='errorbar';allowed={'line'};
      elseif any(strcmp(creator,{'xline','yline'})),kind=creator;allowed={'line','text'};
      elseif overlay&&__mf_has__(h,'fitboxtotext'),kind='textbox';allowed={'patch','text'};
      endif
    endif
    if isempty(kind),leaves(end+1)=struct('h',h,'role',type,'group',NaN);continue;endif
    flattened=true;
    alpha=1;if isprop(h,'alpha'),alpha=get(h,'alpha');endif
    if ~isnumeric(alpha)||~isscalar(alpha)||alpha~=1,code='transparency';return;endif
    children=__mf_children__(h,tree);found=struct('h',{},'role',{},'group',{});
    for wanted=allowed
      for c=children
        ctype=__mf_prop__(c,'type','');
        if ~any(strcmp(ctype,allowed)),code='unsupported_group';args=struct('type',kind,'creator',creator);return;endif
        if ~strcmp(ctype,wanted{1})||strcmp(__mf_prop__(c,'visible','on'),'off'),continue;endif
        found(end+1)=struct('h',c,'role',kind,'group',h);
      endfor
    endfor
    leaves=[leaves,found];
  endfor
endfunction

function ok=__mf_has__(h,name)
  ok=isprop(h,name); % exact names only; get() would accept abbreviations
endfunction

function baselines=__mf_baselines__(kids)
  baselines=[];
  for h=kids
    if strcmp(__mf_prop__(h,'type',''),'hggroup')&&~isempty(__mf_prop__(h,'basevalue',[]))
      baseline=__mf_prop__(h,'baseline',[]);baselines=[baselines,baseline(:).'];
    endif
  endfor
endfunction

function [s,code,args]=__mf_series__(leaf,ax,limits,has_light,tree)
  h=leaf.h;s=[];code='';args=struct();type=__mf_prop__(h,'type','');kind=type;
  if strcmp(type,'surface'),[s,code,args]=__mf_surface__(h,limits,has_light,tree);return;endif
  if strcmp(type,'patch'),[s,code,args]=__mf_patch__(leaf,ax,limits,has_light,tree);return;endif
  if strcmp(type,'text'),[s,code,args]=__mf_text_record__(leaf,limits);return;endif
  if strcmp(type,'image'),[s,code,args]=__mf_image__(h,limits,tree);return;endif
  if strcmp(type,'hggroup')
    children=__mf_children__(h,tree);creator='';
    try creator=getappdata(h,'__creator__');catch end_try_catch
    if strcmp(creator,'__stem__'),kind='stem';
    elseif numel(children)==1&&strcmp(__mf_prop__(children(1),'type',''),'line')&&~isempty(__mf_prop__(h,'xdata',[])),kind='stairs';
    else code='unsupported_group';args=struct('type',type,'creator',creator);return;endif
    if ~isempty(__mf_prop__(h,'zdata',[])),code='unsupported_group';args=struct('type','3d_hggroup');return;endif
  elseif ~any(strcmp(type,{'line','scatter'})),code='unsupported_object';args=struct('type',type);return;endif
  if ~__mf_opaque__(h,tree),code='transparency';return;endif
  x=__mf_prop__(h,'xdata',[]);y=__mf_prop__(h,'ydata',[]);z=__mf_prop__(h,'zdata',[]);n=numel(x);
  if ~__mf_vector__(x)||~__mf_vector__(y)||numel(y)~=n||(~isempty(z)&&(~__mf_vector__(z)||numel(z)~=n)),code='invalid_data';args=struct('property','coordinates');return;endif
  if n>limits.source_samples,code='budget_exceeded';args=__mf_budget__('source_samples',n,limits.source_samples);return;endif
  is3=~isempty(z);
  if is3&&strcmp(kind,'scatter')&&n>limits.samples,code='budget_exceeded';args=__mf_budget__('samples',n,limits.samples);return;endif
  [idx,ok,anchor_count]=__mf_sample__(x,y,z,limits.samples);
  if ~ok,code='budget_exceeded';args=__mf_budget__('segment_anchors',anchor_count,limits.samples);return;endif
  [line,edge,face,color_ok,color_code]=__mf_colors__(h,ax,kind,is3);
  if ~color_ok,code=color_code;return;endif
  marker=__mf_prop__(h,'marker','none');
  if ~any(strcmp(marker,{'none','o','s','square','^','v','>','<','.','+','x','*'})),code='unsupported_marker';args=struct('marker',marker);return;endif
  s=struct('kind',kind,'x',{__mf_array__(x(idx))},'y',{__mf_array__(y(idx))},...
    'line_color',line,'marker_edge_color',edge,'marker_face_color',face,...
    'marker_face_auto',strcmp(__mf_prop__(h,'markerfacecolor','none'),'auto'),...
    'line_style',__mf_prop__(h,'linestyle','none'),'line_width',double(__mf_prop__(h,'linewidth',.5)),...
    'marker',marker,'marker_size',double(__mf_prop__(h,'markersize',6)),...
    'display_name',__mf_prop__(h,'displayname',''),'base_value',double(__mf_prop__(h,'basevalue',0)),...
    'decimated',numel(idx)<n,'original_points',n,'rendered_points',numel(idx),...
    'source_indices',{__mf_array__(idx-1)});
  if is3,s.z=__mf_array__(z(idx));endif
  if strcmp(kind,'scatter')
    sizes=__mf_prop__(h,'sizedata',36);
    if ~__mf_real__(sizes)||~(isscalar(sizes)||(__mf_vector__(sizes)&&numel(sizes)==n))||any(~isfinite(sizes(:)))||any(sizes(:)<0),s=[];code='invalid_data';args=struct('property','sizedata');return;endif
    if ~isscalar(sizes),sizes=sizes(idx);endif
    s.sizes=__mf_array__(sizes);s.size_units='points-squared';
    if is3
      c=get(h,'cdata');encoding='truecolor';if isscalar(c),encoding='indexed';endif
      s.color_data=struct('encoding',encoding,'mapping','scaled','cdata_class',class(c),'data',__mf_descriptor__(c));
    endif
  endif
  if ~isnan(leaf.group)
    % A flattened group child: the group's name labels its first line only.
    s.role=leaf.role;
    s.display_name='';
    if any(strcmp(leaf.role,{'errorbar','xline','yline'}))&&__mf_first_line__(leaf,tree)
      s.display_name=__mf_prop__(leaf.group,'displayname','');
    endif
    % Baselines and constant lines cross the whole axes in Octave (listeners
    % keep them at the limits); the viewer extends them over its own view.
    if any(strcmp(leaf.role,{'bar','xline','yline'}))&&n==2&&~is3&&all(isfinite(x))&&all(isfinite(y))
      if x(1)==x(2)&&y(1)~=y(2),s.span='vertical';
      elseif y(1)==y(2)&&x(1)~=x(2),s.span='horizontal';endif
    endif
  endif
endfunction

function [s,code,args]=__mf_image__(h,limits,tree)
  s=[];code='';args=struct();c=get(h,'cdata');shape=size(c);n=shape(1)*shape(2);
  if n>limits.image_pixels,code='budget_exceeded';args=__mf_budget__('image_pixels',n,limits.image_pixels);return;endif
  if ~__mf_real__(c)||isempty(c)||~any(strcmp(class(c),{'double','single','uint8','uint16'}))||...
      ~(ndims(c)==2||(ndims(c)==3&&shape(3)==3))||any(~isfinite(c(:)))
    code='invalid_data';args=struct('property','image.cdata');return;
  endif
  if ~__mf_opaque__(h,tree)||~strcmp(get(h,'alphadatamapping'),'none'),code='transparency';return;endif
  x=get(h,'xdata');y=get(h,'ydata');
  for pair={x,y}
    v=pair{1};if ~__mf_real__(v)||numel(v)~=2||any(~isfinite(v)),code='invalid_data';args=struct('property','image.coordinates');return;endif
  endfor
  % A singleton with distinct endpoints (or a collapsed multi-pixel range)
  % has ambiguous native raster placement. Keep its exact Octave PNG.
  if (shape(2)==1&&x(1)~=x(2))||(shape(1)==1&&y(1)~=y(2))||...
      (shape(2)>1&&x(1)==x(2))||(shape(1)>1&&y(1)==y(2))
    code='unsupported_object';args=struct('type','image_coordinates');return;
  endif
  for item={struct('v',x,'n',shape(2)),struct('v',y,'n',shape(1))}
    entry=item{1};step=1;if entry.n>1,step=(entry.v(2)-entry.v(1))/(entry.n-1);endif
    if ~isfinite(step)||any(~isfinite([entry.v(1)-step/2,entry.v(2)+step/2])),code='invalid_data';args=struct('property','image.coordinates');return;endif
  endfor
  encoding='indexed';if ndims(c)==3,encoding='truecolor';endif
  mapping=get(h,'cdatamapping');
  if ~any(strcmp(mapping,{'scaled','direct'})),code='unsupported_color';return;endif
  s=struct('kind','image','shape',{__mf_array__(shape(1:2))},'x',{__mf_array__(x)},'y',{__mf_array__(y)},...
    'cdata',__mf_descriptor__(c),'encoding',encoding,'mapping',mapping,'cdata_class',class(c),...
    'pixel_count',n,'display_name',__mf_prop__(h,'displayname',''),'line_style','none',...
    'decimated',false,'original_points',0,'rendered_points',0);
endfunction

function first=__mf_first_line__(leaf,tree)
  first=false;
  for c=__mf_children__(leaf.group,tree)
    if strcmp(__mf_prop__(c,'type',''),'line')&&strcmp(__mf_prop__(c,'visible','on'),'on'),first=(c==leaf.h);return;endif
  endfor
endfunction

function [s,code,args]=__mf_patch__(leaf,ax,limits,has_light,tree)
  h=leaf.h;s=[];code='';args=struct();v=get(h,'vertices');f=get(h,'faces');
  if ~__mf_real__(v)||ndims(v)~=2||isempty(v)||~any(columns(v)==[2 3]),code='invalid_data';args=struct('property','vertices');return;endif
  n=rows(v);
  if n>limits.patch_vertices,code='budget_exceeded';args=__mf_budget__('patch_vertices',n,limits.patch_vertices);return;endif
  if ~__mf_real__(f)||ndims(f)~=2||isempty(f),code='invalid_data';args=struct('property','faces');return;endif
  fan=rows(f)*max(0,columns(f)-2);
  if fan>limits.patch_triangles,code='budget_exceeded';args=__mf_budget__('patch_triangles',fan,limits.patch_triangles);return;endif
  if columns(v)==3&&~all(v(:,3)==0),code='unsupported_patch';args=struct('property','vertices');return;endif
  f=double(f);used=~isnan(f);
  % Indices are one-based integers; NaN is only trailing padding of a face.
  if any(f(used)<1|f(used)>n|f(used)~=fix(f(used)))||any(any(diff(used,1,2)>0)),code='invalid_data';args=struct('property','faces');return;endif
  if ~__mf_opaque__(h,tree),code='transparency';return;endif
  if has_light,code='lighting';return;endif
  if ~strcmp(__mf_prop__(h,'marker','none'),'none'),code='unsupported_patch';args=struct('property','marker');return;endif
  [face,code]=__mf_patch_color__(get(h,'facecolor'),h,ax);
  if ~isempty(code),return;endif
  [edge,code]=__mf_patch_color__(get(h,'edgecolor'),h,ax);
  if ~isempty(code),return;endif
  name=__mf_prop__(h,'displayname','');
  if ~isnan(leaf.group),name=__mf_prop__(leaf.group,'displayname','');endif
  role=leaf.role;
  s=struct('kind','patch2d','role',role,'vertices',__mf_descriptor__(v(:,1:2)),...
    'faces',__mf_descriptor__(f-1),'index_base',0,'face_color',face,'edge_color',edge,...
    'line_style',get(h,'linestyle'),'line_width',double(get(h,'linewidth')),...
    'display_name',name,'decimated',false,'original_points',n,'rendered_points',n);
  if strcmp(role,'bar')
    % The group's own data describes each face for data tips; the drawn
    % geometry stays the patch Octave computed.
    g=leaf.group;x=__mf_prop__(g,'xdata',[]);y=__mf_prop__(g,'ydata',[]);
    if __mf_vector__(x)&&__mf_vector__(y)&&numel(x)==rows(f)&&numel(y)==rows(f)
      s.bar=struct('horizontal',strcmp(__mf_prop__(g,'horizontal','off'),'on'),...
        'layout',__mf_prop__(g,'barlayout',''),'width',double(__mf_prop__(g,'barwidth',NaN)),...
        'base_value',double(__mf_prop__(g,'basevalue',0)),...
        'positions',{__mf_array__(x)},'values',{__mf_array__(y)});
      if ~isfinite(s.bar.width),s.bar.width=0;endif
      if ~isfinite(s.bar.base_value),s.bar.base_value=0;endif
    endif
  endif
endfunction

function [color,code]=__mf_patch_color__(value,h,ax)
  % One colour for the whole patch, or the figure keeps its PNG.
  code='';color='none';
  if ~ischar(value)
    [color,ok]=__mf_rgb_or_none__(value);
    if ~ok,code='unsupported_color';endif
    return;
  endif
  if strcmp(value,'none'),return;endif
  if strcmp(value,'interp'),code='interpolated_color';return;endif
  if ~strcmp(value,'flat'),code='unsupported_color';return;endif
  c=get(h,'facevertexcdata');
  if ~__mf_real__(c)||isempty(c),code='unsupported_color';return;endif
  if rows(c)~=1||~any(columns(c)==[1 3]),code='patch_colors';return;endif
  if columns(c)==3
    [color,ok]=__mf_rgb_or_none__(double(c));
    if ~ok||~isfloat(c),color='none';code='unsupported_color';endif
    return;
  endif
  % Scalar scaled CData: the same colormap index rule as the 3D scatter.
  map=get(ax,'colormap');clim=get(ax,'clim');c=double(c);
  if ~strcmp(get(h,'cdatamapping'),'scaled')||~isfinite(c)||isempty(map)||columns(map)~=3||~(clim(2)>clim(1))
    code='unsupported_color';return;
  endif
  index=min(rows(map),max(1,1+fix(rows(map)*(c-clim(1))/(clim(2)-clim(1)))));
  [color,ok]=__mf_rgb_or_none__(map(index,:));
  if ~ok,color='none';code='unsupported_color';endif
endfunction

function [s,code,args]=__mf_text_record__(leaf,limits)
  h=leaf.h;s=[];code='';args=struct();
  units=get(h,'units');
  if ~any(strcmp(units,{'data','normalized'})),code='unsupported_text';args=struct('property','units');return;endif
  if ~strcmp(get(h,'fontunits'),'points'),code='unsupported_text';args=struct('property','fontunits');return;endif
  interpreter=get(h,'interpreter');
  if ~any(strcmp(interpreter,{'tex','none'})),code='unsupported_text';args=struct('property','interpreter');return;endif
  position=get(h,'position');rotation=get(h,'rotation');
  if ~__mf_real__(position)||numel(position)<2||any(~isfinite(position(1:2))),code='invalid_data';args=struct('property','text.position');return;endif
  if ~__mf_real__(rotation)||~isscalar(rotation)||~isfinite(rotation),code='invalid_data';args=struct('property','text.rotation');return;endif
  [lines,ok]=__mf_text_lines__(get(h,'string'));
  if ~ok,code='invalid_data';args=struct('property','text.string');return;endif
  if numel(lines)>limits.text_lines,code='budget_exceeded';args=__mf_budget__('text_lines',numel(lines),limits.text_lines);return;endif
  chars=sum(cellfun('numel',lines));
  if chars>limits.text_chars,code='budget_exceeded';args=__mf_budget__('text_chars',chars,limits.text_chars);return;endif
  [color,ok1]=__mf_rgb_or_none__(get(h,'color'));
  [background,ok2]=__mf_rgb_or_none__(get(h,'backgroundcolor'));
  [edge,ok3]=__mf_rgb_or_none__(get(h,'edgecolor'));
  if ~(ok1&&ok2&&ok3)||ischar(color),code='unsupported_color';return;endif
  size_value=double(get(h,'fontsize'));margin=double(get(h,'margin'));
  if ~isscalar(size_value)||~isfinite(size_value)||size_value<=0||~isscalar(margin)||~isfinite(margin)||margin<0
    code='invalid_data';args=struct('property','text.fontsize');return;
  endif
  weight='normal';if any(strcmp(get(h,'fontweight'),{'bold','demi'})),weight='bold';endif
  angle='normal';if any(strcmp(get(h,'fontangle'),{'italic','oblique'})),angle='italic';endif
  s=struct('kind','text','role',leaf.role,'units',units,'position',{__mf_array__(position(1:2))},...
    'lines',{lines},'interpreter',interpreter,...
    'horizontal_alignment',get(h,'horizontalalignment'),'vertical_alignment',get(h,'verticalalignment'),...
    'rotation',mod(double(rotation),360),'font_size',size_value,'font_weight',weight,'font_angle',angle,...
    'color',color,'background_color',background,'edge_color',edge,'margin',margin,...
    'line_style',get(h,'linestyle'),'line_width',double(get(h,'linewidth')),...
    'clipping',strcmp(get(h,'clipping'),'on'),'display_name','',...
    'decimated',false,'original_points',0,'rendered_points',0);
  if ~isnan(leaf.group)&&strcmp(__mf_prop__(leaf.group,'tag',''),'__mf_sgtitle__')&&...
      isappdata(leaf.group,'__mf_sgtitle_auto__')&&isequal(getappdata(leaf.group,'__mf_sgtitle_auto__'),true)&&...
      isappdata(leaf.group,'__mf_sgtitle_auto_geometry__')
    geometry=getappdata(leaf.group,'__mf_sgtitle_auto_geometry__');
    % Post-creation user geometry edits end browser automatic title placement.
    if isstruct(geometry)&&isscalar(geometry)&&isfield(geometry,'position')&&isfield(geometry,'units')&&...
        isequal(geometry.position,get(leaf.group,'position'))&&isequal(geometry.units,get(leaf.group,'units'))
      s.figure_title=true;
    endif
  endif
endfunction

function [lines,ok]=__mf_text_lines__(value)
  % Cell elements and char-matrix rows are lines; a newline splits further.
  lines={};ok=true;
  if isnumeric(value)&&isreal(value),value=num2str(value);endif
  if ischar(value),value=cellstr(value);endif
  if ~iscell(value),ok=false;return;endif
  for k=1:numel(value)
    item=value{k};
    if isnumeric(item)&&isreal(item),item=num2str(item);endif
    if ~ischar(item)||rows(item)>1,ok=false;lines={};return;endif
    lines=[lines,strsplit(item,char(10))];
  endfor
endfunction

function [s,code,args]=__mf_surface__(h,limits,has_light,tree)
  s=[];code='';args=struct();z=get(h,'zdata');shape=size(z);n=numel(z);
  if ~__mf_real__(z)||ndims(z)~=2||any(shape<2),code='invalid_data';args=struct('property','zdata');return;endif
  if n>limits.surface_vertices,code='budget_exceeded';args=__mf_budget__('surface_vertices',n,limits.surface_vertices);return;endif
  x=get(h,'xdata');y=get(h,'ydata');
  if ~__mf_real__(x)||~__mf_real__(y),code='invalid_data';args=struct('property','coordinates');return;endif
  if isvector(x)&&numel(x)==shape(2),xl='vector';
  elseif isequal(size(x),shape),xl='matrix';else code='invalid_data';args=struct('property','xdata');return;endif
  if isvector(y)&&numel(y)==shape(1),yl='vector';
  elseif isequal(size(y),shape),yl='matrix';else code='invalid_data';args=struct('property','ydata');return;endif
  if ~__mf_opaque__(h,tree),code='transparency';return;endif
  if has_light&&((~strcmp(get(h,'facecolor'),'none')&&~strcmp(get(h,'facelighting'),'none'))||(~strcmp(get(h,'edgecolor'),'none')&&~strcmp(get(h,'edgelighting'),'none'))),code='lighting';return;endif
  c=get(h,'cdata');
  if ~__mf_real__(c)||~(isequal(size(c),shape)||isequal(size(c),[shape 3])),code='invalid_data';args=struct('property','cdata');return;endif
  if (isa(c,'uint64')&&any(c(:)>uint64(flintmax()-1)))||...
     (isa(c,'int64')&&any(c(:)>int64(flintmax()-1)|c(:)<-int64(flintmax()-1)))
    code='unsupported_color';return;
  endif
  if ndims(c)==3
    if ~any(strcmp(class(c),{'double','single','uint8','uint16'})),code='unsupported_color';return;endif
    maximum=1;if isa(c,'uint8'),maximum=255;elseif isa(c,'uint16'),maximum=65535;endif
    if any(c(:)<0|c(:)>maximum),code='unsupported_color';return;endif
  endif
  [face,code]=__mf_surface_color__(get(h,'facecolor'),c,get(h,'cdatamapping'));
  if ~isempty(code),return;endif
  [edge,code]=__mf_surface_color__(get(h,'edgecolor'),c,get(h,'cdatamapping'),'vertex');
  if ~isempty(code),return;endif
  style=get(h,'meshstyle');
  if ~any(strcmp(style,{'both','row','column'})),code='unsupported_surface';args=struct('property','meshstyle');return;endif
  marker=get(h,'marker');
  if ~strcmp(marker,'none'),code='unsupported_surface';args=struct('property','marker');return;endif
  valid=isfinite(z);
  if strcmp(xl,'vector'),valid &= repmat(isfinite(x(:).'),shape(1),1);else valid &= isfinite(x);endif
  if strcmp(yl,'vector'),valid &= repmat(isfinite(y(:)),1,shape(2));else valid &= isfinite(y);endif
  cells=valid(1:end-1,1:end-1)&valid(2:end,1:end-1)&valid(1:end-1,2:end)&valid(2:end,2:end);
  [rr,cc]=find(cells);origins=(rr(:)-1)+(cc(:)-1)*shape(1);
  edges=zeros(0,2);
  if any(strcmp(style,{'both','column'}))
    [rr,cc]=find(valid(1:end-1,:)&valid(2:end,:));i=(rr(:)-1)+(cc(:)-1)*shape(1);edges=[edges; i,i+1];
  endif
  if any(strcmp(style,{'both','row'}))
    [rr,cc]=find(valid(:,1:end-1)&valid(:,2:end));i=(rr(:)-1)+(cc(:)-1)*shape(1);edges=[edges; i,i+shape(1)];
  endif
  % Nonfinite CData is allowed only when no rendered face/edge reads it.
  color_owners=[];
  if strcmp(face.mode,'flat'),color_owners=[color_owners;origins(:)];endif
  if strcmp(edge.mode,'flat'),color_owners=[color_owners;edges(:,1)];endif
  for channel=1:size(c,3)
    plane=c(:,:,channel);
    if any(~isfinite(plane(color_owners+1))),code='unsupported_color';return;endif
  endfor
  s=struct('cdata_class',class(c),'cdata_mapping',get(h,'cdatamapping'),'cdata',__mf_descriptor__(c),...
    'kind','surface','shape',{__mf_array__(shape)},'coordinate_layout',struct('x',xl,'y',yl),...
    'x',__mf_descriptor__(x),'y',__mf_descriptor__(y),'z',__mf_descriptor__(z),...
    'face_color',face,'edge_color',edge,'cell_color_owner','row-column-origin',...
    'edge_color_owner','first-vertex','mesh_style',style,...
    'line_style',get(h,'linestyle'),'line_width',double(get(h,'linewidth')),...
    'display_name',get(h,'displayname'),'decimated',false,'original_points',n,...
    'rendered_points',n,'cell_origins',{__mf_array__(origins)},...
    'edge_indices',__mf_descriptor__(edges),'index_base',0,'triangle_count',2*numel(origins));
endfunction

function [color,code]=__mf_surface_color__(value,c,mapping,association='cell')
  code='';color=struct('mode','none','association','constant');
  if ischar(value)
    if strcmp(value,'none'),return;
    elseif strcmp(value,'interp'),code='interpolated_color';return;
    elseif ~strcmp(value,'flat'),code='unsupported_surface';return;endif
    if ~any(strcmp(mapping,{'scaled','direct'})),code='unsupported_color';return;endif
      encoding='indexed';if ndims(c)==3,encoding='truecolor';endif
    color=struct('mode','flat','association',association,'encoding',encoding,'mapping',mapping,...
      'cdata_class',class(c),'data',__mf_descriptor__(c));
  else
    [rgb,ok]=__mf_rgb_or_none__(value);
    if ~ok||ischar(rgb),code='unsupported_color';return;endif
    color=struct('mode','constant','association','constant','rgb',rgb);
  endif
endfunction

function [idx,ok,anchor_count]=__mf_sample__(x,y,z,limit)
  n=numel(x);idx=1:n;ok=true;anchor_count=n;if n<=limit,return;endif
  valid=isfinite(x(:).')&isfinite(y(:).');if ~isempty(z),valid &= isfinite(z(:).');endif
  starts=find(valid&[true,~valid(1:end-1)]);ends=find(valid&[~valid(2:end),true]);
  gaps=find(~valid&[true,valid(1:end-1)]);
  anchors=[1,n,starts,ends,gaps];
  valid_indices=find(valid);
  if ~isempty(valid_indices)
    for values={x,y,z}
      v=values{1};if isempty(v),continue;endif
      [~,lo]=min(v(valid_indices));[~,hi]=max(v(valid_indices));
      anchors=[anchors,valid_indices(lo),valid_indices(hi)];
    endfor
  endif
  anchors=unique(anchors);anchor_count=numel(anchors);
  if numel(anchors)>limit,idx=[];ok=false;return;endif
  stride=4;if ~isempty(z),stride=6;endif
  buckets=floor((limit-numel(anchors))/stride);idx=anchors;
  if buckets>0
    boundaries=round(linspace(1,n+1,buckets+1));
    for k=1:buckets
      r=boundaries(k):boundaries(k+1)-1;r=r(valid(r));if isempty(r),continue;endif
      for values={x,y,z}
        v=values{1};if isempty(v),continue;endif
        [~,lo]=min(v(r));[~,hi]=max(v(r));idx=[idx,r(lo),r(hi)];
      endfor
    endfor
  endif
  idx=sort(unique(idx));
endfunction

function [line,edge,face,ok,code]=__mf_colors__(h,ax,kind,is3)
  ok=false;code='unsupported_color';line='none';edge='none';face='none';
  if strcmp(kind,'scatter')
    c=__mf_prop__(h,'cdata',[]);
    if ~isempty(c)&&~(isnumeric(c)&&(isscalar(c)||isequal(size(c),[1 3]))),code='scatter_colors';return;endif
    base=c;
    if is3&&isscalar(c)
      map=get(ax,'colormap');clim=get(ax,'clim');
      if ~isfinite(c)||isempty(map),return;endif
      index=min(rows(map),max(1,1+fix(rows(map)*(double(c)-clim(1))/(clim(2)-clim(1)))));
      base=map(index,:);
    endif
  else
    [line,valid]=__mf_rgb_or_none__(__mf_prop__(h,'color','none'));if ~valid,return;endif
    base=line;
  endif
  edge=__mf_prop__(h,'markeredgecolor','auto');face=__mf_prop__(h,'markerfacecolor','none');
  if ischar(edge)&&any(strcmp(edge,{'auto','flat'})),edge=base;endif
  if ischar(face)&&strcmp(face,'flat')&&strcmp(kind,'scatter'),face=base;
  elseif ischar(face)&&strcmp(face,'auto'),face=__mf_prop__(ax,'color','none');endif
  [edge,edge_ok]=__mf_rgb_or_none__(edge);[face,face_ok]=__mf_rgb_or_none__(face);ok=edge_ok&&face_ok;
endfunction

function ok=__mf_opaque__(h,tree)
  ok=false;
  for name={'markeredgealpha','markerfacealpha','edgealpha','facealpha','alphadata'}
    % get() accepts abbreviations: 'alphadata' would read a patch's alphadatamapping.
    if ~isprop(h,name{1}),continue;endif
    value=__mf_prop__(h,name{1},1);
    if ~isnumeric(value)||~isscalar(value)||~isreal(value)||value~=1,return;endif
  endfor
  % Group descendants are bounded globally before this helper is entered.
  for child=__mf_children__(h,tree)
    if ~__mf_opaque__(child,tree),return;endif
  endfor
  ok=true;
endfunction

function [bar,code,args]=__mf_colorbar__(h,peer,limits,tree)
  code='';args=struct();bar=[];[position,ok]=__mf_position__(h);
  if ~ok,code='unsupported_units';args=struct('property','colorbar.units');return;endif
  vertical=__mf_prop__(h,'__vertical__','');
  if ~any(strcmp(vertical,{'on','off'})),code='unsupported_colorbar';args=struct('property','__vertical__');return;endif
  axis='x';orientation='horizontal';if strcmp(vertical,'on'),axis='y';orientation='vertical';endif
  map=get(h,'colormap');
  if rows(map)>limits.colormap,code='budget_exceeded';args=__mf_budget__('colormap',rows(map),limits.colormap);return;endif
  if ~__mf_real__(map)||columns(map)~=3||any(~isfinite(map(:)))||any(map(:)<0|map(:)>1),code='invalid_data';args=struct('property','colorbar.colormap');return;endif
  children=__mf_axes_children__(h,tree);
  if numel(children)~=1||~strcmp(__mf_prop__(children(1),'type',''),'image')||~__mf_opaque__(children(1),tree)
    code='unsupported_colorbar';args=struct('property','children');return;
  endif
  expected=1:rows(map);if strcmp(vertical,'on'),expected=expected(:);endif
  if ~isequal(__mf_prop__(children(1),'cdata',[]),expected)
    code='unsupported_colorbar';args=struct('property','children.cdata');return;
  endif
  ticks=get(h,[axis 'tick']);
  if numel(ticks)>limits.ticks,code='budget_exceeded';args=__mf_budget__('ticks',numel(ticks),limits.ticks);return;endif
  bar=struct('peer_axes',peer,'position',{position},'orientation',orientation,...
    'location',__mf_prop__(h,'location',''),'limits',{__mf_array__(get(h,[axis 'lim']))},...
    'ticks',{__mf_array__(get(h,[axis 'tick']))},'tick_labels',{__mf_labels__(get(h,[axis 'ticklabel']))},...
    'direction',get(h,[axis 'dir']),'axis_location',get(h,[axis 'axislocation']),...
    'label',__mf_graphics_text__(__mf_prop__(h,'label',get(h,[axis 'label']))),...
    'colormap',__mf_descriptor__(map));
endfunction

function [position,ok]=__mf_position__(h)
  ok=true;
  % Built-in read-only conversion. Unlike changing Units, no listeners run.
  try position=__mf_array__(__get_position__(h,'normalized'));
  catch position=[];ok=false;end_try_catch
  if ~strcmp(__mf_prop__(get(h,'parent'),'type',''),'figure'),ok=false;endif
endfunction

function d=__mf_descriptor__(value)
  d=struct('shape',{__mf_array__(size(value))},'order','column-major','values',{__mf_array__(value)});
endfunction
function a=__mf_array__(value)
  a=num2cell(double(value(:).')); % jsonencode preserves empty/singleton arrays.
endfunction
function ok=__mf_real__(value)
  ok=isnumeric(value)&&isreal(value)&&~issparse(value);
endfunction
function ok=__mf_vector__(value)
  ok=__mf_real__(value)&&(isvector(value)||isempty(value));
endfunction
function args=__mf_budget__(name,actual,limit)
  args=struct('budget',name,'actual',actual,'limit',limit);
endfunction
function data=__mf_fail__(data,code,args)
  data.supported=false;data.axes={};data.decimated=false;data.vertex_count=0;data.triangle_count=0;data.pixel_count=0;
  data.reason_code=code;data.reason_args=args;
  data.reason=sprintf(__mf_text__('Only the PNG view is available: %s.', 'Yalnızca PNG görünümü kullanılabilir: %s.'),code);
endfunction
function [color,ok]=__mf_rgb_or_none__(color)
  ok=ischar(color)&&strcmp(color,'none');
  if isnumeric(color)&&isreal(color)&&numel(color)==3&&all(isfinite(color(:)))&&all(color(:)>=0&color(:)<=1)
    color=double(color(:).');ok=true;
  endif
endfunction
function legend_data=__mf_legend__(ax)
  legend_data=struct('visible',false,'location','','labels',{{}});
  h=__mf_prop__(ax,'__legend_handle__',[]);if isempty(h)||~isgraphics(h),return;endif
  legend_data.visible=strcmp(__mf_prop__(h,'visible','off'),'on');legend_data.location=__mf_prop__(h,'location','');
  legend_data.labels=__mf_labels__(__mf_prop__(h,'string',{}));
endfunction
function labels=__mf_labels__(value)
  if ischar(value),labels=cellstr(value);elseif iscell(value),labels=value(:).';else labels=cellstr(num2str(value));endif
endfunction
function text=__mf_graphics_text__(h)
  text=__mf_prop__(h,'string','');
  % A label whose own handle is invisible is not drawn by Octave either.
  if strcmp(__mf_prop__(h,'visible','on'),'off'),text='';return;endif
  if iscell(text)
    parts={};for k=1:numel(text),if ischar(text{k}),parts{end+1}=text{k};else parts{end+1}=num2str(text{k});endif;endfor
    text=strjoin(parts,' ');
  elseif isnumeric(text),text=num2str(text);endif
endfunction
function value=__mf_prop__(h,name,fallback)
  try value=get(h,name);catch value=fallback;end_try_catch
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
