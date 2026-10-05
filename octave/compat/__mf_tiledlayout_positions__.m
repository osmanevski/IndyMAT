function positions = __mf_tiledlayout_positions__(m, n, spacing, padding)
% Pure normalized tile positions for tiledlayout (fixed grid subset).
  if ~isscalar(m) || ~isscalar(n) || any(~isfinite([m n])) || any([m n] < 1) || any(fix([m n]) ~= [m n])
    error('__mf_tiledlayout_positions__: rows and columns must be positive integers');
  endif
  gaps = struct('none', 0, 'tight', 0.012, 'compact', 0.025, 'loose', 0.07);
  if ~isfield(gaps, spacing) || ~isfield(gaps, padding)
    error('__mf_tiledlayout_positions__: spacing and padding must be compact, tight, loose, or none');
  endif
  gap = gaps.(spacing); pad = gaps.(padding);
  left = pad + 0.10; right = pad + 0.04; bottom = pad + 0.10; top = pad + 0.06;
  % Reserve at least half of each available dimension for actual tile content.
  gapX=min(gap,(1-left-right)/(2*max(1,n-1)));
  gapY=min(gap,(1-bottom-top)/(2*max(1,m-1)));
  usableW = 1-left-right-(n-1)*gapX; usableH = 1-bottom-top-(m-1)*gapY;
  positions = zeros(m*n,4); k=0;
  for row=1:m
    for col=1:n
      k=k+1; positions(k,:)=[left+(col-1)*(usableW/n+gapX), 1-top-row*(usableH/m+gapY)+gapY, usableW/n, usableH/m];
    endfor
  endfor
endfunction
