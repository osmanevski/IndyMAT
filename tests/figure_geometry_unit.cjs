"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { validate } = require("../frontend/figure_data_utils.cjs");
const G = require("../frontend/figure_geometry_utils.cjs");
const C = require("../frontend/figure_camera_utils.cjs");
const Color = require("../frontend/figure_color_utils.cjs");
let count = 0;
function check(name, fn) {
  fn();
  count++;
  console.log("PASS " + name);
}
function fixture(name) {
  const result = validate(JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json"))));
  assert.equal(result.ok, true);
  return result.data;
}
const folder = path.join(__dirname, "fixtures/figure_v3");
const files = fs.readdirSync(folder).filter((f) => f.endsWith(".json"));
for (const file of files) check("validate then build " + file, () => {
  const result = validate(JSON.parse(fs.readFileSync(path.join(folder, file))));
  assert.equal(result.ok, true);
  if (!result.supported) {
    assert.equal(result.data.axes.length, 0);
    assert.deepEqual(G.buildScene().meshes, []);
  }
  for (const a of result.data.axes) {
    const before = JSON.stringify(a);
    const scene = G.buildScene(a);
    assert.equal(JSON.stringify(a), before);
    assert.equal(scene.axes, a);
    for (const object of [...scene.meshes, ...scene.edges, ...scene.lines, ...scene.points]) assert.ok(object.positions.every(Number.isFinite));
  }
});
check("exact cell connectivity and +Z winding", () => {
  const a = fixture("surf_vector").axes[0];
  const m = G.buildScene(a).meshes[0];
  assert.deepEqual(Array.from(m.source.vertexIndices), [0, 1, 2, 3, 2, 3, 4, 5]);
  assert.deepEqual(Array.from(m.indices), [0, 2, 1, 1, 2, 3, 4, 6, 5, 5, 6, 7]);
  assert.deepEqual(Array.from(m.cellOfTriangle), [0, 0, 2, 2]);
  for (let i = 0; i < m.indices.length; i += 3) {
    const p = Array.from(m.positions.subarray(m.indices[i] * 3, m.indices[i] * 3 + 3));
    const q = Array.from(m.positions.subarray(m.indices[i + 1] * 3, m.indices[i + 1] * 3 + 3));
    const r = Array.from(m.positions.subarray(m.indices[i + 2] * 3, m.indices[i + 2] * 3 + 3));
    assert.ok((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]) > 0);
  }
});
for (const name of ["surf_vector", "surf_matrix_nan", "surf_integer_clim", "mesh_default", "colorbar"]) check("flat buffers and exact grid-only edges " + name, () => {
  const a = fixture(name).axes[0];
  const s = a.series[0];
  const scene = G.buildScene(a);
  const m = scene.meshes[0];
  const e = scene.edges[0];
  const colors = Color.resolveSurfaceColors(s, a);
  assert.equal(m.source.series, s);
  assert.equal(m.source.shape, s.shape);
  assert.equal(m.source.cellOrigins, s.cell_origins);
  for (let i = 0; i < s.cell_origins.length; i++) {
    for (let v = 0; v < 4; v++) assert.deepEqual(m.colors.subarray(i * 12 + v * 3, i * 12 + v * 3 + 3), colors.faceColors.subarray(i * 3, i * 3 + 3));
  }
  const n = s.edge_indices.shape[0];
  assert.equal(e.positions.length, n * 6);
  for (let i = 0; i < n; i++) {
    const source = [s.edge_indices.values[i], s.edge_indices.values[i + n]];
    assert.deepEqual(Array.from(e.source.vertexIndices.subarray(i * 2, i * 2 + 2)), source);
    const [p, q] = source;
    assert.ok(q - p === 1 && Math.floor(p / s.shape[0]) === Math.floor(q / s.shape[0]) || q - p === s.shape[0]);
    for (let v = 0; v < 2; v++) assert.deepEqual(e.colors.subarray(i * 6 + v * 3, i * 6 + v * 3 + 3), colors.edgeColors.subarray(i * 3, i * 3 + 3));
  }
});
check("missing grid vertex removes incident cells, retains boundary edge", () => {
  const a = fixture("surf_matrix_nan").axes[0];
  const scene = G.buildScene(a);
  assert.equal(scene.counts.triangles, 2);
  assert.deepEqual(Array.from(scene.meshes[0].cellOfTriangle), [0, 0]);
  assert.ok(!scene.meshes[0].source.vertexIndices.includes(4));
  assert.ok(scene.edges[0].source.vertexIndices.includes(5));
  assert.ok(!scene.edges[0].source.vertexIndices.includes(4));
});
check("plot3 gap keeps source/rendered mappings and run endpoints", () => {
  const a = fixture("plot3_gap").axes[0];
  const scene = G.buildScene(a);
  const line = scene.lines[0];
  assert.deepEqual(Array.from(line.breaks), [0, 2, 4]);
  assert.deepEqual(Array.from(line.sourceIndices), [0, 1, 3, 4]);
  assert.deepEqual(Array.from(line.source.sampleIndices), [0, 1, 3, 4]);
  assert.equal(line.source.series, a.series[0]);
  const projected = Array.from({ length: 4 }, (_, i) => ({ x: i * 20, y: 0, depth: 0.5 }));
  const expanded = G.expandPolyline(projected, 2, line.breaks);
  assert.equal(expanded.length, 36);
  assert.deepEqual(Array.from(expanded.subarray(0, 9)), [0, 1, 0.5, 0, -1, 0.5, 20, 1, 0.5]);
  assert.equal(expanded[18], 40);
});
check("scatter sizes stay points-squared, marker quads are pixels", () => {
  const p = G.buildScene(fixture("scatter3_sizes").axes[0]).points[0];
  assert.deepEqual(Array.from(p.sizes), [9, 25, 49]);
  assert.equal(p.sizeUnits, "points-squared");
  const markers = G.expandMarkers([{ x: 10, y: 20, depth: 0.5 }, { x: 30, y: 40, depth: 0.75 }], [6, 0]);
  assert.equal(markers.positions.length, 18);
  assert.deepEqual(Array.from(markers.positions.subarray(0, 9)), [7, 17, 0.5, 13, 17, 0.5, 7, 23, 0.5]);
  assert.deepEqual(Array.from(markers.uv.subarray(0, 6)), [-1, -1, 1, -1, -1, 1]);
  assert.deepEqual(Array.from(markers.sourceIndices), [0]);
});
check("screen expansion degeneracy, none style, dash phase across segments", () => {
  const p = [{ x: 0, y: 0, depth: 0.5 }, { x: 5, y: 0, depth: 0.5 }, { x: 14, y: 0, depth: 0.5 }];
  assert.equal(G.expandPolyline([p[0], p[0]], 2).length, 0);
  assert.equal(G.expandPolyline(p, 2, undefined, "none").length, 0);
  const dashed = G.expandPolyline(p, 2, undefined, "--");
  assert.equal(dashed.length, 54);
  assert.equal(dashed[24], 7);
  assert.equal(dashed[36], 11);
  assert.equal(G.expandMarkers([p[0]], 4).positions.length, 18);
});
check("none passes and row/column mesh edge selection", () => {
  for (const mode of ["row", "column"]) {
    const f = fixture("surf_vector");
    const s = f.axes[0].series[0];
    s.mesh_style = mode;
    const all = s.edge_indices;
    const n = all.shape[0];
    const pairs = Array.from({ length: n }, (_, i) => [all.values[i], all.values[i + n]]).filter(([a, b]) => mode === "row" ? b - a === s.shape[0] : b - a === 1);
    s.edge_indices = { ...all, shape: [pairs.length, 2], values: [...pairs.map((p) => p[0]), ...pairs.map((p) => p[1])] };
    assert.equal(validate(f).ok, true);
    assert.equal(G.buildScene(f.axes[0]).counts.edges, pairs.length);
    s.face_color = { mode: "none", association: "constant" };
    s.edge_color = { mode: "none", association: "constant" };
    assert.equal(validate(f).ok, true);
    const scene = G.buildScene(f.axes[0]);
    assert.equal(scene.meshes.length, 0);
    assert.equal(scene.edges.length, 0);
  }
});
check("large offset data retained as doubles, not Float32 coordinates", () => {
  const a = fixture("plot3_gap").axes[0];
  a.xlim = [1e12, 1e12 + 4];
  a.series[0].x = [1e12 + 0.25, 1e12 + 1, null, 1e12 + 3, 1e12 + 3.75];
  const line = G.buildScene(a).lines[0];
  assert.equal(G.samplePoint(line.source.series, line.source.sampleIndices[0])[0], 1e12 + 0.25);
  assert.ok(Math.abs(line.positions[0]) <= 1);
  assert.deepEqual(C.normalisedToData(a, C.dataToNormalised(a, [1e12 + 0.25, 2.5, 4])), [1e12 + 0.25, 2.5, 4]);
});

