function map = parula(n)
% PARULA Approximation of MATLAB R2025b parula using interpolated control points.
% These control points approximate MATLAB's colours; they are not MATLAB's table.
% Octave's current-figure colormap length is the default.
  if nargin == 0, n = rows(colormap()); endif
  if ~isnumeric(n) || ~isreal(n) || ~isscalar(n) || ~isfinite(n) || n < 0 || fix(n) ~= n
    error('parula: length must be a nonnegative integer');
  endif
  if n == 0, map=zeros(0,3); return; endif
  anchors = [0.2081 0.1663 0.5292; 0.2116 0.2559 0.6673; 0.2123 0.3598 0.7417; 0.1928 0.4670 0.7685; 0.1473 0.5650 0.7532; 0.1020 0.6498 0.7140; 0.1400 0.7350 0.6200; 0.3320 0.8020 0.4590; 0.6080 0.8090 0.2530; 0.8760 0.8410 0.1130; 0.9763 0.9831 0.0538];
  q = linspace(0,1,n); x=linspace(0,1,rows(anchors)); map=zeros(n,3);
  for c=1:3, map(:,c)=interp1(x,anchors(:,c),q,'linear')'; endfor
endfunction
