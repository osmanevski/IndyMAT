function [color, code] = __mf_figure_patch_interp_color__(raw, mapping, nvertices)
  % Pure descriptor construction for one CData row per source patch vertex.
  % Explicit interp disambiguates vertex association when face counts match.
  color=struct('mode','none','association','constant');code='';
  if ~isnumeric(nvertices)||~isreal(nvertices)||~isscalar(nvertices)||...
      ~isfinite(nvertices)||nvertices<1||nvertices~=fix(nvertices)||nvertices>40000
    code='invalid_data';return;
  endif
  if ~isnumeric(raw)||~isreal(raw)||issparse(raw)||ndims(raw)~=2||...
      rows(raw)~=nvertices||~any(columns(raw)==[1 3])
    code='patch_colors';return;
  endif
  if ~ischar(mapping)||~any(strcmp(mapping,{'scaled','direct'}))||any(~isfinite(raw(:)))
    code='unsupported_color';return;
  endif
  if (isa(raw,'uint64')&&any(raw(:)>uint64(flintmax()-1)))||...
      (isa(raw,'int64')&&any(raw(:)>int64(flintmax()-1)|raw(:)<-int64(flintmax()-1)))
    code='unsupported_color';return;
  endif
  encoding='indexed';data=raw;
  if columns(raw)==3
    encoding='truecolor';
    if ~any(strcmp(class(raw),{'double','single','uint8','uint16'}))
      code='unsupported_color';return;
    endif
    maximum=1;if isa(raw,'uint8'),maximum=255;elseif isa(raw,'uint16'),maximum=65535;endif
    if any(raw(:)<0|raw(:)>maximum),code='unsupported_color';return;endif
    data=reshape(raw,[nvertices,1,3]);
  endif
  descriptor=struct('shape',{num2cell(size(data))},'order','column-major',...
    'values',{num2cell(double(data(:).'))});
  color=struct('mode','interp','association','vertex','encoding',encoding,...
    'mapping',mapping,'cdata_class',class(raw),'data',descriptor);
endfunction
