function bounds=__mf_export_bounds__(positions,insets,padding)
% Pixel-space union of plot boxes and their [left bottom right top] decorations.
  if nargin<3, padding=2; endif
  if columns(positions)~=4 || ~isequal(size(positions),size(insets)) || isempty(positions) || ...
      any(~isfinite(positions(:))) || any(~isfinite(insets(:))) || any(any(positions(:,3:4)<=0)) || any(insets(:)<0)
    error('exportgraphics: invalid or empty export geometry');
  endif
  lo=min(positions(:,1:2)-insets(:,1:2),[],1)-padding;
  hi=max(positions(:,1:2)+positions(:,3:4)+insets(:,3:4),[],1)+padding;
  bounds=[lo hi-lo];
endfunction
