function [vertices, triangles, owners, edges, code, args, normalized_faces] = __mf_figure_contourf_geometry__(vertices, faces, level, z = [])
  % Pure native-child admission. Never reconstruct contours or remove holes.
  % Installed __contour__.m uses one polygon per patch, in overpainting order.
  triangles=zeros(0,3);owners=zeros(0,1);edges=zeros(0,3);code='';args=struct();normalized_faces=faces;
  if ~isnumeric(level)||~isreal(level)||issparse(level)||~isscalar(level)
    code='invalid_data';args=struct('property','contour.level');return;
  endif
  if isnan(level)
    % Lowest-level holes carry NaN CData and an explicit background face.
    % They cannot be given a finite contour level without inventing data.
    code='contour_filled';args=struct('property','contour.level','detail','background_hole');return;
  endif
  if ~isfinite(level)||(isa(level,'uint64')&&level>uint64(flintmax()-1))||...
      (isa(level,'int64')&&(level>int64(flintmax()-1)||level < -int64(flintmax()-1)))
    code='invalid_data';args=struct('property','contour.level');return;
  endif
  if ~isnumeric(vertices)||~isreal(vertices)||issparse(vertices)||ndims(vertices)~=2||...
      isempty(vertices)||~any(columns(vertices)==[2 3])
    code='invalid_data';args=struct('property','vertices');return;
  endif
  if rows(vertices)>40000
    code='budget_exceeded';args=struct('budget','patch_vertices','actual',rows(vertices),'limit',40000);return;
  endif
  if ~isnumeric(faces)||~isreal(faces)||issparse(faces)||ndims(faces)~=2||isempty(faces)
    code='invalid_data';args=struct('property','faces');return;
  endif
  if rows(faces)~=1
    code='contour_filled';args=struct('property','faces','detail','compound_faces');return;
  endif
  if columns(faces)>40001
    code='budget_exceeded';args=struct('budget','contour_source_indices','actual',columns(faces),'limit',40001);return;
  endif
  if any(~isfinite(vertices(:)))
    code='contour_filled';args=struct('property','vertices','detail','holes_or_nonfinite');return;
  endif
  if ~isnumeric(z)||~isreal(z)||issparse(z)||...
      (~isempty(z)&&(~isvector(z)||numel(z)~=rows(vertices)||any(~isfinite(z(:)))))
    code='invalid_data';args=struct('property','zdata');return;
  endif
  if columns(vertices)==2
    if isempty(z),z=zeros(rows(vertices),1);endif
    vertices=[double(vertices),double(z(:))];
  elseif ~isempty(z)&&~isequal(double(vertices(:,3)),double(z(:)))
    code='invalid_data';args=struct('property','zdata');return;
  endif
  % Measured native contourf boundary paths repeat consecutive corner XYZ.
  % Remove zero-length steps only from connectivity, never from source data.
  % Nonadjacent repeated positions remain ambiguous and fail triangulation.
  row=faces(1,:);padding=find(isnan(row),1);
  if ~isempty(padding)
    if any(~isnan(row(padding:end))),code='invalid_data';args=struct('property','faces');return;endif
    row=row(1:padding-1);
  endif
  if any(~isfinite(row)|row~=fix(row)|row<1|row>rows(vertices))
    code='invalid_index';args=struct('property','faces');return;
  endif
  if numel(row)<3,code='invalid_data';args=struct('property','faces');return;endif
  row=double(row);
  keep=[true,any(vertices(row(2:end),:)~=vertices(row(1:end-1),:),2).'];row=row(keep);
  if numel(row)>1&&isequal(vertices(row(1),:),vertices(row(end),:)),row(end)=[];endif
  normalized_faces=row;
  [triangles,owners,edges,code,args]=__mf_patch3d_geometry__(vertices,normalized_faces);
endfunction
