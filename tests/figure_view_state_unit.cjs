"use strict";
const assert = require("node:assert/strict");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");

(async () => {
  const registry = {};
  global.figureViewHarness = { registry };
  const build = await esbuild.build({ stdin: { contents: 'import "./figures.js";', resolveDir: path.resolve("frontend") }, bundle: true, write: false, platform: "node", format: "cjs", plugins: [{ name: "boundaries", setup(build) {
    build.onResolve({ filter: /^\.\/(state|registry|figure3d|figure_tools|figure_window_controls|i18n)\.js$/ }, (args) => ({ path: path.basename(args.path), namespace: "stub" }));
    build.onLoad({ filter: /.*/, namespace: "stub" }, (args) => ({ contents: {
      "state.js": "export default {};",
      "registry.js": "export default globalThis.figureViewHarness.registry;",
      "figure3d.js": "export class Figure3D {}; export function drawColorbars() {}",
      "figure_tools.js": "export const figureReasonText=()=>'';export const figureReductionText=()=>'';export function unmountFigureTools(){}",
      "figure_window_controls.js": "export const mountFigureWindowControls=()=>()=>{};",
      "i18n.js": "export const t=x=>x; export const onLanguageChange=()=>()=>{};"
    }[args.path], loader: "js" }));
  } }] });
  const loaded = new Module(path.resolve("frontend/view_test.js"), module);
  loaded._compile(build.outputFiles[0].text, path.resolve("frontend/view_test.js"));
  const proto = registry.InteractiveFigure.prototype;
  const image = (values) => ({ kind: "image", shape: [2, 2], x: [1, 2], y: [1, 2], cdata: { shape: [2, 2], values }, encoding: "indexed" });
  const line = { kind: "line", display_name: "visible line" };
  const axes = (series) => ({ series, visible: true, xlim: [0, 4], ylim: [0, 4], xscale: "linear", yscale: "linear", xdir: "normal", ydir: "normal" });
  const view = (series) => Object.assign(Object.create(proto), { data: { axes: [axes(series)] }, box: () => ({ x: 0, y: 0, w: 4, h: 4 }), pixelData: (v) => v, hits: [], faces: [] });
  const bottom = image([1, 3, 2, 4]), top = image([5, 7, 6, 8]);
  let plot = view([bottom, line, top]);
  plot.hits = [{ x: 1, y: 1, vx: 1, vy: 1, axis: 0, seriesIndex: 1, series: line }];
  assert.equal(plot.hitAt({ x: 1, y: 1 }).pixel.value, 5, "opaque image must hide older line");
  plot = view([bottom, top, line]);
  plot.hits = [{ x: 1, y: 1, vx: 1, vy: 1, axis: 0, seriesIndex: 2, series: line }];
  assert.equal(plot.hitAt({ x: 1, y: 1 }).series, line, "later line remains selectable");
  const patch = { kind: "patch2d", face_color: [1, 0, 0] };
  const face = { axis: 0, seriesIndex: 1, series: patch, points: [[.5, .5], [2.5, .5], [2.5, 2.5], [.5, 2.5]], tip: { x: 1, y: 1 } };
  plot = view([bottom, patch, top]); plot.faces = [face];
  assert.equal(plot.hitAt({ x: 1, y: 1 }).pixel.value, 5, "opaque image must hide older face");
  plot = view([bottom, top, patch]); plot.faces = [{ ...face, seriesIndex: 2 }];
  assert.equal(plot.hitAt({ x: 1, y: 1 }).series, patch, "later filled face remains selectable");
  plot = view([bottom, line, top]);
  plot.hits = [{ x: 3, y: 1, vx: 3, vy: 1, axis: 0, seriesIndex: 1, series: line }];
  assert.equal(plot.hitAt({ x: 3, y: 1 }).series, line, "image masks only its extent");
  plot.hits[0].x = 1;
  assert.equal(plot.hitAt({ x: 3, y: 1 }), null, "sample hidden by image cannot be picked nearby outside image");
  plot = view([bottom]);
  plot.data.axes.push(axes([top]));
  assert.equal(plot.hitAt({ x: 1, y: 1 }).pixel.value, 5, "later axes owns overlapping image");
  console.log("PASS image/line/face/axes stacking, visible overlays, bounds and occluded nearby sample");

  const triangle = { kind: "patch2d", face_color: [0, 1, 0], edge_color: "none", line_style: "none", line_width: 1, vertices: { shape: [3, 2], values: [.5, 1.5, 1, .5, .5, 1.5] }, faces: { shape: [1, 3], values: [0, 1, 2] } };
  const paintedTriangle = (patch, series = [bottom, line, patch]) => {
    const plot = view(series);
    plot.box = () => ({ x: 0, y: 0, w: 400, h: 400 });
    plot.dataPixel = (v) => v * 100; plot.pixelData = (v) => v / 100;
    plot.ctx = new Proxy({}, { get: (obj, key) => obj[key] || (() => {}) });
    plot.palette = {};
    const lineIndex = series.indexOf(line);
    plot.hits = [{ x: 100, y: 100, vx: 1, vy: 1, axis: 0, seriesIndex: lineIndex, series: line }];
    plot.drawPatch(patch, 0, series.indexOf(patch));
    return plot;
  };
  plot = paintedTriangle(triangle);
  assert.equal(plot.faces.length, 1, "actual general polygon paint must record occlusion");
  assert.equal(plot.faces[0].tip, null, "triangle must not invent a rectangular face tip");
  assert.equal(plot.hitAt({ x: 100, y: 100 }), null, "opaque triangle masks both older line and image at interior");
  assert.equal(plot.hitAt({ x: 50, y: 50 }).series, triangle, "original triangle vertices remain selectable");
  plot = paintedTriangle(triangle, [bottom, triangle, line]);
  assert.equal(plot.hitAt({ x: 100, y: 100 }).series, line, "later overlay remains visible above triangle");
  plot = paintedTriangle({ ...triangle, face_color: "none" });
  assert.equal(plot.hitAt({ x: 100, y: 100 }).series, line, "unfilled triangle does not obscure interior");
  plot = paintedTriangle({ ...triangle, role: "textbox" });
  assert.equal(plot.hitAt({ x: 100, y: 100 }), null, "opaque textbox masks older data without numerical tips");
  console.log("PASS actual general patch paint masks hidden objects without inventing face tips; vertices and later overlays remain selectable");

  const sharedTitle = { series: [{ figure_title: true, font_size: 12, lines: ["Shared", "Second line"], margin: 2 }] };
  const left = { ...axes([line]), position: [.1, .099, .35, .7335], title_layout_position: [.1, .11, .35, .815] };
  const right = { ...left, position: [.55, .099, .35, .7335], title_layout_position: [.55, .11, .35, .815] };
  const complete = [left, right, sharedTitle];
  const narrow = { x: 0, y: 0, width: 500, height: 172 };
  const twoD = Object.assign(Object.create(proto), { data: { axes: [right], figure_axes: complete }, figureViewport: narrow });
  const vp = require("../frontend/figure_viewport_utils.cjs");
  const expected = vp.fitFigureAxes(left, narrow, complete), actual = twoD.axesViewport(0);
  assert.equal(actual.y, expected.y); assert.equal(actual.height, expected.height);
  assert(actual.height < right.position[3]*narrow.height, "narrow mixed 2D axes must reserve extra shared-title space");
  console.log("PASS mixed 2D geometry uses complete shared title and aligns with 3D frame reserve");

  const captured = new Set();
  const canvas = { classList: { toggle() {}, remove() {} }, getBoundingClientRect: () => ({ left: 0, top: 0 }), setPointerCapture: (id) => captured.add(id), hasPointerCapture: (id) => captured.has(id), releasePointerCapture: (id) => captured.delete(id) };
  plot = Object.assign(Object.create(proto), { canvas, data: { axes: [axes([line])] }, initial: [{ x: [0, 4], y: [0, 4] }], views: [{ x: [1, 3], y: [1, 3] }], width: 400, height: 400, tip: { hidden: false, style: {}, offsetHeight: 30 }, box: () => ({ x: 0, y: 0, w: 400, h: 400 }), axisAt: () => 0, draw() {} });
  plot.bind();
  const down = () => canvas.onpointerdown({ button: 0, shiftKey: true, pointerId: 9, clientX: 100, clientY: 100 });
  down(); canvas.onpointermove({ clientX: 150, clientY: 100 });
  assert.notDeepEqual(plot.views, plot.initial);
  canvas.onkeydown({ key: "Home", preventDefault() {} });
  assert.equal(plot.drag, null); assert.equal(captured.size, 0); assert.deepEqual(plot.views, plot.initial);
  canvas.onpointerup({ pointerId: 9, clientX: 180, clientY: 120 });
  assert.deepEqual(plot.views, plot.initial, "pointerup cannot reapply cancelled drag after reset");
  for (const handler of ["onpointercancel", "onlostpointercapture"]) {
    down(); canvas[handler]({ pointerId: 10 }); assert(plot.drag, "unrelated pointer must not cancel");
    canvas[handler]({ pointerId: 9 }); assert.equal(plot.drag, null); assert.equal(captured.size, 0);
  }
  console.log("PASS reset during pan releases capture; cancelled/lost/unrelated pointer handling");

  plot.hits = [{ x: 100, y: 100, vx: 1, vy: 3, axis: 0, seriesIndex: 0, series: line }]; plot.faces = [];
  plot.showHit({ x: 100, y: 100 }, true);
  assert.equal(plot.tip.style.left, "109px");
  const text = plot.tip.textContent;
  plot.box = () => ({ x: 0, y: 0, w: 800, h: 200 }); plot.width = 800; plot.height = 200;
  plot.positionTip(plot.pinnedHit);
  assert.equal(plot.tip.style.left, "209px"); assert.equal(plot.tip.style.top, "20px");
  assert.equal(plot.tip.textContent, text, "reprojecting preserves numerical tip contents");
  plot.reset(); assert.equal(plot.pinnedHit, null); assert.equal(plot.tip.hidden, true);
  console.log("PASS pinned 2D data anchor reprojects under resize and reset clears it");
  delete global.figureViewHarness;
})().catch((error) => { console.error(error); process.exit(1); });
