"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { validate } = require("../frontend/figure_data_utils.cjs");
const C = require("../frontend/figure_camera_utils.cjs");
const G = require("../frontend/figure_geometry_utils.cjs");
const P = require("../frontend/figure_pick_utils.cjs");
let count = 0;
function check(name, fn) {
  fn();
  count++;
  console.log("PASS " + name);
}
function fixture(name) {
  return JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json")));
}
const desc = (shape, values) => ({ shape, values, order: "column-major" });
function surfaceFigure() {
  const figure = fixture("surf_vector");
  const a = figure.axes[0];
  Object.assign(a, { xlim: [0, 1], ylim: [0, 1], zlim: [-1, 1], view: [0, 90], data_aspect_ratio: [1, 1, 2], clim: [10, 40] });
  Object.assign(a.camera, { target: [0.5, 0.5, 0], position: [0.5, 0.5, 10], up_vector: [0, 1, 0] });
  Object.assign(a.series[0], { shape: [2, 2], x: desc([1, 2], [0, 1]), y: desc([2, 1], [0, 1]), z: desc([2, 2], [0, 0, 0, 0]),
    cdata: desc([2, 2], [10, 20, 30, 40]), face_color: { mode: "constant", association: "constant", rgb: [1, 0, 0] },
    edge_color: { mode: "none", association: "constant" }, cell_origins: [0], edge_indices: desc([4, 2], [0, 2, 0, 1, 1, 3, 2, 3]),
    triangle_count: 2, original_points: 4, rendered_points: 4 });
  figure.vertex_count = 4;
  figure.triangle_count = 2;
  return figure;
}
function sample(a, kind, points, sources = points.map((_, i) => i)) {
  const s = fixture(kind === "scatter" ? "scatter3_sizes" : "plot3_gap").axes[0].series[0];
  Object.assign(s, { id: a.series.length, x: points.map((p) => p[0]), y: points.map((p) => p[1]), z: points.map((p) => p[2]),
    source_indices: sources, rendered_points: points.length, original_points: sources.at(-1) + 1, marker: kind === "scatter" ? "o" : "none",
    sizes: points.map(() => 36), decimated: sources.at(-1) + 1 !== points.length });
  a.series.push(s);
  return s;
}
function prepared(figure) {
  figure.vertex_count = figure.axes[0].series.reduce((sum, s) => sum + s.rendered_points, 0);
  figure.triangle_count = figure.axes[0].series.reduce((sum, s) => sum + (s.triangle_count || 0), 0);
  figure.decimated = figure.axes[0].decimated = figure.axes[0].series.some((s) => s.decimated);
  const admitted = validate(figure);
  assert.equal(admitted.ok, true, JSON.stringify(admitted));
  const a = admitted.data.axes[0];
  return { scene: G.buildScene(a), camera: C.createCamera(a), axes: a };
}
const viewport = { width: 600, height: 600 };
function click(state, data, options) {
  const screen = C.project(state.camera, viewport, C.dataToNormalised(state.axes, data));
  return P.pick(state.scene, state.camera, viewport, screen.x, screen.y, options);
}
check("ray triangle exact intersection, winding independent", () => {
  const ray = { origin: [0.25, 0.25, 2], direction: [0, 0, -1] };
  const triangle = [[0, 0, 0], [1, 0, 0], [0, 1, 0]];
  assert.deepEqual(P.intersectTriangle(ray, ...triangle), { distance: 2, point: [0.25, 0.25, 0] });
  assert.deepEqual(P.intersectTriangle(ray, ...triangle.reverse()), { distance: 2, point: [0.25, 0.25, 0] });
  assert.equal(P.intersectTriangle({ origin: [2, 2, 2], direction: [0, 0, -1] }, ...triangle), null);
  assert.equal(P.intersectTriangle({ origin: [0.25, 0.25, 2], direction: [0, 0, 1] }, ...triangle), null);
  assert.equal(P.intersectTriangle(ray, [0, 0, 0], [0, 0, 0], [0, 0, 0]), null);
});
check("surface known ray snaps row/column and raw CData", () => {
  const state = prepared(surfaceFigure());
  const hit = click(state, [0.1, 0.1, 0]);
  assert.deepEqual(hit, { kind: "surface", series: 0, row: 0, column: 0, data: { x: 0, y: 0, z: 0 }, colorValue: 10, depth: 0.5 });
  const opposite = click(state, [0.9, 0.9, 0]);
  assert.equal(opposite.row, 1);
  assert.equal(opposite.column, 1);
  assert.equal(opposite.colorValue, 40);
});
check("frontmost face wins independent of series order", () => {
  for (const reverse of [false, true]) {
    const f = surfaceFigure();
    const a = f.axes[0];
    const front = structuredClone(a.series[0]);
    front.z.values.fill(0.5);
    front.id = 1;
    a.series[0].z.values.fill(-0.5);
    a.series.push(front);
    if (reverse) {
      a.series.reverse();
      a.series.forEach((s, i) => { s.id = i; });
    }
    const hit = click(prepared(f), [0.1, 0.1, 0]);
    assert.equal(hit.kind, "surface");
    assert.equal(hit.data.z, 0.5);
    assert.equal(hit.series, reverse ? 0 : 1);
    assert.ok(hit.depth < 0.5);
  }
});
check("point behind surface loses, point in front wins", () => {
  const f = surfaceFigure();
  sample(f.axes[0], "scatter", [[0.1, 0.1, -0.5]]);
  assert.equal(click(prepared(f), [0.1, 0.1, 0]).kind, "surface");
  f.axes[0].series[1].z[0] = 0.5;
  const hit = click(prepared(f), [0.1, 0.1, 0]);
  assert.equal(hit.kind, "point");
  assert.deepEqual(hit.data, { x: 0.1, y: 0.1, z: 0.5 });
  assert.deepEqual(hit.colorValue, [0.2, 0.4, 0.6]);
});
check("nearer point wins over closer-in-screen but hidden point", () => {
  const f = surfaceFigure();
  sample(f.axes[0], "scatter", [[0.1, 0.1, -0.5], [0.11, 0.1, 0.5]]);
  const hit = click(prepared(f), [0.1, 0.1, 0]);
  assert.equal(hit.kind, "point");
  assert.equal(hit.index, 1);
});
check("line sample original indices and doubles, depth occlusion", () => {
  const f = surfaceFigure();
  sample(f.axes[0], "line", [[0.1, 0.1, -0.5], [0.3, 0.3, -0.5]], [13, 17]);
  assert.equal(click(prepared(f), [0.1, 0.1, 0]).kind, "surface");
  const s = f.axes[0].series[1];
  s.z.fill(0.5);
  s.x[0] = 0.10000000000000003;
  const hit = click(prepared(f), [0.1, 0.1, 0]);
  assert.equal(hit.kind, "line");
  assert.equal(hit.index, 13);
  assert.equal(hit.data.x, 0.10000000000000003);
  assert.notEqual(hit.data.x, Math.fround(hit.data.x));
});
check("point picking uses own centre occlusion, even off clicked surface", () => {
  const f = surfaceFigure();
  sample(f.axes[0], "scatter", [[0.99, 0.5, -0.5]]);
  const state = prepared(f);
  const p = C.project(state.camera, viewport, C.dataToNormalised(state.axes, [1.01, 0.5, 0]));
  assert.equal(P.pick(state.scene, state.camera, viewport, p.x, p.y, { pointRadiusPixels: 10 }), null);
});
check("zero-sized markers and isolated lines are not visible hits", () => {
  const f = surfaceFigure();
  f.axes[0].series = [];
  sample(f.axes[0], "scatter", [[0.5, 0.5, 0]]).sizes = [0];
  sample(f.axes[0], "line", [[0.5, 0.5, 0]]);
  assert.equal(click(prepared(f), [0.5, 0.5, 0]), null);
});
check("axes and viewport clipping", () => {
  const f = surfaceFigure();
  f.axes[0].series[0].z.values.fill(2);
  const state = prepared(f);
  assert.equal(click(state, [0.1, 0.1, 0]), null);
  assert.equal(P.pick(state.scene, state.camera, viewport, -1, 100), null);
  assert.equal(P.pick(state.scene, state.camera, viewport, 100, 601), null);
  assert.equal(P.pick(state.scene, state.camera, viewport, NaN, 100), null);
});
check("surface disabled faces do not occlude points", () => {
  const f = surfaceFigure();
  f.axes[0].series[0].face_color = { mode: "none", association: "constant" };
  sample(f.axes[0], "scatter", [[0.1, 0.1, -0.5]]);
  assert.equal(click(prepared(f), [0.1, 0.1, 0]).kind, "point");
});
check("pick full precision surface at large data offsets", () => {
  const f = surfaceFigure();
  f.axes[0].xlim = [1e12, 1e12 + 1];
  f.axes[0].camera.target[0] = 1e12 + 0.5;
  f.axes[0].series[0].x.values = [1e12 + 0.125, 1e12 + 0.875];
  const hit = click(prepared(f), [1e12 + 0.2, 0.1, 0]);
  assert.equal(hit.data.x, 1e12 + 0.125);
  assert.notEqual(hit.data.x, Math.fround(hit.data.x));
});
check("surface snap considers fourth corner outside hit triangle", () => {
  const state = prepared(surfaceFigure());
  const hit = click(state, [0.8, 0.8, 0]);
  assert.deepEqual(hit.data, { x: 1, y: 1, z: 0 });
});
check("fixture gap maps pick to original sample without fabricated bridge", () => {
  const f = fixture("plot3_gap");
  const admitted = validate(f);
  assert.equal(admitted.ok, true);
  const a = admitted.data.axes[0];
  const state = { axes: a, camera: C.createCamera(a), scene: G.buildScene(a) };
  const hit = click(state, [4, 6, 8]);
  assert.equal(hit.index, 3);
  assert.deepEqual(hit.data, { x: 4, y: 6, z: 8 });
});
console.log(`${count} picking cases passed.`);