check("maximum 40000 grid vertices, exact cells and edges", () => {
  const f = fixture("surf_vector");
  const s = f.axes[0].series[0];
  const n = 200;
  const cells = [];
  const edges = [];
  for (let col = 0; col < n - 1; col++) for (let row = 0; row < n - 1; row++) cells.push(row + col * n);
  for (let col = 0; col < n; col++) for (let row = 0; row < n - 1; row++) edges.push([row + col * n, row + 1 + col * n]);
  for (let col = 0; col < n - 1; col++) for (let row = 0; row < n; row++) edges.push([row + col * n, row + (col + 1) * n]);
  const descriptor = (shape, values) => ({ shape, values, order: "column-major" });
  Object.assign(s, { shape: [n, n], x: descriptor([1, n], Array.from({ length: n }, (_, i) => i)),
    y: descriptor([n, 1], Array.from({ length: n }, (_, i) => i)), z: descriptor([n, n], Array(n * n).fill(1)),
    cdata: descriptor([n, n], Array(n * n).fill(1)), cell_origins: cells,
    edge_indices: descriptor([edges.length, 2], [...edges.map((e) => e[0]), ...edges.map((e) => e[1])]),
    triangle_count: cells.length * 2, original_points: n * n, rendered_points: n * n });
  s.face_color.data = structuredClone(s.cdata);
  f.vertex_count = n * n;
  f.triangle_count = cells.length * 2;
  assert.equal(validate(f).ok, true);
  const scene = G.buildScene(f.axes[0]);
  assert.equal(scene.counts.vertices, 40000);
  assert.equal(scene.counts.triangles, 79202);
  assert.equal(scene.counts.meshVertices, 158404);
  assert.equal(scene.counts.edges, 79600);
  assert.equal(scene.meshes[0].indices.at(-1), 158403);
  assert.equal(scene.meshes[0].source.vertexIndices.at(-1), 39999);
});

check("legacy stem/stairs in v3 axes keep gaps and source tips", () => {
  for (const kind of ["stem", "stairs"]) {
    const f = fixture("plot3_gap");
    const a = f.axes[0];
    const s = a.series[0];
    delete s.z;
    s.kind = kind;
    assert.equal(validate(f).ok, true);
    const line = G.buildScene(a).lines[0];
    assert.deepEqual(Array.from(line.breaks), kind === "stem" ? [0, 2, 4, 6, 8] : [0, 3, 6]);
    assert.deepEqual(Array.from(line.source.pickableVertices), kind === "stem" ? [0, 1, 0, 1, 0, 1, 0, 1] : [1, 0, 1, 1, 0, 1]);
    assert.deepEqual(Array.from(line.source.sampleIndices), kind === "stem" ? [0, 0, 1, 1, 3, 3, 4, 4] : [0, 0, 1, 3, 3, 4]);
  }
});
console.log(`${count} geometry cases passed; ${files.length} real fixtures validated.`);
