"use strict";
// Synthetic protocol data, never presented as a measured Octave object tree.
const fs = require("node:fs");
const { triangulatePatch } = require("../frontend/figure_patch3d_utils.cjs");
const descriptor = (shape, values) => ({ shape, values, order: "column-major" });
const matrix = rows => descriptor([rows.length, rows[0].length], rows[0].flatMap((_, c) => rows.map(row => row[c])));
const constant = rgb => ({ mode: "constant", association: "constant", rgb });
function patch(vertices, faces, id = 0) {
  const g = triangulatePatch(vertices, faces);
  if (g.code) throw new Error(g.code);
  return { id, kind: "patch3d", role: "patch", index_base: 0, triangulation: "first-valid-ear-v1", polygon_vertex_limit: 64,
    vertices: matrix(vertices), faces: matrix(faces), triangle_indices: matrix(g.triangles), triangle_faces: g.faceOfTriangle,
    edge_indices: matrix(g.edges), triangle_count: g.triangles.length, face_color: constant([1,0,0]), edge_color: constant([0,0,0]),
    line_style: "-", line_width: 1, display_name: "", decimated: false, original_points: vertices.length, rendered_points: vertices.length };
}
function figure(series) {
  const f = JSON.parse(fs.readFileSync(require.resolve("./fixtures/figure_v3/surf_vector.json")));
  const a = f.axes[0]; a.xlim = a.ylim = [0, 1]; a.zlim = [0, 2]; a.clim = [1, 2]; a.view = [0,90]; a.visible = false;
  a.colormap = matrix([[1,0,0],[0,0,1]]); a.series = series; a.decimated = false;
  Object.assign(a.camera, { position: [.5,.5,10], target: [.5,.5,1], up_vector: [0,1,0] });
  f.vertex_count = series.reduce((n,s) => n + s.rendered_points, 0);
  f.triangle_count = series.reduce((n,s) => n + s.triangle_count, 0); f.decimated = false;
  return f;
}
function contours(count = 2) {
  return figure(Array.from({ length: count }, (_, id) => {
    const margin = id / (count * 3);
    const s = patch([[margin,margin,1],[1-margin,margin,1],[1-margin,1-margin,1],[margin,1-margin,1]], [[0,1,2,3]], id);
    Object.assign(s, { role: "contourf", level: id + 1, contour_group: 0 });
    s.face_color = { mode: "flat", association: "face", encoding: "indexed", mapping: "direct", cdata_class: "double", data: descriptor([1,1],[id + 1]) };
    return s;
  }));
}
module.exports = { descriptor, matrix, constant, patch, figure, contours };
function interpolatedPatch() {
  const s = patch([[0,0,1],[1,0,1],[0,1,1]],[[0,1,2]]);
  const c = {mode:"interp",association:"vertex",encoding:"truecolor",mapping:"direct",cdata_class:"uint8",data:descriptor([3,1,3],[255,0,0,0,255,0,0,0,255])};
  s.face_color = structuredClone(c); s.edge_color = structuredClone(c);
  return figure([s]);
}
function interpolatedSurface() {
  const f = JSON.parse(fs.readFileSync(require.resolve("./fixtures/figure_v3/surf_vector.json")));
  const s = f.axes[0].series[0];
  s.face_color = {...s.face_color,mode:"interp",association:"vertex"};
  s.edge_color = structuredClone(s.face_color);
  return f;
}
module.exports.interpolatedPatch = interpolatedPatch;
module.exports.interpolatedSurface = interpolatedSurface;
