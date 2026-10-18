"use strict";
// Scalar transparency of 2D patches and scatter: validator admission and the
// 2D canvas drawing (separate face/edge alpha, legend swatches, hit testing).
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
const { validate } = require("../frontend/figure_data_utils.cjs");

const load = (name) => JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json"), "utf8"));
const copy = (v) => JSON.parse(JSON.stringify(v));
const constant = (value) => ({ mode: "constant", association: "constant", value });
let count = 0;
function check(name, fn) { fn(); count++; }

const barPayload = load("bar_grouped");
const linePayload = load("line2d");
const scatterPayload = copy(linePayload);
{
  const s = scatterPayload.axes[0].series[0];
  s.kind = "scatter"; s.line_style = "none"; s.sizes = [36]; s.size_units = "points-squared";
}
assert.equal(validate(scatterPayload).ok, true, JSON.stringify(validate(scatterPayload)));
assert.equal(validate(barPayload).ok, true);

function accepts(name, payload, mutate) {
  check("accept " + name, () => {
    const input = copy(payload);
    mutate(input);
    const result = validate(input);
    assert.equal(result.ok, true, JSON.stringify(result));
  });
}
function rejects(name, payload, mutate, code) {
  check("reject " + name, () => {
    const input = copy(payload);
    mutate(input);
    const result = validate(input);
    assert.equal(result.ok, false, name);
    if (code) assert.equal(result.reason_code, code, JSON.stringify(result));
  });
}
const patchOf = (p) => p.axes[0].series.find((s) => s.kind === "patch2d");
const first = (p) => p.axes[0].series[0];

accepts("patch2d face alpha", barPayload, (p) => { patchOf(p).face_alpha = constant(.5); });
accepts("patch2d face and edge alpha, zero allowed", barPayload, (p) => { patchOf(p).face_alpha = constant(0); patchOf(p).edge_alpha = constant(.25); });
accepts("scatter marker alphas", scatterPayload, (p) => { first(p).marker_face_alpha = constant(.4); first(p).marker_edge_alpha = constant(.6); });

rejects("patch2d alpha out of range", barPayload, (p) => { patchOf(p).face_alpha = constant(1.5); }, "transparency");
rejects("patch2d negative alpha", barPayload, (p) => { patchOf(p).edge_alpha = constant(-.1); }, "transparency");
rejects("patch2d NaN-like alpha", barPayload, (p) => { patchOf(p).face_alpha = constant(null); }, "transparency");
rejects("patch2d explicit alpha 1 (absent by contract)", barPayload, (p) => { patchOf(p).face_alpha = constant(1); });
rejects("patch2d data-driven alpha", barPayload, (p) => { patchOf(p).face_alpha = { mode: "flat", association: "vertex", value: .5 }; }, "transparency");
rejects("patch2d bare number", barPayload, (p) => { patchOf(p).face_alpha = .5; }, "transparency");
rejects("patch2d null descriptor", barPayload, (p) => { patchOf(p).face_alpha = null; }, "transparency");
rejects("patch2d carrying marker alpha", barPayload, (p) => { patchOf(p).marker_face_alpha = constant(.5); });
rejects("textbox patch carrying alpha", barPayload, (p) => { Object.assign(patchOf(p), { role: "textbox", face_alpha: constant(.5) }); });
rejects("line carrying face alpha", linePayload, (p) => { first(p).face_alpha = constant(.5); });
rejects("line carrying marker alpha", linePayload, (p) => { first(p).marker_face_alpha = constant(.5); });
rejects("scatter carrying face alpha", scatterPayload, (p) => { first(p).face_alpha = constant(.5); });
rejects("scatter marker alpha out of range", scatterPayload, (p) => { first(p).marker_face_alpha = constant(2); }, "transparency");
rejects("scatter marker alpha data-driven", scatterPayload, (p) => { first(p).marker_edge_alpha = { mode: "flat", association: "constant", value: .5 }; }, "transparency");
rejects("3D scatter carrying marker alpha", load("scatter3_sizes"), (p) => { first(p).marker_face_alpha = constant(.5); });

