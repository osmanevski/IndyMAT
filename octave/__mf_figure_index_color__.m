function [rgb,ok] = __mf_figure_index_color__(value,mapping,map,clim)
  % Same indexed colour rule as figure_color_utils.descriptorColor. Pure data.
  rgb='none';ok=false;
  if ~isnumeric(value)||~isreal(value)||~isscalar(value)||~isfinite(value)||...
      ~isnumeric(map)||~isreal(map)||issparse(map)||columns(map)~=3||isempty(map)||...
      any(~isfinite(map(:)))||any(map(:)<0|map(:)>1)||...
      ~isnumeric(clim)||numel(clim)~=2||any(~isfinite(clim))||clim(2)<=clim(1)
    return;
  endif
  integer=isinteger(value);v=double(value);n=rows(map);
  if strcmp(mapping,'direct')
    index=v;if ~integer,index=fix(v)-1;endif
  elseif strcmp(mapping,'scaled')
    if v<=clim(1),index=0;
    elseif v>=clim(2),index=n-1;
    else
      span=clim(2)-clim(1);
      if isfinite(span),fraction=(v-clim(1))/span;
      else fraction=(v/2-clim(1)/2)/(clim(2)/2-clim(1)/2);endif
      index=floor(n*fraction);
    endif
  else return;
  endif
  index=min(n-1,max(0,index));rgb=double(map(index+1,:));ok=true;
endfunction
