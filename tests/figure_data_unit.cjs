"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { validate, decodeArray, LIMITS, REASON_CODES } = require("../frontend/figure_data_utils.cjs");
let count = 0;
function check(name, fn) {
  fn();
  count++;
}
const copy = (v) => JSON.parse(JSON.stringify(v));
const descriptor = (shape, values) => ({ shape, order: "column-major", values });
// Synthetic in-memory validator inputs, NOT claimed as recorded Octave fixtures.
const line = { id: 0, kind: "line", x: [1, 2, null, 4], y: [2, 3, null, 5], z: [3, 4, null, 6],
  line_color: [.2, .4, .6], marker_edge_color: [.2, .4, .6], marker_face_color: "none", marker_face_auto: false,
  line_style: "-", line_width: .5, marker: "o", marker_size: 6, display_name: "line", base_value: 0,
  decimated: false, original_points: 4, rendered_points: 4, source_indices: [0, 1, 2, 3] };
const axis = { id: 0, dimension: 3, supported: true, reason: "", position: [.13, .11, .775, .815],
  xlim: [0, 5], ylim: [0, 6], zlim: [0, 7], xscale: "linear", yscale: "linear", zscale: "linear",
  xdir: "normal", ydir: "normal", zdir: "normal", xlabel: "x", ylabel: "y", zlabel: "z", title: "",
  xtick: [], ytick: [], ztick: [], xticklabel: [], yticklabel: [], zticklabel: [], grid: { x: false, y: false, z: false },
  legend: { visible: false, location: "", labels: [] }, decimated: false, series: [line], colorbars: [],
  view: [-37.5, 30], camera: { projection: "orthographic", position: [10, 20, 30], target: [2, 3, 4], up_vector: [0, 0, 1], view_angle: 10,
    position_mode: "auto", target_mode: "auto", upvector_mode: "auto", viewangle_mode: "auto" },
  data_aspect_ratio: [1, 1, 1], data_aspect_ratio_mode: "auto", plot_box_aspect_ratio: [1, 1, 1], plot_box_aspect_ratio_mode: "auto",
  clim: [0, 1], colormap: descriptor([2, 3], [0, 1, 0, 0, 0, 0]) };
const scene = { version: 3, supported: true, decimated: false, point_limit: 2000, source: { job: "f".repeat(32), figure: 1 },
  limits: { ...LIMITS }, reason: "", reason_code: "", reason_args: {}, axes: [axis], vertex_count: 4, triangle_count: 0 };
function bad(name, mutate, code) {
  check(name, () => {
    const input = copy(scene);
    mutate(input);
    const result = validate(input);
    assert.equal(result.ok, false, name);
    if (code) assert.equal(result.reason_code, code, JSON.stringify(result));
  });
}
check("accept v3 input without mutation", () => {
  const before = copy(scene);
  assert.equal(validate(scene).ok, true, JSON.stringify(validate(scene)));
  assert.deepEqual(scene, before);
});
check("legacy version 2", () => {
  const v = copy(scene);
  v.version = 2;
  delete v.axes[0].series[0].z;
  assert.equal(validate(v).ok, true);
});
check("legacy singleton normalisation", () => {
  const v = copy(scene);
  v.version = 2;
  const s = v.axes[0].series[0];
  delete s.z;
  delete s.source_indices;
  s.x = 1;
  s.y = null;
  s.original_points = 1;
  assert.deepEqual(validate(v).data.axes[0].series[0].x, [1]);
  assert.deepEqual(validate(v).data.axes[0].series[0].y, [null]);
  assert.equal(s.x, 1);
});
for (const code of REASON_CODES) check("fallback " + code, () => {
  const v = copy(scene);
  v.supported = false;
  v.axes = [];
  v.reason_code = code;
  assert.deepEqual([validate(v).ok, validate(v).supported], [true, false]);
});
bad("unknown version", (v) => { v.version = 4; }, "unknown_version");
bad("wrong vector shape", (v) => { v.axes[0].series[0].y.pop(); });
bad("bad source index", (v) => { v.axes[0].series[0].source_indices[3] = 4; }, "invalid_index");
bad("unordered source index", (v) => { v.axes[0].series[0].source_indices[2] = 1; }, "invalid_index");
bad("negative source index", (v) => { v.axes[0].series[0].source_indices[0] = -1; }, "invalid_index");
bad("wrong source counts", (v) => { v.axes[0].series[0].rendered_points = 5; });
bad("wrong aggregate counts", (v) => { v.vertex_count = 99; });
bad("over samples", (v) => { v.axes[0].series[0].x = Array(2001).fill(1); }, "budget_exceeded");
bad("over axes", (v) => { v.axes = Array(17).fill(v.axes[0]); }, "budget_exceeded");
bad("over series", (v) => { v.axes[0].series = Array(129).fill(v.axes[0].series[0]); }, "budget_exceeded");
bad("nonfinite JSON numeric", (v) => { v.axes[0].series[0].z[0] = Infinity; });
bad("malicious limit increase", (v) => { v.limits.surface_vertices = 1e9; });
bad("log axes", (v) => { v.axes[0].zscale = "log"; }, "log_3d");
bad("perspective", (v) => { v.axes[0].camera.projection = "perspective"; }, "perspective");
bad("manual camera", (v) => { v.axes[0].camera.target_mode = "manual"; }, "manual_camera");
bad("singular camera", (v) => { v.axes[0].camera.position = v.axes[0].camera.target; });
bad("parallel camera up", (v) => { v.axes[0].camera.up_vector = [8, 17, 26]; });
bad("wrong aspect", (v) => { v.axes[0].data_aspect_ratio[0] = 0; });
bad("invalid marker", (v) => { v.axes[0].series[0].marker = "d"; }, "unsupported_marker");
bad("invalid color", (v) => { v.axes[0].series[0].line_color = [2, 0, 0]; }, "unsupported_color");
bad("UTF-8 byte budget", (v) => { v.reason = "ğ".repeat(4194305); }, "json_budget");
bad("escaped byte budget", (v) => { v.reason = "\u0000".repeat(1398102); }, "json_budget");
bad("cyclic input", (v) => { v.axes[0].cycle = v; });
check("descriptor empty, singleton, null, order", () => {
  assert.deepEqual(decodeArray(descriptor([0, 2], [])).values, []);
  assert.deepEqual(decodeArray(descriptor([1, 1], [null])).values, [null]);
  assert.deepEqual(decodeArray(descriptor([2, 2], [1, 2, 3, 4])).values, [1, 2, 3, 4]);
  assert.equal(decodeArray(descriptor([2, 3], [1])).ok, false);
  assert.equal(decodeArray({ shape: [2, 2], order: "row-major", values: [1, 2, 3, 4] }).ok, false);
  assert.equal(decodeArray(descriptor([40000, 40000], [])).ok, false);
});
const surface = { cdata_class: "uint8", cdata_mapping: "scaled", cdata: descriptor([2, 3], [0, 1, 2, 3, 4, 5]), id: 0, kind: "surface", shape: [2, 3], coordinate_layout: { x: "vector", y: "vector" },
  x: descriptor([1, 3], [10, 20, 30]), y: descriptor([2, 1], [40, 50]), z: descriptor([2, 3], [1, 2, 3, 4, 5, 6]),
  face_color: { mode: "flat", association: "cell", encoding: "indexed", mapping: "scaled", cdata_class: "uint8", data: descriptor([2, 3], [0, 1, 2, 3, 4, 5]) },
  edge_color: { mode: "constant", association: "constant", rgb: [0, 0, 0] }, cell_color_owner: "row-column-origin", edge_color_owner: "first-vertex",
  mesh_style: "both", line_style: "-", line_width: .5, display_name: "surface", decimated: false, original_points: 6, rendered_points: 6,
  cell_origins: [0, 2], edge_indices: descriptor([7, 2], [0, 2, 4, 0, 1, 2, 3, 1, 3, 5, 2, 3, 4, 5]), index_base: 0, triangle_count: 4 };
