function colors = colororder(varargin)
% COLORORDER Get/set axes or figure colour order; named palettes are unsupported.
% Implicit calls use the figure (getter falls back to root defaults without creating one).
% Setters always apply a valid palette. Existing lines are recoloured only when
% every line matches the old order's cyclic sequence in oldest-first child order.
% Otherwise all existing line colours are preserved with an explicit warning.
% Equal RGB values cannot prove whether a colour was assigned explicitly.
% An axes palette resets on a later plot without hold (Octave newplot semantics);
% the figure-level default persists. Named palettes are unsupported.
  target = []; setting = false;
  if nargin == 0
    target = get(0,'currentfigure');
    if isempty(target), colors=get(0,'defaultaxescolororder'); return; endif
  elseif nargin == 1
    candidate = varargin{1};
    if isscalar(candidate) && (isgraphics(candidate,'axes') || isfigure(candidate))
      target = candidate;
    else
      value = candidate; setting = true;
    endif
  elseif nargin == 2
    target = varargin{1}; value = varargin{2}; setting = true;
  else
    error('colororder: expected colororder, colororder(colors), or colororder(target, colors)');
  endif
  if setting, parsed = __mf_graphics_parse_colors__(value); endif
  if isempty(target) && nargin == 1, target = gcf(); endif
  if ~isscalar(target) || ~(isgraphics(target,'axes') || isfigure(target))
    error('colororder: target must be an axes or figure handle');
  endif
  if ~setting
    if isfigure(target), colors = get(target, 'defaultaxescolororder');
    else colors = get(target, 'ColorOrder'); endif
    return;
  endif
  if isfigure(target), children=findall(target,'Type','axes'); else children=target; endif
  plans=cell(numel(children),1); skipped=false;
  for k=1:numel(children)
    ax=children(k); lines=[];
    % Legend/colorbar/annotation strokes are decorations, not plotted series.
    if ~any(strcmp(get(ax,'tag'),{'legend','colorbar','scribeoverlay'}))
      lines=flipud(findall(ax,'Type','line')(:));
    endif
    oldcolors=cell(numel(lines),1);
    for j=1:numel(lines), oldcolors{j}=get(lines(j),'color'); endfor
    [mapped,recolor]=__mf_colororder_recolor__(get(ax,'colororder'),parsed,oldcolors);
    plans{k}=struct('lines',lines,'colors',{mapped},'recolor',recolor);
    skipped=skipped || ~recolor;
  endfor
  if isfigure(target), set(target,'defaultaxescolororder',parsed); endif
  for k=1:numel(children)
    ax=children(k); plan=plans{k}; set(ax,'ColorOrder',parsed);
    if plan.recolor
      for j=1:numel(plan.lines), set(plan.lines(j),'Color',plan.colors(j,:)); endfor
      set(ax,'ColorOrderIndex',mod(numel(plan.lines),rows(parsed))+1);
    endif
  endfor
  if skipped
    warning('IndyMAT:colororder:existingLines',...
      'colororder: existing lines were not recoloured because explicit colours cannot be distinguished from automatic colours; the new ColorOrder has been applied');
  endif
  if nargout > 0, colors = parsed; endif
endfunction
