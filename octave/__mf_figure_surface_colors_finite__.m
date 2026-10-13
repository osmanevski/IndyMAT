function ok = __mf_figure_surface_colors_finite__(c, origins, edges, nrows, face_mode, edge_mode)
  % Pure colour-read check after source shape and geometry budget validation.
  % Connectivity uses zero-based column-major surface vertex indices.
  owners=[];ok=false;
  if strcmp(face_mode,'flat'),owners=[owners;origins(:)];endif
  if strcmp(face_mode,'interp')
    owners=[owners;origins(:);origins(:)+1;origins(:)+nrows;origins(:)+nrows+1];
  endif
  if strcmp(edge_mode,'flat'),owners=[owners;edges(:,1)];endif
  if strcmp(edge_mode,'interp'),owners=[owners;edges(:)];endif
  if any(~isfinite(owners)|owners<0|owners~=fix(owners)|owners>=size(c,1)*size(c,2)),return;endif
  for channel=1:size(c,3)
    plane=c(:,:,channel);
    if any(~isfinite(plane(owners+1))),return;endif
  endfor
  ok=true;
endfunction
