"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
const { validate } = require("../frontend/figure_data_utils.cjs");
const colors = require("../frontend/figure_color_utils.cjs");
const geometry = require("../frontend/figure_geometry_utils.cjs");
const cameraMath = require("../frontend/figure_camera_utils.cjs");
const picking = require("../frontend/figure_pick_utils.cjs");
const tools = require("../frontend/figure_tool_utils.cjs");
const descriptor = (shape, values) => ({ shape, order: "column-major", values });
const fixtureDir = path.join(__dirname, "fixtures/figure_v3");
let count = 0;
function test(name, run) { run(); count++; console.log("PASS " + name); }

// Synthetic IN-MEMORY inputs derived from a recorded flat fixture. These are
// validator/renderer unit cases, never represented as measured Octave output.
function figure({ rows = 2, cols = 2, values = [0, 1, 2, 3], mapping = "scaled", cdataClass = "double", encoding = "indexed", missing = [] } = {}) {
  const data = JSON.parse(fs.readFileSync(path.join(fixtureDir, "surf_vector.json")));
  const axes = data.axes[0];
  Object.assign(axes, { xlim: [0, cols - 1], ylim: [0, rows - 1], zlim: [-1, 1],
    view: [0, 90], clim: [0, 4], data_aspect_ratio: [1, 1, 2],
    colormap: descriptor([4, 3], [1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0]) });
  Object.assign(axes.camera, { target: [(cols - 1) / 2, (rows - 1) / 2, 0],
    position: [(cols - 1) / 2, (rows - 1) / 2, 10], up_vector: [0, 1, 0] });
  const s = axes.series[0];
  const n = rows * cols;
  const raw = descriptor(encoding === "truecolor" ? [rows, cols, 3] : [rows, cols], values.slice());
  Object.assign(s, { shape: [rows, cols], coordinate_layout: { x: "vector", y: "vector" },
    x: descriptor([1, cols], Array.from({ length: cols }, (_, i) => i)),
    y: descriptor([rows, 1], Array.from({ length: rows }, (_, i) => i)),
    z: descriptor([rows, cols], Array.from({ length: n }, (_, i) => missing.includes(i) ? null : 0)),
    cdata_class: cdataClass, cdata_mapping: mapping, cdata: raw,
    face_color: { mode: "interp", association: "vertex", encoding, mapping, cdata_class: cdataClass, data: structuredClone(raw) },
    edge_color: { mode: "none", association: "constant" }, cell_origins: [],
    original_points: n, rendered_points: n, display_name: "Interp source" });
  const good = (i) => !missing.includes(i);
  for (let c = 0; c < cols - 1; c++) for (let r = 0; r < rows - 1; r++) {
    const i = r + c * rows;
    if ([i, i + 1, i + rows, i + rows + 1].every(good)) s.cell_origins.push(i);
  }
  const edges = [];
  for (let c = 0; c < cols; c++) for (let r = 0; r < rows - 1; r++) {
    const i = r + c * rows;
    if (good(i) && good(i + 1)) edges.push([i, i + 1]);
  }
  for (let c = 0; c < cols - 1; c++) for (let r = 0; r < rows; r++) {
    const i = r + c * rows;
    if (good(i) && good(i + rows)) edges.push([i, i + rows]);
  }
  s.edge_indices = descriptor([edges.length, 2], [...edges.map((e) => e[0]), ...edges.map((e) => e[1])]);
  data.vertex_count = n;
  s.triangle_count = data.triangle_count = s.cell_origins.length * 2;
  return data;
}
const admitted = (f) => { const result = validate(f); assert(result.ok && result.supported, JSON.stringify(result)); return result.data.axes[0]; };
const nearRGB = (actual, expected) => actual.forEach((v, i) => assert(Math.abs(v - expected[i]) < 1e-6, `${actual} != ${expected}`));
function resolved(options, expected) {
  const axes = admitted(figure(options));
  const result = colors.resolveSurfaceColors(axes.series[0], axes);
  assert.equal(result.faceColors.length, 0);
  assert.equal(result.edgeColors.length, 0);
  nearRGB([...result.vertexColors], expected.flat());
  return axes;
}
test("all 38 stored fixtures remain valid without edits", () => {
  const files = fs.readdirSync(fixtureDir).filter((name) => name.endsWith(".json"));
  assert.equal(files.length, 38);
  for (const filename of files) assert(validate(JSON.parse(fs.readFileSync(path.join(fixtureDir, filename)))).ok, filename);
});
test("vertex interp admitted without mutation", () => {
  const f = figure(); const before = structuredClone(f); admitted(f); assert.deepEqual(f, before);
});
for (const [name, mutate] of [
  ["cell association", (s) => { s.face_color.association = "cell"; }],
  ["constant association", (s) => { s.face_color.association = "constant"; }],
  ["missing association", (s) => { delete s.face_color.association; }],
  ["edge interp", (s) => { s.edge_color = structuredClone(s.face_color); }],
  ["cell-sized CData", (s) => { s.cdata = s.face_color.data = descriptor([1, 1], [0]); }],
  ["transposed CData", (s) => { s.face_color.data.shape = [1, 4]; }],
  ["missing data", (s) => { delete s.face_color.data; }],
  ["truncated data", (s) => { s.face_color.data.values.pop(); }],
  ["extra data", (s) => { s.face_color.data.values.push(1); }],
  ["row-major data", (s) => { s.face_color.data.order = "row-major"; }],
  ["scalar values", (s) => { s.face_color.data.values = 1; }],
  ["unknown encoding", (s) => { s.face_color.encoding = "texture"; }],
  ["wrong truecolor rank", (s) => { s.face_color.encoding = "truecolor"; }],
  ["unknown mapping", (s) => { s.face_color.mapping = "none"; }],
  ["parent mapping mismatch", (s) => { s.face_color.mapping = "direct"; }],
  ["unknown class", (s) => { s.face_color.cdata_class = "logical"; }],
  ["parent class mismatch", (s) => { s.face_color.cdata_class = "uint8"; }],
  ["parent values mismatch", (s) => { s.face_color.data.values[0] = 1; }],
  ["NaN JSON number", (s) => { s.cdata.values[0] = s.face_color.data.values[0] = NaN; }],
  ["null referenced origin", (s) => { s.cdata.values[0] = s.face_color.data.values[0] = null; }],
  ["null referenced non-origin vertex", (s) => { s.cdata.values[3] = s.face_color.data.values[3] = null; }]
]) test("reject malformed interp: " + name, () => {
  const f = figure(); mutate(f.axes[0].series[0]); assert.equal(validate(f).ok, false);
});
for (const name of ["encoding", "mapping", "cdata_class"]) test("reject missing interp " + name, () => {
  const f = figure(); delete f.axes[0].series[0].face_color[name]; assert.equal(validate(f).ok, false);
});
const R = [1, 0, 0], GREEN = [0, 1, 0], BLUE = [0, 0, 1], YELLOW = [1, 1, 0];
test("scaled vertices include CLim clipping", () => resolved({ values: [-20, 1, 2, 100] }, [R, GREEN, BLUE, YELLOW]));
for (const cdataClass of ["double", "single"]) {
  test("floating direct truncation/clipping " + cdataClass, () => resolved({ values: [-10, 2.9, 3, 100], mapping: "direct", cdataClass }, [R, GREEN, BLUE, YELLOW]));
}
for (const cdataClass of ["int8", "uint8", "int16", "uint16", "int32", "uint32", "int64", "uint64"]) {
  test("integer direct zero-based/clipping " + cdataClass, () => resolved({ values: [0, 1, 2, 100], mapping: "direct", cdataClass }, [R, GREEN, BLUE, YELLOW]));
  test("integer scaled " + cdataClass, () => resolved({ values: [0, 1, 2, 100], cdataClass }, [R, GREEN, BLUE, YELLOW]));
}
test("negative signed direct indexes clip", () => resolved({ values: [-128, -1, 2, 127], mapping: "direct", cdataClass: "int8" }, [R, R, BLUE, YELLOW]));
test("fractional and overflowing integer values rejected", () => {
  for (const value of [-1, .5, 256]) assert.equal(validate(figure({ cdataClass: "uint8", values: [0, 1, 2, value] })).ok, false);
  for (const cdataClass of ["int64", "uint64"]) assert.equal(validate(figure({ cdataClass, values: [0, 1, 2, 2 ** 53] })).ok, false);
});
for (const [cdataClass, max] of [["double", 1], ["single", 1], ["uint8", 255], ["uint16", 65535]]) {
  for (const mapping of ["scaled", "direct"]) test("truecolor interp bypasses map/CLim " + cdataClass + " " + mapping, () => {
    resolved({ values: [max, 0, 0, max, 0, max, 0, max, 0, 0, max, 0], cdataClass, encoding: "truecolor", mapping }, [R, GREEN, BLUE, YELLOW]);
  });
}
test("malformed truecolor channels, range, class and null rejected", () => {
  for (const mutation of [(s) => { s.cdata.shape = s.face_color.data.shape = [2, 2, 2]; },
    (s) => { s.cdata.values[3] = s.face_color.data.values[3] = 256; },
    (s) => { s.cdata_class = s.face_color.cdata_class = "int8"; },
    (s) => { s.cdata.values[11] = s.face_color.data.values[11] = null; }]) {
    const f = figure({ encoding: "truecolor", cdataClass: "uint8", values: Array(12).fill(0) });
    mutation(f.axes[0].series[0]); assert.equal(validate(f).ok, false);
  }
});
test("reversed colormap resolves vertex RGB without resampling", () => {
  const f = figure({ mapping: "direct", cdataClass: "uint8" });
  const a = f.axes[0];
  a.colormap.values = [1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0];
  const s = admitted(f).series[0];
  nearRGB([...colors.resolveSurfaceColors(s, a).vertexColors], [YELLOW, BLUE, GREEN, R].flat());
});
test("fixed off-origin diagonal and per-source-vertex RGB", () => {
  const a = admitted(figure()); const mesh = geometry.buildScene(a).meshes[0];
  assert.deepEqual([...mesh.indices], [0, 2, 1, 1, 2, 3]);
  assert.deepEqual([...mesh.source.vertexIndices], [0, 1, 2, 3]);
  assert.deepEqual([...mesh.cellOfTriangle], [0, 0]);
  nearRGB([...mesh.colors], [R, GREEN, BLUE, YELLOW].flat());
  assert.equal(mesh.colorInterpolation, colors.SURFACE_COLOR_INTERPOLATION);
  // On the shared diagonal halfway between green and blue, A is cyan/2.
  const middleRGB = GREEN.map((v, i) => (v + BLUE[i]) / 2);
  assert.deepEqual(middleRGB, [0, .5, .5]);
  assert.notDeepEqual(middleRGB, colors.descriptorColor(a.series[0].face_color, 1, a), "A must not become a fragment colormap lookup");
});
test("3x3 buffer duplicates preserve corner identity and grid edges have no diagonals", () => {
  const a = admitted(figure({ rows: 3, cols: 3, values: [0, 1, 2, 3, 2, 1, 0, 3, 2] }));
  const s = a.series[0];
  s.edge_color = { mode: "flat", association: "vertex", encoding: "indexed", mapping: s.cdata_mapping, cdata_class: s.cdata_class, data: structuredClone(s.cdata) };
  admitted({ ...figure({ rows: 3, cols: 3, values: s.cdata.values }), axes: [a] });
  const scene = geometry.buildScene(a), mesh = scene.meshes[0];
  assert.equal(scene.counts.triangles, 8); assert.equal(scene.counts.edges, 12);
  assert.deepEqual([...mesh.source.vertexIndices], [0, 1, 3, 4, 1, 2, 4, 5, 3, 4, 6, 7, 4, 5, 7, 8]);
  for (let i = 0; i < mesh.source.vertexIndices.length; i++) nearRGB([...mesh.colors.subarray(i * 3, i * 3 + 3)], colors.descriptorColor(s.face_color, mesh.source.vertexIndices[i], a));
  for (let i = 0; i < s.edge_indices.shape[0]; i++) assert([1, 3].includes(s.edge_indices.values[i + 12] - s.edge_indices.values[i]));
});
test("80x80 vertex grid stays exact and the 40000 vertex budget is unchanged", () => {
  const a = admitted(figure({ rows: 80, cols: 80, values: Array.from({ length: 6400 }, (_, i) => i % 4) }));
  const scene = geometry.buildScene(a);
  assert.equal(scene.counts.vertices, 6400);
  assert.equal(scene.counts.meshVertices, 79 * 79 * 4);
  assert.equal(scene.counts.triangles, 79 * 79 * 2);
  assert([...scene.meshes[0].colors].every(Number.isFinite));
  const over = validate(figure({ rows: 201, cols: 200, values: Array(40200).fill(1) }));
  assert.equal(over.reason_code, "budget_exceeded");
  assert.deepEqual(over.reason_args, { budget: "surface_vertices", actual: 40200, limit: 40000 });
});
test("missing XYZ and colour stay unreferenced; flat and interp have identical holes", () => {
  const f = figure({ rows: 3, cols: 3, values: [0, 1, 2, 3, 0, 1, 2, 3, null], missing: [8] });
  const a = admitted(f), s = a.series[0], scene = geometry.buildScene(a);
  assert.deepEqual(s.cell_origins, [0, 1, 3]); assert.equal(scene.counts.triangles, 6);
  assert(!scene.meshes[0].source.vertexIndices.includes(8));
  assert([...scene.meshes[0].colors].every(Number.isFinite));
  assert([...colors.resolveSurfaceColors(s, a).vertexColors.subarray(24)].every(Number.isNaN));
  assert.equal(colors.descriptorColor(s.face_color, 8, a), null, "null CData must not map to index zero");
  const viewport = { width: 600, height: 600 }, camera = cameraMath.createCamera(a);
  const pixel = cameraMath.project(camera, viewport, cameraMath.dataToNormalised(a, [1.5, 1.5, 0]));
  assert.equal(picking.pick(scene, camera, viewport, pixel.x, pixel.y), null, "missing cell was pickable");
  const flat = structuredClone(a); Object.assign(flat.series[0].face_color, { mode: "flat", association: "cell" });
  assert.deepEqual(geometry.buildScene(flat).meshes[0].indices, scene.meshes[0].indices);
  assert.deepEqual(geometry.buildScene(flat).meshes[0].positions, scene.meshes[0].positions);
});
test("truecolor NaN at a missing vertex is unused; a referenced channel rejects", () => {
  const f = figure({ rows: 3, cols: 3, encoding: "truecolor", values: Array.from({ length: 27 }, (_, i) => i % 9 === 8 ? null : .5), missing: [8] });
  const a = admitted(f); assert([...geometry.buildScene(a).meshes[0].colors].every(Number.isFinite));
  assert.equal(colors.descriptorColor(a.series[0].face_color, 8, a), null);
  a.series[0].cdata.values[9] = a.series[0].face_color.data.values[9] = null;
  assert.equal(validate(f).ok, false);
});
test("surface tips snap to original XYZ/raw CData, including truecolor bytes and ties", () => {
  const vp = { width: 600, height: 600 };
  for (const f of [figure({ values: [10, 20, 30, 40] }), figure({ encoding: "truecolor", cdataClass: "uint8", values: [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 0] })]) {
    const a = admitted(f), scene = geometry.buildScene(a), camera = cameraMath.createCamera(a);
    for (const [point, index] of [[[.1, .1, 0], 0], [[.9, .9, 0], 3], [[.5, .5, 0], 0]]) {
      const pixel = cameraMath.project(camera, vp, cameraMath.dataToNormalised(a, point));
      const hit = picking.pick(scene, camera, vp, pixel.x, pixel.y);
      assert.equal(hit.row, index % 2); assert.equal(hit.column, Math.floor(index / 2));
      assert.deepEqual(Object.values(hit.data), geometry.surfacePoint(a.series[0], index));
      assert.deepEqual(hit.colorValue, colors.rawColorValue(a.series[0], index));
      const text = tools.tipText({ ...hit.data, row: hit.row, column: hit.column, colorValue: hit.colorValue });
      assert(text.includes("Color value"));
      assert(!text.includes("0.1"), "tip displayed interpolated XYZ instead of source data");
    }
  }
});

