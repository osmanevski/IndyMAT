function ok = __mf_figure_patch_interp_faces__(faces)
  % Called after geometry admission. Native probes do not pin a general
  % polygon tessellation; only triangles have a unique interpolation rule.
  ok = true;
  for i = 1:rows(faces)
    face = faces(i,:); face = face(~isnan(face));
    if numel(face)>3 && face(1)==face(end), face=face(1:end-1); endif
    if numel(face)~=3, ok=false; return; endif
  endfor
endfunction