// Drawing: a recording canvas context fed to the real drawPatch/marker/legend.
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
  const loaded = new Module(path.resolve("frontend/alpha2d_test.js"), module);
  loaded._compile(build.outputFiles[0].text, path.resolve("frontend/alpha2d_test.js"));
  const proto = registry.InteractiveFigure.prototype;

  // Records every fill/stroke with the globalAlpha and style active at that moment.
  function recorder() {
    const calls = [];
    const state = { globalAlpha: 1 }, stack = [];
    const ctx = new Proxy(state, {
      get(obj, key) {
        if (key in obj) return obj[key];
        if (key === "measureText") return () => ({ width: 10 });
        if (key === "save") return () => stack.push(obj.globalAlpha);
        if (key === "restore") return () => { obj.globalAlpha = stack.pop(); };
        if (key === "fill" || key === "fillRect") return () => calls.push({ op: "fill", alpha: obj.globalAlpha, style: obj.fillStyle });
        if (key === "stroke" || key === "strokeRect") return () => calls.push({ op: "stroke", alpha: obj.globalAlpha, style: obj.strokeStyle });
        return () => {};
      },
      set(obj, key, value) { obj[key] = value; return true; }
    });
    return { ctx, calls, state };
  }
  const palette = { background: "#ffffff", text: "#000000" };
  const view = (series, ctx) => Object.assign(Object.create(proto), {
    data: { axes: [{ series, visible: true, xlim: [0, 4], ylim: [0, 4], xscale: "linear", yscale: "linear", xdir: "normal", ydir: "normal", legend: { visible: true, labels: ["a"] } }] },
    box: () => ({ x: 0, y: 0, w: 400, h: 400 }), dataPixel: (v) => v * 100, pixelData: (v) => v / 100, hits: [], faces: [], ctx, palette, measure: () => 0
  });
  const triangle = (extra = {}) => ({ kind: "patch2d", role: "patch", face_color: [1, 0, 0], edge_color: [0, 0, 1], line_style: "-", line_width: 1, display_name: "a",
    vertices: { shape: [3, 2], values: [.5, 1.5, 1, .5, .5, 1.5] }, faces: { shape: [1, 3], values: [0, 1, 2] }, index_base: 0, ...extra });

  check("patch face and edge use separate alpha, reset afterwards", () => {
    const { ctx, calls, state } = recorder();
    view([], ctx).drawPatch(triangle({ face_alpha: constant(.5), edge_alpha: constant(.3) }), 0, 0);
    const fill = calls.find((c) => c.op === "fill"), stroke = calls.find((c) => c.op === "stroke");
    assert.equal(fill.alpha, .5);
    assert.equal(stroke.alpha, .3);
    assert.equal(state.globalAlpha, 1, "globalAlpha must be restored");
  });
  check("patch without alpha keys draws at 1", () => {
    const { ctx, calls } = recorder();
    view([], ctx).drawPatch(triangle(), 0, 0);
    assert.deepEqual(calls.map((c) => c.alpha), [1, 1]);
  });
  check("face alpha leaves the edge opaque", () => {
    const { ctx, calls } = recorder();
    view([], ctx).drawPatch(triangle({ face_alpha: constant(.2) }), 0, 0);
    assert.equal(calls.find((c) => c.op === "stroke").alpha, 1);
  });
  check("alpha 0 face still records the polygon for occlusion/tips (unchanged behaviour)", () => {
    const { ctx, calls } = recorder();
    const plot = view([], ctx);
    plot.drawPatch(triangle({ face_alpha: constant(0) }), 0, 0);
    assert.equal(calls.find((c) => c.op === "fill").alpha, 0);
    assert.equal(plot.faces.length, 1);
  });
  check("scatter marker face/edge alpha", () => {
    const { ctx, calls, state } = recorder();
    const s = { kind: "scatter", marker: "o", marker_face_alpha: constant(.4), marker_edge_alpha: constant(.6), marker_edge_color: [0, 0, 1], marker_face_color: [0, 0, 1], marker_face_auto: false, line_style: "none", line_color: "none", x: [1], y: [1] };
    const plot = view([s], ctx);
    plot.drawSeries(s, plot.data.axes[0], 0, 0);
    assert.equal(calls.find((c) => c.op === "fill").alpha, .4);
    assert.equal(calls.find((c) => c.op === "stroke").alpha, .6);
    assert.equal(state.globalAlpha, 1);
  });
  check("dot marker takes the edge alpha", () => {
    const { ctx, calls } = recorder();
    const s = { kind: "scatter", marker: ".", marker_edge_alpha: constant(.5), marker_edge_color: [0, 0, 1], marker_face_color: "none", marker_face_auto: false, line_style: "none", line_color: "none", x: [1], y: [1] };
    const plot = view([s], ctx);
    plot.drawSeries(s, plot.data.axes[0], 0, 0);
    assert.equal(calls.find((c) => c.op === "fill").alpha, .5);
  });
  check("legend swatches carry the object alpha", () => {
    const { ctx, calls, state } = recorder();
    const s = triangle({ face_alpha: constant(.5), edge_alpha: constant(.3) });
    const plot = view([s], ctx);
    plot.drawRuns = () => {};
    plot.drawLegend(plot.data.axes[0], 0);
    const swatch = calls.filter((c) => c.style && String(c.style).startsWith("rgb(255"));
    assert.equal(swatch.find((c) => c.op === "fill").alpha, .5);
    assert.equal(calls.filter((c) => c.op === "stroke").some((c) => c.alpha === .3), true);
    assert.equal(state.globalAlpha, 1);
  });
  check("legend scatter swatch carries marker alpha", () => {
    const { ctx, calls } = recorder();
    const s = { kind: "scatter", display_name: "a", marker: "o", marker_face_alpha: constant(.4), marker_edge_color: [0, 0, 1], marker_face_color: [0, 0, 1], marker_face_auto: false, line_style: "none", line_color: "none", x: [1], y: [1] };
    const plot = view([s], ctx);
    plot.drawRuns = () => {};
    plot.drawLegend(plot.data.axes[0], 0);
    assert.equal(calls.some((c) => c.op === "fill" && c.alpha === .4), true);
  });
  console.log(`PASS figure alpha2d unit: ${count} checks`);
})().catch((e) => { console.error(e); process.exit(1); });
