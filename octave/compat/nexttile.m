function ax = nexttile(varargin)
% NEXTTILE Create/reuse fixed-grid axes; spans cannot overlap occupied tiles.
  state=[]; idx=[]; span=[1 1]; args=varargin; reference=[];
  if ~isempty(args) && isstruct(args{1}) && isfield(args{1},'Figure')
    reference=args{1}; f=reference.Figure; args(1)=[];
  else f=gcf(); endif
  if ~isscalar(f) || ~isfigure(f), error('nexttile: layout figure is no longer valid'); endif
  if ~isappdata(f,'__mf_tiledlayout_state__'), error('nexttile: call tiledlayout first'); endif
  state=getappdata(f,'__mf_tiledlayout_state__');
  if ~isempty(reference) && (~isfield(reference,'Generation') || ~isequal(reference.Generation,state.generation))
    error('nexttile: obsolete layout reference; use the current tiledlayout result');
  endif
  % Octave axes handles are usually negative; zero alone denotes an empty tile.
  state.axes(~isgraphics(state.axes,'axes'))=0;
  if numel(args)>1, error('nexttile: expected nexttile, nexttile(k), or nexttile([r c])'); endif
  if ~isempty(args)
    if isnumeric(args{1}) && isscalar(args{1}), idx=args{1};
    elseif isnumeric(args{1}) && numel(args{1})==2, span=args{1};
    else error('nexttile: tile index or two-element span required'); endif
  endif
  if isempty(idx), idx=__mf_tiledlayout_find__(state.rows,state.columns,state.axes~=0,span); endif
  [p,covered]=__mf_tiledlayout_span__(state.positions,state.rows,state.columns,idx,span);
  existing=state.axes(idx);
  if isgraphics(existing,'axes') && isequal(span,[1 1])
    ax=existing;
  else
    if any(isgraphics(state.axes(covered),'axes')), error('nexttile: requested span overlaps an occupied tile'); endif
    ax=axes('parent',f,'position',p,'tag','__mf_tiledlayout_axes__');
    state.axes(covered)=ax;
  endif
  setappdata(f,'__mf_tiledlayout_state__',state); axes(ax);
endfunction
