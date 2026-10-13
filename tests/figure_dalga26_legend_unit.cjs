"use strict";

const assert = require("node:assert/strict");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");

 (async () => {
const filename = path.resolve("frontend/figure3d.js");
const build = await esbuild.build({ entryPoints: [filename], bundle: true, write: false, platform: "node", format: "cjs",
  plugins: [{ name: "legend-boundaries", setup(build) {
    build.onResolve({ filter: /^\.\/(?:figure_webgl|figure_tools|i18n)\.js$/ }, (args) => ({ path: path.basename(args.path), namespace: "boundary" }));
    build.onLoad({ filter: /.*/, namespace: "boundary" }, (args) => ({ contents: {
      "figure_webgl.js": "export class FigureWebGL {}",
      "figure_tools.js": "export const figureCanvasDescription=()=>''; export const mountFigureTools=()=>({}); export const unmountFigureTools=()=>{};",
      "i18n.js": "export const t=(key)=>key; export const onLanguageChange=()=>()=>{};"
    }[args.path], loader: "js" }));
  } }] });
const loaded = new Module(filename, module);
loaded._compile(build.outputFiles[0].text, filename);
const { Figure3D } = loaded.exports;

function context() {
  const events = { texts: [], fills: [], strokes: [], lines: [], rectStrokes: 0 };
  const ctx = { fillStyle: "", strokeStyle: "", lineWidth: 1, textAlign: "", font: "", dash: [],
    measureText(text) { return { width: String(text).length * 6 }; },
    save() {}, restore() {}, fillRect(...args) { events.fills.push({ color: this.fillStyle, args }); },
    strokeRect(...args) { events.rectStrokes++; }, fillText(text) { events.texts.push(text); },
    setLineDash(value) { this.dash = value; }, beginPath() {}, moveTo(...point) { events.lines.push(["M", point]); },
    lineTo(...point) { events.lines.push(["L", point]); }, stroke() { events.strokes.push({ color: this.strokeStyle, lines: events.lines.splice(0) }); } };
  return { ctx, events };
}

const d = (shape, values) => ({ shape, values, order: "column-major" });
const contour = (id, group, displayName = "levels", role = "contour") => ({ id, kind: "line", role, contour_group: group, level: id + 1,
  display_name: displayName, line_color: [0.2 + id * 0.01, 0.7, 0.3], line_style: "-", line_width: 1,
  marker: "none", marker_edge_color: "none", marker_face_color: "none", marker_face_auto: false,
  contour_color: { mode: "constant", association: "constant", rgb: [0.2, 0.7, 0.3] } });
const patch = { id: 5, kind: "patch3d", role: "patch", display_name: "red face", line_style: "-", line_width: 1,
  faces: d([1, 3], [0, 1, 2]), triangle_indices: d([1, 3], [0, 1, 2]), triangle_faces: [0],
  edge_indices: d([3, 3], [0, 1, 2, 1, 2, 0, 0, 0, 0]),
  face_color: { mode: "constant", association: "constant", rgb: [1, 0, 0] },
  edge_color: { mode: "none", association: "constant" }, marker: "none" };
const filledContour = (id, group, displayName = "levels") => ({ ...structuredClone(patch), id, kind: "patch3d", role: "contourf",
  contour_group: group, level: id, display_name: displayName, face_color: { mode: "constant", association: "constant", rgb: [0.4, 0.5, 0.8] } });
const axes = { series: [contour(0, 0), contour(1, 0), contour(2, 0), contour(3, 1), contour(4, 1), patch],
  legend: { visible: true, location: "northeast", labels: ["Group A", "Group B", "Patch face"] },
  colormap: d([1, 3], [1, 0, 0]), clim: [0, 1] };
const palette = { background: [0, 0, 0], text: [1, 1, 1], axis: [0.3, 0.3, 0.3] };
function paint(series = axes.series, labels = axes.legend.labels) {
  const { ctx, events } = context();
  Figure3D.prototype.drawLegend.call({ ctx, legendMarker() {} }, { ...axes, series, legend: { ...axes.legend, labels } },
    { x: 10, y: 10, width: 300, height: 200 }, palette);
  return events;
}

const failures = [];
function expects(name, fn) {
  try { fn(); console.log(`PASS ${name}`); }
  catch (error) { failures.push(`${name}: ${error.message}`); console.log(`REPRODUCED ${name}: ${error.message}`); }
}

// Both checks are run before any product fix so the baseline failure is clear.
expects("contour paths collapse to one entry per contour_group and explicit labels index groups", () => {
  assert.deepEqual(paint().texts, ["Group A", "Group B", "Patch face"]);
});
expects("same-name unlabelled groups remain separate and a blank first group keeps its label index", () => {
  assert.deepEqual(paint([contour(0, 0), contour(1, 0), contour(2, 1), contour(3, 1)], []).texts, ["levels", "levels"]);
  assert.deepEqual(paint([contour(0, 0, ""), contour(1, 0, ""), contour(2, 1, "second")], ["", "Explicit second"]).texts,
    ["Explicit second"]);
});
expects("filled contour paths collapse by contour_group", () => {
  assert.deepEqual(paint([filledContour(0, 0), filledContour(1, 0), filledContour(2, 0), filledContour(3, 1)], []).texts,
    ["levels", "levels"]);
});
expects("patch legend uses a face swatch and draws no edge when edge_color is none", () => {
  const events = paint();
  assert.equal(events.fills.filter((item) => item.color === "rgb(255,0,0)").length, 1);
  assert.equal(events.strokes.length, 2, "only two contour line swatches should be stroked");
  assert.equal(events.rectStrokes, 1, "edgecolor none must not draw a patch outline");
});
expects("patch legend resolves flat face and interpolated first-vertex owners", () => {
  const flat = structuredClone(patch);
  Object.assign(flat, { id: 6, faces: d([2, 3], [0, 1, 2, 1, 2, 0]), triangle_faces: [1], display_name: "flat" });
  flat.face_color = { mode: "flat", association: "face", encoding: "truecolor", mapping: "direct", cdata_class: "double",
    data: d([2, 1, 3], [1, 0, 0, 0, 0, 1]) };
  const interp = structuredClone(patch);
  Object.assign(interp, { id: 7, faces: d([1, 3], [2, 0, 1]), triangle_indices: d([1, 3], [0, 1, 2]), display_name: "interp" });
  interp.face_color = { mode: "interp", association: "vertex", encoding: "truecolor", mapping: "direct", cdata_class: "double",
    data: d([3, 1, 3], [1, 0, 0, 0, 0, 1, 0, 1, 0]) };
  const events = paint([flat, interp], ["flat face", "interp vertex"]);
  assert.deepEqual(events.fills.map((item) => item.color), ["rgb(0,0,0)", "rgb(0,0,255)", "rgb(0,255,0)"]);
});
expects("patch legend resolves flat edge-face and interpolated edge-vertex owners", () => {
  const flat = structuredClone(patch);
  Object.assign(flat, { id: 8, display_name: "flat edge", face_color: { mode: "none", association: "constant" } });
  flat.edge_color = { mode: "flat", association: "face", encoding: "truecolor", mapping: "direct", cdata_class: "double",
    data: d([2, 1, 3], [0, 1, 1, 0, 0, 0]) };
  const interp = structuredClone(patch);
  Object.assign(interp, { id: 9, display_name: "interp edge", face_color: { mode: "none", association: "constant" } });
  interp.edge_color = { mode: "interp", association: "vertex", encoding: "truecolor", mapping: "direct", cdata_class: "double",
    data: d([3, 1, 3], [1, 1, 0, 1, 0, 0, 0, 0, 1]) };
  const events = paint([flat, interp], ["flat edge", "interp edge"]);
  assert.deepEqual(events.strokes.map((stroke) => stroke.color), ["rgb(0,255,0)", "rgb(255,255,0)"]);
});
if (failures.length) {
  console.error(`${failures.length} legend regression(s) reproduced before fix`);
  process.exitCode = 1;
} else console.log("FIGURE DALGA26 LEGEND PASS: group ownership, explicit labels and patch face swatches");
})().catch((error) => { console.error(error); process.exitCode = 1; });
