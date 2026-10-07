"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const shapes = require("../frontend/figure_shape_utils.cjs");
const { validate } = require("../frontend/figure_data_utils.cjs");
let count = 0;
function check(name, fn) {
  fn();
  count++;
}
const fixture = (name) => validate(JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json"), "utf8"))).data;
const texts = (runs) => runs.map((run) => [run.text, run.script, run.bold, run.italic]);

check("TeX: Greek letters, symbols and plain text", () => {
  assert.equal(shapes.plainText("teorik: \\sigma = 0.3, \\Omega \\leq \\infty"), "teorik: σ = 0.3, Ω ≤ ∞");
  assert.equal(shapes.plainText("ölçülen: ortalama = 0.50"), "ölçülen: ortalama = 0.50");
  assert.deepEqual(texts(shapes.parseTex("")), []);
});
check("TeX: superscript and subscript, one character or a group", () => {
  assert.deepEqual(texts(shapes.parseTex("x^2_i y^{10} 10^{-3}")), [["x", 0, false, false], ["2", 1, false, false], ["i", -1, false, false],
    [" y", 0, false, false], ["10", 1, false, false], [" 10", 0, false, false], ["-3", 1, false, false]]);
  assert.deepEqual(texts(shapes.parseTex("\\sigma_x^2 e^\\pi")), [["σ", 0, false, false], ["x", -1, false, false], ["2", 1, false, false],
    [" e", 0, false, false], ["π", 1, false, false]]);
});
check("TeX: font switches stay inside their group", () => {
  assert.deepEqual(texts(shapes.parseTex("a {\\bf b} c \\it d")), [["a ", 0, false, false], [" b", 0, true, false], [" c ", 0, false, false], [" d", 0, false, true]]);
  assert.deepEqual(texts(shapes.parseTex("\\bf B \\rm r")), [[" B ", 0, true, false], [" r", 0, false, false]]);
});
check("TeX: escapes, unknown and unsupported markup stay readable", () => {
  assert.equal(shapes.plainText("a\\_b \\{c\\} 50\\% \\\\"), "a_b {c} 50% \\");
  assert.equal(shapes.plainText("\\unknowncmd x"), "\\unknowncmd x");
  assert.equal(shapes.plainText("\\fontsize{14}big \\color{red}red \\color[rgb]{0 .5 0}green \\fontname{Arial}f"), "big red green f");
  assert.equal(shapes.plainText("tail^"), "tail^");
  assert.equal(shapes.plainText("a}b{c"), "abc");
  assert.equal(shapes.plainText("x^😀y"), "x😀y");
  assert.equal(shapes.plainText("{".repeat(40) + "deep"), "{".repeat(24) + "deep");
});
check("TeX: interpreter none is literal", () => {
  assert.deepEqual(texts(shapes.parseTex("\\sigma_x^2", "none")), [["\\sigma_x^2", 0, false, false]]);
});
check("text layout: alignment of the block around its anchor", () => {
  const metrics = { lineHeight: 12, ascent: 8, descent: 2 };
  const left = shapes.layoutText({ ...metrics, widths: [40, 20], halign: "left", valign: "top", margin: 3 });
  assert.deepEqual([left.x, left.y, left.width, left.height], [0, 0, 40, 24]);
  assert.deepEqual(left.box, { x: -3, y: -3, w: 46, h: 30 });
  assert.deepEqual(left.lines, [{ x: 0, baseline: 9 }, { x: 0, baseline: 21 }]);
  const centre = shapes.layoutText({ ...metrics, widths: [40, 20], halign: "center", valign: "middle" });
  assert.deepEqual([centre.x, centre.y], [-20, -12]);
  assert.deepEqual(centre.lines.map((line) => line.x), [-20, -10]);
  const right = shapes.layoutText({ ...metrics, widths: [40, 20], halign: "right", valign: "bottom" });
  assert.deepEqual([right.x, right.y], [-40, -24]);
  assert.deepEqual(right.lines.map((line) => line.x), [-40, -20]);
  assert.equal(right.lines[1].baseline, -3);
  const baseline = shapes.layoutText({ ...metrics, widths: [10], halign: "left", valign: "baseline" });
  assert.equal(baseline.lines[0].baseline, 0);
  assert.equal(shapes.layoutText({ ...metrics, widths: [10], valign: "cap" }).y, 0);
  assert.deepEqual(shapes.layoutText({ ...metrics, widths: [] }).box, { x: 0, y: 0, w: 0, h: 0 });
});
check("faces of a real grouped bar fixture", () => {
  const bar = fixture("bar_grouped").axes[0].series[0];
  const faces = shapes.facePolygons(bar);
  assert.equal(faces.length, 3);
  assert.deepEqual(faces.map((face) => face.index), [0, 1, 2]);
  const rectangle = shapes.faceRectangle(faces[1].points);
  assert.deepEqual([rectangle.y0, rectangle.y1], [0.5, 3]);
  assert(Math.abs((rectangle.x0 + rectangle.x1) / 2 - 20) < 2 && rectangle.x1 < 20, "first series sits left of its category");
  assert.deepEqual(shapes.faceTip(bar, faces[1]), { x: 20, y: 3 });
});
check("stacked horizontal bars report their own value", () => {
  const top = fixture("barh_stacked").axes[0].series.filter((s) => s.kind === "patch2d")[1];
  const faces = shapes.facePolygons(top);
  assert.deepEqual(shapes.faceRectangle(faces[1].points), { x0: 3, x1: 7, y0: 1.6, y1: 2.4 });
  assert.deepEqual(shapes.faceTip(top, faces[1]), { x: 4, y: 2 });
});
check("histogram rectangles report their bin edges", () => {
  const hist = fixture("hist_flat").axes[0].series[0];
  const faces = shapes.facePolygons(hist);
  assert.equal(faces.length, 3);
  const tip = shapes.faceTip(hist, faces[2]);
  assert.equal(tip.y, 3);
  assert(Math.abs(tip.x[0] - 7 / 3) < 1e-12 && tip.x[1] === 3);
  const touching = { ...hist, role: "bar", bar: { horizontal: false, layout: "grouped", width: 1, base_value: 0, positions: [1.3, 2, 2.7], values: [1, 2, 3] } };
  const edges = shapes.faceTip(touching, faces[0]).x;
  assert(edges[0] === 1 && Math.abs(edges[1] - 5 / 3) < 1e-12, "touching bars report edges, not their centre");
  assert.equal(shapes.faceTip(touching, faces[0]).y, 1);
});
check("a face that reads a missing vertex is left out; padding ends a face", () => {
  const patches = fixture("patch_missing_vertex").axes[0].series;
  assert.deepEqual(shapes.facePolygons(patches[0]).map((face) => face.points), [[[0, 0], [1, 0], [1, 1]]]);
  assert.deepEqual(shapes.facePolygons(patches[1]).map((face) => face.index), [0]);
  assert.equal(shapes.faceTip(patches[0], shapes.facePolygons(patches[0])[0]), null);
  const padded = { vertices: { shape: [4, 2], values: [0, 1, 1, 0, 0, 0, 1, 1] }, faces: { shape: [2, 4], values: [0, 0, 1, 1, 2, 2, 3, null] } };
  assert.deepEqual(shapes.facePolygons(padded).map((face) => face.points.length), [4, 3]);
  const missingBar = { ...padded, bar: { horizontal: false, width: .8, positions: [1, 2], values: [null, 2] } };
  assert.equal(shapes.faceTip(missingBar, shapes.facePolygons(padded)[0]), null);
});
check("point in polygon uses the even-odd rule", () => {
  const square = [[0, 0], [4, 0], [4, 4], [0, 4]];
  assert.equal(shapes.pointInPolygon(square, 2, 2), true);
  assert.equal(shapes.pointInPolygon(square, 5, 2), false);
  assert.equal(shapes.pointInPolygon(square, 2, -1), false);
  const bow = [[0, 0], [4, 4], [4, 0], [0, 4]];
  assert.equal(shapes.pointInPolygon(bow, 1, 2), true);
  assert.equal(shapes.pointInPolygon(bow, 2, 1), false);
  assert.equal(shapes.faceRectangle([[0, 0], [4, 1], [4, 4], [0, 4]]), null);
  assert.equal(shapes.faceRectangle(bow), null);
  assert.equal(shapes.faceRectangle([[0, 0], [1, 0], [1, 1]]), null);
});
check("nice ticks", () => {
  assert.deepEqual(shapes.niceTicks(0, 3), [0, 0.5, 1, 1.5, 2, 2.5, 3]);
  assert.deepEqual(shapes.niceTicks(0.5, 3.5), [0.5, 1, 1.5, 2, 2.5, 3, 3.5]);
  assert.deepEqual(shapes.niceTicks(-1, 1, 4), [-1, -0.5, 0, 0.5, 1]);
  assert.deepEqual(shapes.niceTicks(0, 1000), [0, 200, 400, 600, 800, 1000]);
  assert.deepEqual(shapes.niceTicks(0.11, 0.19, 4), [0.12, 0.14, 0.16, 0.18]);
  assert.deepEqual(shapes.niceTicks(3, 0), shapes.niceTicks(0, 3));
  assert.deepEqual(shapes.niceTicks(1, 1), []);
  assert.deepEqual(shapes.niceTicks(0, Infinity), []);
});
check("axis ticks: Octave's own at the exported limits, sensible after a zoom", () => {
  const axis = { xtick: [1, 2, 3], xticklabel: ["a", "b", "c"], xscale: "linear", xtickmode: "manual", xticklabelmode: "manual",
    ytick: [0, 1, 2, 3], yticklabel: ["0", "1", "2", "3"], yscale: "linear", ytickmode: "auto", yticklabelmode: "auto" };
  assert.deepEqual(shapes.axisTicks(axis, "x", [0.5, 3.5], [0.5, 3.5]), [{ value: 1, label: "a" }, { value: 2, label: "b" }, { value: 3, label: "c" }]);
  // Category labels stay on their own ticks, also when only one is left in view.
  assert.deepEqual(shapes.axisTicks(axis, "x", [1.5, 2.5], [0.5, 3.5]), [{ value: 2, label: "b" }]);
  assert.deepEqual(shapes.axisTicks(axis, "y", [0, 3], [0, 3]).map((tick) => tick.label), ["0", "1", "2", "3"]);
  assert.deepEqual(shapes.axisTicks(axis, "y", [0, 0.4], [0, 3]), [0, 0.1, 0.2, 0.3, 0.4].map((value) => ({ value, label: null })));
  const manual = { ...axis, xticklabelmode: "auto", xticklabel: ["1", "2", "3"] };
  assert.deepEqual(shapes.axisTicks(manual, "x", [0.9, 2.1], [0.5, 3.5]).map((tick) => tick.value), [1, 2]);
  assert.deepEqual(shapes.axisTicks(manual, "x", [1.9, 2.1], [0.5, 3.5]).map((tick) => tick.value), [1.9, 1.95, 2, 2.05, 2.1]);
  assert.equal(shapes.axisTicks({ xscale: "linear" }, "x", [0, 1], [0, 1]), null, "older artifacts keep the viewer's own ticks");
  const log = { xtick: [1, 10, 100], xticklabel: ["10^{0}", "10^{1}", "10^{2}"], xscale: "log", xtickmode: "auto", xticklabelmode: "auto" };
  assert.deepEqual(shapes.axisTicks(log, "x", [1, 100], [1, 100]).map((tick) => tick.label), ["10^{0}", "10^{1}", "10^{2}"]);
  assert.equal(shapes.axisTicks(log, "x", [2, 50], [1, 100]), null);
  assert.deepEqual(shapes.axisTicks({ ...axis, xticklabel: [] }, "x", [0.5, 3.5], [0.5, 3.5]).map((tick) => tick.label), ["", "", ""]);
});
console.log(`FIGURE SHAPE UNIT PASS: ${count} cases (TeX subset, text layout, faces, tips, ticks).`);
