"use strict";
const assert=require("node:assert/strict");
const {descriptor,matrix,constant,figure}=require("./figure_dalga26_support.cjs");
const {validate}=require("../frontend/figure_data_utils.cjs");
const {patchPoint}=require("../frontend/figure_patch3d_utils.cjs");
const G=require("../frontend/figure_geometry_utils.cjs");
const C=require("../frontend/figure_camera_utils.cjs");
// Only vertices, faces, zdata, level and face/edge colors below are measured.
// The containing schema/camera, scaled color mapping, stroke width/style and
// IDs are explicit synthetic harness settings; the capture lacks those fields.
function native(p, id = 0) {
  const vertices = p.vertices.map((v, i) => v.length === 3 ? v.slice() : [...v, p.zdata.length ? p.zdata[i] : 0]);
  const source = p.faces.map(i => i === null ? null : i - 1);
  const same = (a, b) => vertices[a].every((v, k) => v === vertices[b][k]);
  const faces = []; let padding = false;
  for (const i of source) {
    if (i === null) { padding = true; continue; }
    assert(!padding, "recorded face padding must be trailing");
    assert(Number.isInteger(i) && i >= 0 && i < vertices.length);
    if (!faces.length || !same(faces.at(-1), i)) faces.push(i);
  }
  if (faces.length > 1 && same(faces[0], faces.at(-1))) faces.pop();
  const distinct = new Set(faces.map(i => JSON.stringify(vertices[i])));
  const no_area = distinct.size <= 2;
  const level = p.facevertexcdata;
  const color = value => Array.isArray(value) ? constant(value.slice()) : value === "none" ? {mode:"none",association:"constant"} :
    {mode:value,association:"face",encoding:"indexed",mapping:"scaled",cdata_class:"double",data:descriptor([1,1],[level])};
  return { id, kind:"patch3d", role:"contourf", triangulation:"native-ring-v1", index_base:0,
    vertices:matrix(vertices), faces:descriptor([1,faces.length],faces), source_faces:descriptor([1,source.length],source),
    no_area, background_hole:level === null, level, contour_group:0,
    triangle_count:no_area ? 0 : faces.length-2, face_color:color(p.facecolor), edge_color:color(p.edgecolor),
    line_width:.5, line_style:"-", display_name:"", decimated:false, original_points:vertices.length, rendered_points:vertices.length };
}
function nativeFigure(series) {
  const f = figure(series), a = f.axes[0];
  const all = series.flatMap(s => Array.from({length:s.vertices.shape[0]}, (_, i) => patchPoint(s,i)));
  const ranges = [0,1,2].map(k => { let low=Infinity,high=-Infinity; for(const p of all){low=Math.min(low,p[k]);high=Math.max(high,p[k]);} return low===high ? [low-1,high+1] : [low,high]; });
  [a.xlim,a.ylim,a.zlim] = ranges; a.clim=[-8,8]; a.view=[0,90];
  const centre = ranges.map(([lo,hi]) => (lo+hi)/2);
  Object.assign(a.camera,{target:centre,position:[centre[0],centre[1],centre[2]+100],up_vector:[0,1,0]});
  return f;
}
function prepared(series) {
  const f=nativeFigure(series), a=f.axes[0];
  const result=validate(f);assert(result.ok,JSON.stringify(result));
  return {f,a,scene:G.buildScene(a),camera:C.createCamera(a)};
}
module.exports={native,nativeFigure,prepared};
