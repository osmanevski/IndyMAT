function exportgraphics(target, filename, varargin)
% EXPORTGRAPHICS Export figure/axes to PNG, JPEG, TIFF, or PDF via Octave print.
% Supports Resolution, ContentType (image/vector/auto), and solid BackgroundColor.
% Figure exports preserve the source canvas; axes are cropped around TightInset,
% paired y axes and associated decorations. Primitive snapshots include annotations,
% legends and colorbars; unsupported chart/UI object types raise an error.
  if nargin<2 || ~ischar(filename) || rows(filename)~=1 || isempty(filename), error('exportgraphics: filename character row required'); endif
  if ~isscalar(target) || ~isgraphics(target) || ~any(strcmp(get(target,'Type'),{'figure','axes'})), error('exportgraphics: target must be a figure or axes handle'); endif
  [~,~,ext]=fileparts(filename); ext=lower(ext);
  switch ext
    case '.png', device='-dpng';
    case {'.jpg','.jpeg'}, device='-djpeg';
    case {'.tif','.tiff'}, device='-dtiff';
    case '.pdf', device='-dpdf';
    otherwise, error('exportgraphics: supported formats are png, jpg, tif, and pdf');
  endswitch
  dpi=150; content='auto'; bgcolor=[];
  if mod(numel(varargin),2)~=0, error('exportgraphics: options must be name/value pairs'); endif
  for k=1:2:numel(varargin)
    key=varargin{k}; val=varargin{k+1};
    if ~ischar(key), error('exportgraphics: option names must be character rows'); endif
    switch lower(key)
      case 'resolution'
        if ~isnumeric(val)||~isscalar(val)||~isfinite(val)||val<1||fix(val)~=val, error('exportgraphics: Resolution must be a positive integer'); endif
        dpi=val;
      case 'contenttype'
        if ~ischar(val)||~any(strcmpi(val,{'auto','image','vector'})), error('exportgraphics: ContentType must be auto, image, or vector'); endif
        content=lower(val);
      case 'backgroundcolor'
        if ischar(val) && strcmpi(val,'none'), error('exportgraphics: transparent BackgroundColor is unsupported'); endif
        bgcolor=__mf_graphics_parse_colors__(val);
        if rows(bgcolor)~=1, error('exportgraphics: BackgroundColor must be one colour'); endif
      otherwise, error('exportgraphics: unsupported option %s',key);
    endswitch
  endfor
  if strcmp(content,'vector') && ~strcmp(ext,'.pdf'), error('exportgraphics: vector ContentType is supported only for PDF'); endif
  if strcmp(content,'image') && strcmp(ext,'.pdf'), error('exportgraphics: image ContentType for PDF is unsupported'); endif
  f=__mf_export_snapshot__(target,bgcolor);
  unwind_protect
    resolution=sprintf('-r%d',dpi);
    print(f,filename,device,resolution);
  unwind_protect_cleanup
    if isfigure(f), delete(f); endif
  end_unwind_protect
endfunction
