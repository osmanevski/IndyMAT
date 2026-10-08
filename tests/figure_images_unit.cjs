"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const data = require("../frontend/figure_data_utils.cjs");
const images = require("../frontend/figure_image_utils.cjs");
const load = (name) => JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_images", name + ".json"), "utf8"));
const copy = (v) => JSON.parse(JSON.stringify(v));
for (const name of ["scaled", "rgb", "direct"]) {
  const fixture = load(name), before = JSON.stringify(fixture);
  assert.equal(data.validate(fixture).ok, true, JSON.stringify(data.validate(fixture)));
  assert.equal(JSON.stringify(fixture), before);
}
const rgb = load("rgb"), s = rgb.axes[0].series[0];
assert.deepEqual([...images.rgba(s, rgb.axes[0])], [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]);
assert.deepEqual(images.bounds([30, 10], 3), [35, 5]);
assert.deepEqual(images.bounds([1, 1], 1), [.5, 1.5]);
assert.deepEqual(images.pick(s, 1, 2), { row: 1, column: 0, value: [0, 0, 255] });
assert.equal(images.pick(s, .49, 1), null);
assert.equal(images.pick(s, 2.5, 1), null);
const scaled = load("scaled"), a = scaled.axes[0], image = a.series[0];
assert.deepEqual([...images.rgba(image, a)], [255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255]);
assert.deepEqual(images.pick(image, 30, 8), { row: 0, column: 0, value: 1 });
assert.deepEqual(images.pick(image, 10, 4), { row: 1, column: 2, value: 6 });
const direct = load("direct"), d = direct.axes[0].series[0];
assert.deepEqual([...images.rgba(d, direct.axes[0])], [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255]);
d.cdata_class = "double";
assert.deepEqual([...images.rgba(d, direct.axes[0])], [255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
// Native floating truecolor clips finite values; integer bytes divide by class.
const clamp = copy(s);clamp.cdata_class = "single";clamp.cdata.values = [-1, 0, 2, 1, 0, 2, -1, 1, 0, 1, 0, 2];
assert.deepEqual([...images.rgba(clamp, rgb.axes[0])].slice(0, 8), [0, 0, 0, 255, 255, 0, 0, 255]);
const box = images.aspectBox({ x: 0, y: 0, w: 600, h: 300 }, a, { x: [5, 35], y: [2, 10] });
assert.equal(box.w / box.h, 30 / 8);
assert.equal(box.x + box.w / 2, 300);assert.equal(box.y + box.h / 2, 150);
let count = 0;
function invalid(mutate, code = "invalid_data") {
  const f = load("rgb");mutate(f, f.axes[0].series[0]);
  const result = data.validate(f);assert.equal(result.ok, false, JSON.stringify(result));assert.equal(result.reason_code, code, JSON.stringify(result));count++;
}
invalid((f, s) => { s.cdata.values[0] = null; });
invalid((f, s) => { s.cdata.values[0] = 256; });
invalid((f, s) => { s.cdata.values[0] = .5; });
invalid((f, s) => { s.cdata_class = "int16"; });
invalid((f, s) => { s.cdata.shape = [2, 2]; });
invalid((f, s) => { s.shape = [2, 3]; });
invalid((f, s) => { s.shape = [513, 512]; }, "budget_exceeded");
invalid((f, s) => { s.pixel_count = 3; });
invalid((f, s) => { f.pixel_count = 3; });
invalid((f, s) => { s.x = [1, 1]; });
invalid((f, s) => { s.x = [-1e308, 1e308]; });
invalid((f, s) => { s.encoding = "indexed"; });
invalid((f, s) => { f.limits.image_pixels = 262145; });
invalid((f, s) => { f.version = 2; }, "unsupported_object");
invalid((f, s) => { f.axes[0].xscale = "log"; }, "unsupported_object");
for (const size of [[0, 20], [-1, 20], [20], [null, 20], ["20", 20]]) invalid((f) => { f.source.figure_size = size; });
const size = load("rgb");size.source.figure_size = [640, 320];assert.equal(data.validate(size).ok, true);
console.log(`${count} image rejection cases; measured raster colors, orientation, extents, picking and aspect passed.`);
