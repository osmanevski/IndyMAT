function h = sgtitle(varargin)
% SGTITLE Figure-wide title, with room reserved in automatic subplot grids.
% Returns an Octave annotation handle, not MATLAB's SubplotText object.
% Explicit axes positions and explicit annotation positions are preserved.
  if isempty(varargin),error('sgtitle: title text required');endif
  if isnumeric(varargin{1}) && isscalar(varargin{1}) && isgraphics(varargin{1},'figure')
    f=varargin{1};varargin(1)=[];
  else,f=gcf();endif
  if isempty(varargin),error('sgtitle: title text required');endif
  txt=varargin{1};varargin(1)=[];
  if mod(numel(varargin),2),error('sgtitle: options must be name/value pairs');endif
  explicit_position=false;
  for k=1:2:numel(varargin)
    if ~ischar(varargin{k}),error('sgtitle: property names must be character rows');endif
    explicit_position=explicit_position||any(strcmpi(varargin{k},{'position','units'}));
  endfor
  % FitBoxToText=on anchors the fitted width at x=0 in Octave, even for
  % center alignment. A fixed full-width box centers both PNG and JSON text.
  h=annotation(f,'textbox',[0 .94 1 .04],'string',txt,'edgecolor','none',...
    'fitboxtotext','off','horizontalalignment','center','verticalalignment','top',...
    'fontsize',15,'fontweight','normal','tag','__mf_sgtitle__',varargin{:});
  if ~explicit_position
    th=findall(h,'type','text');extent=get(th,'extent');
    fp=__get_position__(f,'pixels');gap=6/max(1,fp(4));
    lines=get(th,'string');line_count=1;
    if ischar(lines)
      line_count=rows(lines)+sum(lines(:)==char(10));
    elseif iscell(lines)
      line_count=sum(cellfun(@(line) rows(line)+sum(line(:)==char(10)),lines));
    endif
    % Glyph descenders must not change the reserved height when text changes.
    height=max(extent(4),get(h,'fontsize')*1.2*max(1,line_count)/max(1,fp(4)));
    setappdata(h,'__mf_sgtitle_auto__',true);
    % Anchor the top, rather than adding fitted height above a fixed bottom.
    set(h,'position',[0 max(0,1-gap-height) 1 height]);
    setappdata(h,'__mf_sgtitle_auto_geometry__',struct('position',get(h,'position'),'units',get(h,'units')));
    local_layout(f,min(.5,height+2*gap));
  endif
  % Validate/create the replacement before deleting the previous title.
  old=findall(f,'tag','__mf_sgtitle__');delete(old(old~=h));
endfunction

function local_layout(f,reserve)
  tiles=[];
  if isappdata(f,'__mf_tiledlayout_state__')
    tiles=getappdata(f,'__mf_tiledlayout_state__');
    if ~isfield(tiles,'sgtitle_base_positions')
      tiles.sgtitle_base_positions=tiles.positions;
    endif
  endif
  for ax=findall(f,'type','axes')(:)'
    if ~strcmp(get(ax,'units'),'normalized'),continue;endif
    pos=get(ax,'position');state=[];
    if isappdata(ax,'__mf_sgtitle_layout__')
      state=getappdata(ax,'__mf_sgtitle_layout__');
      if isfield(state,'manual') && state.manual,continue;endif
      if ~isequal(pos,state.last)
        % A user edit ends automatic ownership of this axes.
        state.manual=true;setappdata(ax,'__mf_sgtitle_layout__',state);continue;
      endif
    elseif isappdata(ax,'__subplotposition__')
      state=struct('base',pos,'last',pos,'subplot',getappdata(ax,'__subplotposition__'),...
                   'outer',getappdata(ax,'__subplotouterposition__'));
    elseif strcmp(get(ax,'tag'),'__mf_tiledlayout_axes__') && isappdata(f,'__mf_tiledlayout_state__')
      if ~any(all(abs(tiles.positions-pos)<eps,2)),continue;endif
      base=pos;
      if isfield(tiles,'sgtitle_reserve'),base([2 4])/=1-tiles.sgtitle_reserve;endif
      state=struct('base',base,'last',pos,'subplot',[],'outer',[]);
    endif
    if isempty(state),continue;endif
    target=state.base;target([2 4])*=1-reserve;
    set(ax,'position',target);state.last=get(ax,'position');
    if ~isempty(state.subplot)
      base=state.subplot;base([2 4])*=1-reserve;
      outer=state.outer;outer([2 4])*=1-reserve;
      setappdata(ax,'__subplotposition__',base);
      setappdata(ax,'__subplotouterposition__',outer);
    endif
    setappdata(ax,'__mf_sgtitle_layout__',state);
  endfor
  if ~isempty(tiles)
    tiles.positions=tiles.sgtitle_base_positions;
    tiles.positions(:,[2 4])*=1-reserve;
    tiles.sgtitle_reserve=reserve;
    setappdata(f,'__mf_tiledlayout_state__',tiles);
  endif
endfunction
