function figures=__mf_publish_snapshot__()
% Render-property subset for Octave 11.3 print output. Capture concerns plot
% contents, not window selection, Tag, UserData, callbacks or other bookkeeping.
% These native primitives and groups are supported; embedded GUI controls and
% unknown graphics types fail. Dynamic group/legend properties are represented
% by the native descendants they update. Arrays/cells/plain structs only.
  handles=findall(0,'type','figure');
  figures=cell(numel(handles),1);
  chrome={'uimenu','uicontextmenu','uitoolbar','uipushtool','uitoggletool'};
  for f=1:numel(handles)
    objects=sort(findall(handles(f)));
    records={};
    for k=1:numel(objects)
      object=objects(k);
      kind=get(object,'type');
      if any(strcmp(kind,chrome)),continue;endif
      names=visual_properties(kind);
      properties=get(object);
      selected=struct();
      for j=1:numel(names)
        name=names{j};
        if ~isfield(properties,name),continue;endif
        value=properties.(name);
        if strcmp(kind,'figure')&&strcmp(name,'position'),value=value(3:4);endif
        validate_value(value);
        selected.(name)=value;
      endfor
      children=allchild(object);
      keep=true(size(children));
      for j=1:numel(children)
        keep(j)=~any(strcmp(get(children(j),'type'),chrome));
      endfor
      records{end+1}=struct('handle',object,'type',kind,'children',children(keep),'properties',selected);
    endfor
    figures{f}=struct('handle',handles(f),'objects',{records});
  endfor
endfunction

function names=visual_properties(kind)
  common='clipping visible';
  switch kind
    case 'figure'
      % Figure visibility and screen x/y do not change its printed content.
      common='';
      specific='alphamap color colormap graphicssmoothing inverthardcopy paperorientation paperposition papersize paperunits position renderer units';
    case 'axes'
      specific=['alim alphamap alphascale ambientlightcolor box boxstyle cameraposition cameratarget cameraupvector cameraviewangle clim clippingstyle color colormap colorscale dataaspectratio ' ...
        'fontangle fontname fontsize fontsmoothing fontunits fontweight gridalpha gridcolor gridlinestyle innerposition labelfontsizemultiplier layer linewidth minorgridalpha minorgridcolor minorgridlinestyle outerposition plotboxaspectratio position positionconstraint projection sortmethod tickdir ticklabelinterpreter ticklength titlefontsizemultiplier titlefontweight units view ' ...
        'xaxislocation xcolor xdir xgrid xlim xminorgrid xminortick xminortickvalues xscale xtick xticklabel xticklabelrotation ' ...
        'yaxislocation ycolor ydir ygrid ylim yminorgrid yminortick yminortickvalues yscale ytick yticklabel yticklabelrotation ' ...
        'zcolor zdir zgrid zlim zminorgrid zminortick zminortickvalues zscale ztick zticklabel zticklabelrotation'];
    case 'line'
      specific='color linejoin linestyle linewidth marker markeredgecolor markerfacecolor markersize xdata ydata zdata';
    case 'text'
      specific='backgroundcolor color edgecolor fontangle fontname fontsize fontsmoothing fontunits fontweight horizontalalignment interpreter linestyle linewidth margin position rotation string units verticalalignment';
    case 'image'
      specific='alphadata alphadatamapping cdata cdatamapping xdata ydata';
    case {'patch','surface'}
      specific=['alphadata alphadatamapping ambientstrength backfacelighting cdata cdatamapping diffusestrength edgealpha edgecolor edgelighting facealpha facecolor facelighting facenormals faces facevertexalphadata facevertexcdata ' ...
        'linestyle linewidth marker markeredgecolor markerfacecolor markersize meshstyle specularcolorreflectance specularexponent specularstrength vertexnormals vertices xdata ydata zdata'];
    case 'scatter'
      specific='cdata linewidth marker markeredgealpha markeredgecolor markerfacealpha markerfacecolor sizedata xdata ydata zdata';
    case 'light'
      specific='color position style';
    case 'hggroup'
      specific='';
    otherwise
      error(__mf_text__('Publishing figure capture does not support this object type: %s.', 'Yayın grafik yakalama bu nesne türünü desteklemiyor: %s.'),kind);
  endswitch
  names=strsplit(strtrim([common ' ' specific]));
endfunction

function validate_value(value)
  if isnumeric(value)||islogical(value)||ischar(value)
    if issparse(value),error(__mf_text__('Sparse arrays are not supported in published figure properties.', 'Yayın grafik özelliklerinde seyrek diziler desteklenmiyor.'));endif
  elseif iscell(value)
    for k=1:numel(value),validate_value(value{k});endfor
  elseif isstruct(value)
    names=fieldnames(value);
    for k=1:numel(value)
      for j=1:numel(names),validate_value(value(k).(names{j}));endfor
    endfor
  else
    error(__mf_text__('The %s class is not supported in published figure properties.', 'Yayın grafik özelliklerinde %s sınıfı desteklenmiyor.'),class(value));
  endif
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
