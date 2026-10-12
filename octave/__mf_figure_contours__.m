function [x,y,z,closed,code,args] = __mf_figure_contours__(x,y,z,faces,limit=2000)
  % Resolved patch path only. No contour algorithm or graphics property writes.
  % Octave __contour__ closes finite polygons implicitly, while an open path
  % has a terminal NaN vertex. Preserve that terminal break; explicitly append
  % the first vertex of a finite patch to reproduce its source closing edge.
  closed=false;code='';args=struct();n=numel(x);
  if n+1>limit,code='budget_exceeded';args=struct('budget','samples','actual',n+1,'limit',limit);return;endif
  if ~isnumeric(x)||~isreal(x)||issparse(x)||~isvector(x)||...
      ~isnumeric(y)||~isreal(y)||issparse(y)||~isvector(y)||numel(y)~=n||...
      (~isempty(z)&&(~isnumeric(z)||~isreal(z)||issparse(z)||~isvector(z)||numel(z)~=n))||...
      ~isnumeric(faces)||~isreal(faces)||issparse(faces)||rows(faces)~=1||~isequal(faces,1:n)
    code='invalid_data';args=struct('property','contour.path');return;
  endif
  x=x(:).';y=y(:).';z=z(:).';
  finite=isfinite(x)&isfinite(y);if ~isempty(z),finite=finite&isfinite(z);endif
  closed=n>=2&&all(finite);
  if closed
    x=[x,x(1)];y=[y,y(1)];if ~isempty(z),z=[z,z(1)];endif
  endif
endfunction
