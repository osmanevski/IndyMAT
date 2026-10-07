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

// 2D filled shapes and text (synthetic records; the real ones are the fixtures above).
const flatAxis = copy(axis);
flatAxis.dimension = 2;
const patch = { id: 0, kind: "patch2d", role: "bar", vertices: descriptor([8, 2], [1, 1, 2, 2, 3, 3, 4, 4, 0, 5, 5, 0, 0, null, null, 0]),
  faces: descriptor([2, 4], [0, 4, 1, 5, 2, 6, 3, 7]), index_base: 0, face_color: [0, .447, .741], edge_color: [0, 0, 0],
  line_style: "-", line_width: .5, display_name: "a", decimated: false, original_points: 8, rendered_points: 8,
  bar: { horizontal: false, layout: "grouped", width: .8, base_value: 0, positions: [1.5, 3.5], values: [5, null] } };
const note = { id: 1, kind: "text", role: "text", units: "data", position: [1, 2], lines: ["a", "\\sigma"], interpreter: "tex",
  horizontal_alignment: "left", vertical_alignment: "top", rotation: 0, font_size: 10, font_weight: "normal", font_angle: "normal",
  color: [0, 0, 0], background_color: [1, 1, 1], edge_color: "none", margin: 3, line_style: "-", line_width: .5, clipping: false,
  display_name: "", decimated: false, original_points: 0, rendered_points: 0 };
const base = { id: 2, kind: "line", role: "bar", span: "horizontal", x: [.5, 4.5], y: [0, 0], line_color: [0, 0, 0], marker_edge_color: [0, 0, 0],
  marker_face_color: "none", marker_face_auto: false, line_style: "-", line_width: .5, marker: "none", marker_size: 6, display_name: "",
  base_value: 0, decimated: false, original_points: 2, rendered_points: 2, source_indices: [0, 1] };
const flat = copy(scene);
flat.axes = [{ ...flatAxis, visible: true, interpreters: { title: "tex", xlabel: "tex", ylabel: "tex", ticks: "tex" }, xtickmode: "auto", xticklabelmode: "manual", series: [patch, note, base] }];
flat.vertex_count = 10;
flat.limits = { ...LIMITS, text_lines: 256, text_chars: 65536 };
check("2D shapes, text and span line", () => {
  const before = copy(flat);
  const result = validate(flat);
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.equal(result.vertex_count, 10);
  assert.deepEqual(flat, before);
});
check("older artifacts without the optional limits stay valid", () => {
  const v = copy(flat);
  v.limits = { ...LIMITS };
  assert.equal(validate(v).ok, true);
});
function badFlat(name, mutate, code) {
  check(name, () => {
    const v = copy(flat);
    mutate(v, v.axes[0].series);
    const result = validate(v);
    assert.equal(result.ok, false, name);
    if (code) assert.equal(result.reason_code, code, name + " " + JSON.stringify(result));
  });
}
badFlat("patch in a v2 record", (v) => { v.version = 2; }, "unsupported_object");
badFlat("patch in 3D axes", (v) => { v.axes[0].dimension = 3; }, "unsupported_object");
badFlat("text in 3D axes", (v, s) => { v.axes[0].dimension = 3; v.axes[0].series = [{ ...s[1], id: 0 }]; v.vertex_count = 0; }, "unsupported_object");
badFlat("patch on a log axis", (v) => { v.axes[0].yscale = "log"; }, "unsupported_patch");
badFlat("face index out of range", (v, s) => { s[0].faces.values[7] = 8; }, "invalid_index");
badFlat("fractional face index", (v, s) => { s[0].faces.values[0] = .5; }, "invalid_index");
badFlat("one-based patch indices", (v, s) => { s[0].index_base = 1; });
badFlat("padding inside a face", (v, s) => { s[0].faces.values[2] = null; }, "invalid_index");
badFlat("three vertex columns", (v, s) => { s[0].vertices.shape = [4, 4]; });
badFlat("vertex count mismatch", (v, s) => { s[0].rendered_points = 7; });
badFlat("patch vertex budget", (v, s) => { s[0].vertices.shape = [40001, 2]; }, "budget_exceeded");
badFlat("patch face budget", (v, s) => { s[0].faces.shape = [40001, 4]; }, "budget_exceeded");
badFlat("interpolated face colour word", (v, s) => { s[0].face_color = "interp"; }, "unsupported_color");
badFlat("out of range face colour", (v, s) => { s[0].face_color = [0, 0, 2]; }, "unsupported_color");
badFlat("unknown patch role", (v, s) => { s[0].role = "pie"; });
badFlat("bar data of the wrong length", (v, s) => { s[0].bar.values.pop(); });
badFlat("bar data on a plain patch", (v, s) => { s[0].role = "patch"; });
badFlat("pixel text units", (v, s) => { s[1].units = "pixels"; }, "unsupported_text");
badFlat("latex interpreter", (v, s) => { s[1].interpreter = "latex"; }, "unsupported_text");
badFlat("nonfinite text position", (v, s) => { s[1].position = [1, null]; });
badFlat("text line that is not a string", (v, s) => { s[1].lines = ["a", 3]; });
badFlat("text line budget", (v, s) => { s[1].lines = Array(257).fill("x"); }, "budget_exceeded");
badFlat("text character budget", (v, s) => { s[1].lines = ["x".repeat(65537)]; }, "budget_exceeded");
badFlat("unnormalised rotation", (v, s) => { s[1].rotation = 360; });
badFlat("unknown alignment", (v, s) => { s[1].vertical_alignment = "centre"; });
badFlat("text without a colour", (v, s) => { s[1].color = "none"; }, "unsupported_color");
badFlat("text counted as points", (v, s) => { s[1].rendered_points = 1; });
badFlat("span on a longer line", (v, s) => { s[2].x = [1, 2, 3]; s[2].y = [0, 0, 0]; s[2].source_indices = [0, 1, 2]; s[2].original_points = 3; s[2].rendered_points = 3; v.vertex_count = 11; });
badFlat("unknown span", (v, s) => { s[2].span = "diagonal"; });
badFlat("span in a v2 record", (v) => { v.version = 2; v.axes[0].series = [v.axes[0].series[2]]; });
badFlat("increased optional limit", (v) => { v.limits.text_lines = 1e6; });
badFlat("non-boolean axes visibility", (v) => { v.axes[0].visible = "off"; });
badFlat("unknown tick mode", (v) => { v.axes[0].xtickmode = "fixed"; });
check("hidden axes that only carry text", () => {
  const v = copy(flat);
  v.axes[0].visible = false;
  v.axes[0].series = [{ ...v.axes[0].series[1], id: 0 }];
  v.vertex_count = 0;
  assert.equal(validate(v).ok, true, JSON.stringify(validate(v)));
});
for (const [file, kinds] of Object.entries({ "bar_grouped.json": ["patch2d", "line", "patch2d"], "text_boxes.json": ["line", "text", "text", "text"], "group_lines.json": ["line", "line", "line", "text", "line"], "hist_flat.json": ["patch2d"] })) check("real 2D fixture " + file, () => {
  const result = validate(JSON.parse(fs.readFileSync(path.join(folder, file), "utf8")));
  assert.equal(result.supported, true, file);
  assert.equal(result.version, 3);
  assert.deepEqual(result.data.axes[0].series.map((s) => s.kind), kinds);
});
console.log(`${count} validator cases in total, including 2D shapes and text.`);
