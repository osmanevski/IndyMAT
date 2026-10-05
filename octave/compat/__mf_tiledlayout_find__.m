function idx=__mf_tiledlayout_find__(m,n,occupied,span)
% First free row-major rectangular region, independent of any previous cursor.
  if ~isnumeric(span) || ~isreal(span) || numel(span)~=2 || any(~isfinite(span(:))) || any(span(:)<1) || any(fix(span(:))~=span(:))
    error('nexttile: span must contain two positive integers');
  endif
  for row=1:m-span(1)+1
    for col=1:n-span(2)+1
      cells=[];
      for rr=row:row+span(1)-1, cells=[cells (rr-1)*n+(col:col+span(2)-1)]; endfor
      if ~any(occupied(cells)), idx=(row-1)*n+col; return; endif
    endfor
  endfor
  error('nexttile: no empty region fits the requested span');
endfunction
