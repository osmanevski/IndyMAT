function [position, covered] = __mf_tiledlayout_span__(positions, m, n, idx, span)
% Pure tile-span validation and bounding-box calculation for fixed grids.
  if ~isscalar(idx) || ~isfinite(idx) || fix(idx) ~= idx || idx < 1 || idx > m*n
    error('__mf_tiledlayout_span__: tile index out of range');
  endif
  if numel(span) ~= 2 || any(~isfinite(span)) || any(fix(span) ~= span) || any(span < 1)
    error('__mf_tiledlayout_span__: span must contain two positive integers');
  endif
  row=floor((idx-1)/n)+1; col=mod(idx-1,n)+1;
  if row+span(1)-1>m || col+span(2)-1>n, error('__mf_tiledlayout_span__: span exceeds layout'); endif
  covered=[];
  for rr=row:row+span(1)-1, covered=[covered ((rr-1)*n+col:(rr-1)*n+col+span(2)-1)]; endfor
  first=positions(idx,:); last=positions((row+span(1)-2)*n+col+span(2)-1,:);
  position=[first(1),last(2),last(1)+last(3)-first(1),first(2)+first(4)-last(2)];
endfunction