const surfScene = copy(scene);
surfScene.axes[0].series = [surface];
surfScene.vertex_count = 6;
surfScene.triangle_count = 4;
check("flat surface", () => assert.equal(validate(surfScene).ok, true, JSON.stringify(validate(surfScene))));
for (const [name, mutate, code] of [
  ["wrong Z shape", (s) => { s.z.shape = [3, 2]; }],
  ["wrong X layout", (s) => { s.coordinate_layout.x = "matrix"; }],
  ["bad cell index", (s) => { s.cell_origins[1] = 4; }, "invalid_index"],
  ["one-based indices", (s) => { s.index_base = 1; }],
  ["triangulation diagonal as edge", (s) => { s.edge_indices.values[7] = 3; }, "invalid_index"],
  ["wrong CData shape", (s) => { s.face_color.data.shape = [3, 2]; }],
  ["unknown CData class", (s) => { s.face_color.cdata_class = "string"; }],
  ["over surface budget", (s) => { s.shape = [201, 200]; }, "budget_exceeded"],
  ["surface reduction", (s) => { s.decimated = true; }],
  ["missing cell retained", (s) => { s.z.values[4] = null; }, "invalid_index"]
]) check(name, () => {
  const v = copy(surfScene);
  mutate(v.axes[0].series[0]);
  const result = validate(v);
  assert.equal(result.ok, false);
  if (code) assert.equal(result.reason_code, code);
});
check("flat vertex-associated mesh edges", () => {
  const v = copy(surfScene), s = v.axes[0].series[0];
  s.edge_color = copy(s.face_color);
  s.edge_color.association = "vertex";
  assert.equal(validate(v).ok, true);
  s.edge_color.association = "cell";
  assert.equal(validate(v).ok, false);
});
check("integer CData is really integer", () => {
  const v = copy(surfScene), s = v.axes[0].series[0];
  s.cdata.values[0] = .5;
  s.face_color.data.values[0] = .5;
  assert.equal(validate(v).ok, false);
});
check("legacy unsupported axes are readable", () => {
  const v = { version: 2, supported: false, decimated: false, reason: "unsupported", axes: [{ supported: false, series: [] }] };
  assert.equal(validate(v).ok, true);
  assert.equal(validate(v).supported, false);
});
check("scatter sizes", () => {
  const v = copy(scene), s = v.axes[0].series[0];
  s.kind = "scatter";
  s.color_data = { encoding: "truecolor", mapping: "scaled", cdata_class: "double", data: descriptor([1, 3], [.2, .4, .6]) };
  s.sizes = [9, 25, 49, 64];
  s.size_units = "points-squared";
  assert.equal(validate(v).ok, true);
  s.sizes = [9, 25];
  assert.equal(validate(v).ok, false);
});
const folder = path.join(__dirname, "fixtures/figure_v3");
const files = fs.readdirSync(folder).filter((f) => f.endsWith(".json"));
for (const file of files) check("real fixture " + file, () => {
  const result = validate(JSON.parse(fs.readFileSync(path.join(folder, file), "utf8")));
  assert.equal(result.ok, true, file + ": " + JSON.stringify(result));
});
if (!files.length) console.log("SKIP real fixture acceptance: no graphics toolkit in lane; run uret.m in Qt Octave.");
console.log(`${count} validator cases passed; ${files.length} real fixtures checked.`);