// Mocked GL command/buffer verification. This deliberately cannot certify GPU
// compilation, driver interpolation, Qt/MATLAB pixels or browser interaction.
const filename = path.resolve(__dirname, "../frontend/figure_webgl.js");
const bundle = esbuild.buildSync({ entryPoints: [filename], bundle: true, write: false, platform: "node", format: "cjs" });
const loaded = new Module(filename, module);
loaded._compile(bundle.outputFiles[0].text, filename);
const { FigureWebGL, surfaceColorShaderPath } = loaded.exports;
function mockGL() {
  const calls = [], live = new Set();
  return new Proxy({ calls, live, getShaderParameter: () => true, getProgramParameter: () => true,
    getUniformLocation: (_, name) => name, getExtension: () => null }, { get(target, name) {
    if (name in target) return target[name];
    if (/^[A-Z_]+$/.test(name)) return name;
    if (name.startsWith("create")) return () => { const resource = {}; live.add(resource); calls.push([name, resource]); return resource; };
    if (name.startsWith("delete")) return (resource) => { assert(live.delete(resource)); calls.push([name, resource]); };
    return (...args) => calls.push([name, ...args.map((v) => ArrayBuffer.isView(v) ? v.slice() : v)]);
  } });
}
test("contained shader policy defaults to smooth vertex RGB and rejects an unimplemented B", () => {
  assert.equal(colors.SURFACE_COLOR_INTERPOLATION, "vertex-rgb");
  assert(surfaceColorShaderPath(colors.SURFACE_COLOR_INTERPOLATION).includes("return vertexRGB"));
  assert.throws(() => surfaceColorShaderPath("scalar-colormap"), /Unsupported surface color interpolation/);
});
test("interp static uploads and opaque two-sided depth state persist through camera changes", () => {
  const a = admitted(figure()), scene = geometry.buildScene(a), gl = mockGL();
  a.visible = false; // isolate the data draw from the axes' screen decorations
  const listeners = new Map();
  const canvas = { width: 600, height: 600, getContext: () => gl,
    addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: (name) => listeners.delete(name) };
  const renderer = new FigureWebGL(canvas, [scene], () => assert.fail("unexpected fallback"));
  const uploads = gl.calls.filter(([name]) => name === "bufferData");
  assert.deepEqual(uploads[1][2], scene.meshes[0].colors);
  assert.equal(uploads[1][3], gl.STATIC_DRAW);
  assert(gl.calls.some(([name, , shader]) => name === "shaderSource" && shader.includes("smooth out vec3 ink")));
  assert(gl.calls.some(([name, , shader]) => name === "shaderSource" && shader.includes("smooth in vec3 ink")));
  const staticCount = gl.calls.filter(([name, , , usage]) => name === "bufferData" && usage === gl.STATIC_DRAW).length;
  const camera = cameraMath.createCamera(a);
  for (const move of [() => {}, () => cameraMath.orbit(camera, 20, 10), () => cameraMath.pan(camera, 10, -5, { width: 600, height: 600 }), () => cameraMath.zoom(camera, 1.5)]) {
    move();
    renderer.render([{ scene, camera, viewport: { x: 0, y: 0, width: 600, height: 600 } }], { background: [0, 0, 0], text: [1, 1, 1], grid: [.2, .2, .2], axis: [.3, .3, .3] }, 1);
  }
  assert.equal(gl.calls.filter(([name]) => name === "drawElements").length, 4);
  assert(gl.calls.some(([name, value]) => name === "enable" && value === gl.DEPTH_TEST));
  for (const state of [gl.CULL_FACE, gl.BLEND]) assert(gl.calls.some(([name, value]) => name === "disable" && value === state));
  assert(gl.calls.some(([name, , value]) => name === "uniform1i" && value === 0));
  assert.equal(gl.calls.filter(([name, , , usage]) => name === "bufferData" && usage === gl.STATIC_DRAW).length, staticCount);
  assert(!gl.calls.some(([name]) => /Texture/.test(name)), "default interpolation added a GL resource type");
  renderer.dispose(); renderer.dispose(); assert.equal(gl.live.size, 0); assert.equal(listeners.size, 0);
});
console.log(`${count} interp cases passed. All new scene inputs are synthetic in-memory; GL is mocked.`);
