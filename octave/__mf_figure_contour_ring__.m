function [vertices, face, no_area, code, args] = __mf_figure_contour_ring__(vertices, faces, level, z = [])
  % Native contour rings only. O(N) serialization; the browser validates and
  % triangulates the planar ring. No contour reconstruction or hole inference.
  face=[];no_area=false;code='';args=struct();
  if ~isnumeric(level)||~isreal(level)||issparse(level)||~isscalar(level)||isinf(level)||...
      (isa(level,'uint64')&&level>uint64(flintmax()-1))||...
      (isa(level,'int64')&&(level>int64(flintmax()-1)||level < -int64(flintmax()-1)))
    code='invalid_data';args=struct('property','contour.level');return;
  endif
  if ~isnumeric(vertices)||~isreal(vertices)||issparse(vertices)||ndims(vertices)~=2||...
      isempty(vertices)||~any(columns(vertices)==[2 3])||any(~isfinite(vertices(:)))
    code='contour_filled';args=struct('property','vertices','detail','holes_or_nonfinite');return;
  endif
  if rows(vertices)>100000
    code='budget_exceeded';args=struct('budget','vertices','actual',rows(vertices),'limit',100000);return;
  endif
  if ~isnumeric(faces)||~isreal(faces)||issparse(faces)||ndims(faces)~=2||rows(faces)~=1||isempty(faces)
    code='contour_filled';args=struct('property','faces','detail','compound_faces');return;
  endif
  if numel(faces)>100001
    code='budget_exceeded';args=struct('budget','contour_source_indices','actual',numel(faces),'limit',100001);return;
  endif
  if ~isnumeric(z)||~isreal(z)||issparse(z)||(~isempty(z)&&...
      (~isvector(z)||numel(z)~=rows(vertices)||any(~isfinite(z(:)))))
    code='invalid_data';args=struct('property','zdata');return;
  endif
  if columns(vertices)==2
    if isempty(z),z=zeros(rows(vertices),1);endif
    vertices=[double(vertices),double(z(:))];
  elseif ~isempty(z)&&~isequal(double(vertices(:,3)),double(z(:)))
    code='invalid_data';args=struct('property','zdata');return;
  endif
  face=faces;pad=find(isnan(face),1);
  if ~isempty(pad)
    if any(~isnan(face(pad:end))),code='invalid_data';args=struct('property','faces');return;endif
    face=face(1:pad-1);
  endif
  if isempty(face)||any(~isfinite(face)|face~=fix(face)|face<1|face>rows(vertices))
    code='invalid_index';args=struct('property','faces');return;
  endif
  face=double(face);face=face([true,any(diff(vertices(face,:),1,1)~=0,2).']);
  if numel(face)>1&&isequal(vertices(face(1),:),vertices(face(end),:)),face(end)=[];endif
  points=vertices(face,:);
  distinct=any(points~=points(1,:),2);second=find(distinct,1);
  no_area=isempty(second)||all(~distinct|all(points==points(second,:),2));
endfunction
