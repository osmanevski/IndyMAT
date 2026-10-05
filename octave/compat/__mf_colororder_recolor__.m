function [colors,recolor]=__mf_colororder_recolor__(oldorder,neworder,linecolors)
% Pure comparison rule; rows of linecolors are in creation order (oldest first).
% Recolour the whole list only when it exactly matches the old cyclic sequence.
% An explicitly assigned RGB identical to that sequence is indistinguishable.
  oldorder=__mf_graphics_parse_colors__(oldorder);
  neworder=__mf_graphics_parse_colors__(neworder);
  if isempty(linecolors), colors=zeros(0,3); recolor=true; return; endif
  if iscell(linecolors)
    original=linecolors; rgb=zeros(numel(linecolors),3);
    for k=1:numel(linecolors)
      value=linecolors{k};
      if ~isnumeric(value) || ~isreal(value) || numel(value)~=3 || any(~isfinite(value(:))) || any(value(:)<0) || any(value(:)>1)
        colors=original; recolor=false; return; % e.g. a line with Color='none'
      endif
      rgb(k,:)=value(:)';
    endfor
    linecolors=rgb;
  endif
  linecolors=__mf_graphics_parse_colors__(linecolors);
  sequence=(0:rows(linecolors)-1)';
  recolor=isequal(linecolors,oldorder(mod(sequence,rows(oldorder))+1,:));
  if recolor, colors=neworder(mod(sequence,rows(neworder))+1,:);
  else colors=linecolors; endif
endfunction
