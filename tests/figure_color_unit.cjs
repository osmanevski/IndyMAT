"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { validate } = require("../frontend/figure_data_utils.cjs");
const C = require("../frontend/figure_color_utils.cjs");
let count = 0;
function check(name, fn) {
  fn();
  count++;
  console.log("PASS " + name);
}
const rgb = [[1, 0, 0], [0, 1, 0], [0, 0, 1], [1, 1, 1]];
const axes = { clim: [0, 1], colormap: { shape: [4, 3], order: "column-major", values: [1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1] } };
function indexed(value, mapping = "scaled", cdata_class = "double") {
  return { mode: "flat", association: "cell", encoding: "indexed", mapping, cdata_class,
    data: { shape: [1, 1], order: "column-major", values: [value] } };
}
for (const [value, index] of [[-10, 0], [0, 0], [0.249999, 0], [0.25, 1], [0.5, 2], [0.75, 3], [1, 3], [100, 3]]) {
  check("scaled boundary " + value, () => assert.deepEqual(C.descriptorColor(indexed(value), 0, axes), rgb[index]));
}
check("nonzero CLim", () => {
  assert.deepEqual(C.descriptorColor(indexed(12), 0, { ...axes, clim: [10, 14] }), rgb[2]);
});
for (const name of ["double", "single"]) {
  check("floating direct fix, 1-based, clipping " + name, () => {
    for (const [value, index] of [[-2.9, 0], [0, 0], [1.9, 0], [2.9, 1], [3, 2], [4.1, 3], [999, 3]]) {
      assert.deepEqual(C.descriptorColor(indexed(value, "direct", name), 0, axes), rgb[index]);
    }
  });
}
for (const name of ["int8", "uint8", "int16", "uint16", "int32", "uint32", "int64", "uint64"]) {
  check("integer direct zero-based and scaled " + name, () => {
    for (const [value, index] of [[0, 0], [1, 1], [2, 2], [3, 3], [200, 3]]) assert.deepEqual(C.descriptorColor(indexed(value, "direct", name), 0, axes), rgb[index]);
    if (name.startsWith("int")) assert.deepEqual(C.descriptorColor(indexed(-1, "direct", name), 0, axes), rgb[0]);
    assert.deepEqual(C.descriptorColor(indexed(1, "scaled", name), 0, { ...axes, clim: [0, 4] }), rgb[1]);
  });
}
for (const [name, divisor] of [["double", 1], ["single", 1], ["uint8", 255], ["uint16", 65535]]) {
  check("truecolor planar bypasses mapping " + name, () => {
    const descriptor = { mode: "flat", association: "cell", encoding: "truecolor", cdata_class: name, mapping: "direct",
      data: { shape: [2, 2, 3], order: "column-major", values: [divisor, 0, 0, 0, 0, divisor, 0, 0, 0, 0, divisor, divisor] } };
    assert.deepEqual(C.descriptorColor(descriptor, 0, axes), [1, 0, 0]);
    assert.deepEqual(C.descriptorColor(descriptor, 1, axes), [0, 1, 0]);
    assert.deepEqual(C.descriptorColor(descriptor, 3, axes), [0, 0, 1]);
  });
}
check("constant and none", () => {
  assert.deepEqual(C.descriptorColor({ mode: "constant", rgb: [0.2, 0.4, 0.6] }, 99, axes), [0.2, 0.4, 0.6]);
  assert.equal(C.descriptorColor({ mode: "none" }, 99, axes), null);
});
check("contrast 3:1 rule, clamping and none", () => {
  assert.equal(C.contrast([0, 0, 0], [1, 1, 1]), 21);
  assert.equal(C.contrast([1, 1, 1], [0, 0, 0]), 21);
  assert.equal(C.contrast([0, 0, 0], [0, 0, 0]), 1);
  assert.deepEqual(C.adjustForContrast([0.05, 0.05, 0.05], [0, 0, 0], [1, 1, 1]), [1, 1, 1]);
  assert.deepEqual(C.adjustForContrast([1, 0, 0], [0, 0, 0], [1, 1, 1]), [1, 0, 0]);
  assert.deepEqual(C.adjustForContrast([2, -1, 0], [0, 0, 0], [1, 1, 1]), [1, 0, 0]);
  assert.equal(C.adjustForContrast("none", [0, 0, 0], [1, 1, 1]), "none");
  const value = 1.055 * 0.1 ** (1 / 2.4) - 0.055;
  assert.ok(Math.abs(C.contrast([value, value, value], [0, 0, 0]) - 3) < 1e-14);
  assert.deepEqual(C.adjustForContrast([value + 1e-10, value + 1e-10, value + 1e-10], [0, 0, 0], [1, 1, 1]), [value + 1e-10, value + 1e-10, value + 1e-10]);
});
check("colorbar exact discrete stops and reverse", () => {
  const stops = C.colorbarGradient(axes);
  assert.equal(stops.length, 8);
  assert.deepEqual(stops.map((s) => s.offset), [0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1]);
  assert.deepEqual(stops.map((s) => s.rgb), rgb.flatMap((c) => [c, c]));
  assert.deepEqual(C.colorbarGradient({ ...axes, direction: "reverse" }).map((s) => s.rgb), rgb.slice().reverse().flatMap((c) => [c, c]));
  const single = C.colorbarGradient({ colormap: { shape: [1, 3], values: [0.2, 0.4, 0.6] } });
  assert.deepEqual(single, [{ offset: 0, rgb: [0.2, 0.4, 0.6] }, { offset: 1, rgb: [0.2, 0.4, 0.6] }]);
});
for (const file of fs.readdirSync(path.join(__dirname, "fixtures/figure_v3")).filter((f) => f.endsWith(".json"))) {
  const result = validate(JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", file))));
  assert.equal(result.ok, true);
  for (const a of result.data.axes) {
    for (const s of a.series.filter((s) => s.kind === "surface")) check("fixture face/edge owners " + file, () => {
      const resolved = C.resolveSurfaceColors(s, a);
      assert.ok(resolved.faceColors instanceof Float32Array);
      assert.ok(resolved.edgeColors instanceof Float32Array);
      const expected = (descriptor, owner) => {
        if (descriptor.mode === "constant") return descriptor.rgb;
        const value = s.cdata.values[owner];
        const index = Math.max(0, Math.min(a.colormap.shape[0] - 1, descriptor.mapping === "scaled" ? Math.floor(a.colormap.shape[0] * (value - a.clim[0]) / (a.clim[1] - a.clim[0])) : /int/.test(s.cdata_class) ? value : Math.trunc(value) - 1));
        return [0, 1, 2].map((channel) => a.colormap.values[index + channel * a.colormap.shape[0]]);
      };
      assert.deepEqual(Array.from(resolved.faceColors), Array.from(new Float32Array(s.cell_origins.flatMap((owner) => expected(s.face_color, owner)))));
      const edges = s.edge_indices.shape[0];
      assert.deepEqual(Array.from(resolved.edgeColors), Array.from(new Float32Array(s.edge_indices.values.slice(0, edges).flatMap((owner) => expected(s.edge_color, owner)))));
      if (file === "mesh_default.json") assert.ok(resolved.faceColors.every((v) => v === 1));
      assert.equal(C.rawColorValue(s, 0), s.cdata.values[0]);
    });
    for (const bar of a.colorbars || []) check("fixture colorbar exact map " + file, () => {
      const stops = C.colorbarGradient(bar);
      assert.equal(stops.length, bar.colormap.shape[0] * 2);
      assert.deepEqual(stops[0].rgb, [0, 1, 2].map((c) => bar.colormap.values[c * bar.colormap.shape[0]]));
    });
  }
}
check("extreme finite CLim still maps the midpoint and clips", () => {
  const a = { ...axes, clim: [-1e308, 1e308] };
  assert.deepEqual(C.descriptorColor(indexed(0), 0, a), rgb[2]);
  assert.deepEqual(C.descriptorColor(indexed(-1e308), 0, a), rgb[0]);
  assert.deepEqual(C.descriptorColor(indexed(1e308), 0, a), rgb[3]);
});
console.log(`${count} colour cases passed.`);
