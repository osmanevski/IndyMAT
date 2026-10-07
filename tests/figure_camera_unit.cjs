"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { validate } = require("../frontend/figure_data_utils.cjs");
const C = require("../frontend/figure_camera_utils.cjs");
let count = 0;
function check(name, fn) {
  fn();
  count++;
  console.log("PASS " + name);
}
function close(a, b, tolerance = 1e-12) {
  assert.ok(Math.abs(a - b) <= tolerance, `${a} != ${b}`);
}
function vector(a, b, tolerance) {
  assert.equal(a.length, b.length);
  a.forEach((v, i) => close(v, b[i], tolerance));
}
function fixture(name) {
  const result = validate(JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json"))));
  assert.equal(result.ok, true);
  assert.equal(result.supported, true);
  return result.data.axes[0];
}
const axes = fixture("plot3_gap");
const vp = { width: 800, height: 600 };
check("default 3D basis", () => {
  const camera = C.createCamera(axes);
  vector(C.basis(camera).right, [0.7933533402912352, -0.6087614290087207, 0]);
  vector(C.basis(camera).up, [0.3043807145043603, 0.3966766701456175, 0.8660254037844387]);
  vector(C.basis(camera).back, [-0.5272028623656693, -0.6870641468694502, 0.5]);
  vector(camera.extent, [1, 1, 1]);
  vector(camera.target, [0, 0, 0]);
});
check("top view +X right +Y up and exact Z depth", () => {
  const camera = C.createCamera(fixture("line2d"));
  vector(C.basis(camera).right, [1, 0, 0]);
  vector(C.basis(camera).up, [0, 1, 0]);
  vector(C.basis(camera).back, [0, 0, 1]);
  const origin = C.project(camera, vp, [0, 0, 0]);
  assert.deepEqual(origin, { x: 400, y: 300, depth: 0.5 });
  assert.ok(C.project(camera, vp, [1, 0, 0]).x > origin.x);
  assert.ok(C.project(camera, vp, [0, 1, 0]).y < origin.y);
  assert.ok(C.project(camera, vp, [0, 0, 1]).depth < origin.depth);
});
check("reversed axes and manual data aspect", () => {
  const a = fixture("reversed_view");
  vector(C.dataToNormalised(a, [1, 4, 7]), [1, 0.5, 1 / 3]);
  vector(C.dataToNormalised(a, [3, 6, 9]), [-1, -0.5, -1 / 3]);
  const camera = C.createCamera(a);
  const p = C.dataToNormalised(a, a.camera.position);
  const b = C.basis(camera).back;
  vector(p.map((v) => v / Math.hypot(...p)), b);
  assert.ok(b[2] > 0);
});
check("manual plot box aspect and DAR precedence", () => {
  const a = structuredClone(axes);
  a.plot_box_aspect_ratio = [2, 4, 1];
  a.plot_box_aspect_ratio_mode = "manual";
  vector(C.extents(a), [0.5, 1, 0.25]);
  a.data_aspect_ratio_mode = "manual";
  vector(C.extents(a), [1, 1, 1]);
});
for (const name of ["plot3_gap", "line2d", "reversed_view", "surf_matrix_nan"]) {
  check("data inverse " + name, () => {
    const a = fixture(name);
    const point = [a.xlim[0] * 0.7 + a.xlim[1] * 0.3, a.ylim[0] * 0.2 + a.ylim[1] * 0.8, a.zlim[0] * 0.6 + a.zlim[1] * 0.4];
    vector(C.normalisedToData(a, C.dataToNormalised(a, point)), point);
  });
  check("projection/ray/matrix round trip " + name, () => {
    const camera = C.createCamera(fixture(name));
    for (const viewport of [vp, { x: 11, y: 23, width: 300, height: 900 }]) {
      const m = C.matrices(camera, viewport.width, viewport.height);
      for (const key of ["view", "projection", "viewProjection"]) {
        assert.ok(m[key] instanceof Float32Array);
        assert.ok(m[key + "64"] instanceof Float64Array);
        assert.equal(m[key].length, 16);
      }
      const point = [0.3, -0.2, 0.1];
      const screen = C.project(camera, viewport, point);
      const ray = C.unprojectRay(camera, viewport, screen.x, screen.y);
      const distance = screen.depth * (camera.far - camera.near);
      vector(ray.origin.map((v, i) => v + ray.direction[i] * distance), point);
      const clip = [0, 1, 2, 3].map((row) => m.viewProjection64[row] * point[0] + m.viewProjection64[row + 4] * point[1] + m.viewProjection64[row + 8] * point[2] + m.viewProjection64[row + 12]);
      vector(clip, [(screen.x - (viewport.x || 0)) / viewport.width * 2 - 1, 1 - (screen.y - (viewport.y || 0)) / viewport.height * 2, screen.depth * 2 - 1, 1]);
    }
  });
}
check("pan inverse and pointer direction", () => {
  const c = C.createCamera(axes);
  const before = C.project(c, vp, [0.1, 0.2, 0.3]);
  C.pan(c, 30, -15, vp);
  const after = C.project(c, vp, [0.1, 0.2, 0.3]);
  close(after.x - before.x, 30);
  close(after.y - before.y, -15);
  C.pan(c, -30, 15, vp);
  vector(c.target, [0, 0, 0]);
});
check("anchored zoom inverse, anchor stays fixed", () => {
  const c = C.createCamera(axes);
  const p = [0.4, -0.3, 0.2];
  const before = C.project(c, vp, p);
  const anchor = [before.x / vp.width * 2 - 1, 1 - before.y / vp.height * 2];
  C.zoom(c, 2, anchor);
  const after = C.project(c, vp, p);
  close(after.x, before.x);
  close(after.y, before.y);
  C.zoom(c, 0.5, anchor);
  vector(c.target, [0, 0, 0]);
  assert.equal(c.zoom, 1);
});
check("zoom factor magnifies", () => {
  const c = C.createCamera(axes);
  const before = C.project(c, vp, [1, 0, 0]);
  C.zoom(c, 3);
  close(C.project(c, vp, [1, 0, 0]).x - 400, 3 * (before.x - 400));
});
check("reset identity including initial negative azimuth", () => {
  const c = C.createCamera(axes);
  const initial = C.serialize(c);
  C.orbit(c, 72, 14);
  C.pan(c, 8, 19, vp);
  C.zoom(c, 8);
  C.reset(c);
  assert.deepEqual(C.serialize(c), initial);
});
check("orbit pole safety and degree button", () => {
  const c = C.createCamera(axes);
  C.orbit(c, 0, -1e6);
  assert.equal(c.elevation, 89.999);
  assert.ok(C.matrices(c, 800, 600).view.every(Number.isFinite));
  C.orbit(c, 0, 1e6);
  assert.equal(c.elevation, -89.999);
  const a = c.azimuth;
  C.rotateAzimuth(c, 15);
  close(c.azimuth, (a + 15) % 360);
  assert.throws(() => C.orbit(c, 0, NaN), RangeError);
});
check("view state identity and invalid restore", () => {
  const c = C.createCamera(axes);
  const id = { epoch: 7, job: "a".repeat(32), figure: 1 };
  C.pan(c, 23, 19, vp);
  C.zoom(c, 3);
  const state = JSON.parse(JSON.stringify(C.serialize(c, id)));
  const other = C.createCamera(axes);
  assert.equal(C.restore(other, state, { ...id, epoch: 8 }), false);
  assert.equal(C.restore(other, state, { ...id, job: "b".repeat(32) }), false);
  assert.equal(C.restore(other, state, { ...id, figure: 2 }), false);
  assert.equal(C.restore(other, state, id), other);
  assert.deepEqual(C.serialize(other, id), state);
  state.target[0] = 999;
  assert.notEqual(other.target[0], 999);
  const before = C.serialize(other);
  for (const bad of [null, {}, { ...state, zoom: 0 }, { ...state, elevation: 100 }, { ...state, target: [NaN, 0, 0] }]) assert.equal(C.restore(other, bad), false);
  assert.deepEqual(C.serialize(other), before);
});
check("axis box classification and reversed tick anchors", () => {
  const a = fixture("reversed_view");
  const c = C.createCamera(a);
  const box = C.axisBox(c, a);
  assert.equal(box.faces.length, 6);
  assert.equal(box.edges.length, 12);
  assert.deepEqual(box.faces.map((f) => f.away), [true, false, false, true, true, false]);
  for (const [i, key] of ["x", "y", "z"].entries()) {
    assert.equal(box.axes[key].ticks.length, a[key + "tick"].length);
    const ticks = box.axes[key].ticks;
    close(ticks[0].position[i], box.extent[i]);
    close(ticks.at(-1).position[i], -box.extent[i]);
    assert.ok(box.axes[key].labelAnchor.every(Number.isFinite));
  }
});
check("sphere fit keeps every box corner visible in landscape/portrait", () => {
  const c = C.createCamera(axes);
  for (const viewport of [vp, { width: 200, height: 900 }]) {
    for (const x of [-1, 1]) for (const y of [-1, 1]) for (const z of [-1, 1]) {
      const p = C.project(c, viewport, [x, y, z]);
      assert.ok(p.x >= 0 && p.x <= viewport.width && p.y >= 0 && p.y <= viewport.height && p.depth > 0 && p.depth < 1);
    }
  }
});
check("invalid gestures and viewport are rejected", () => {
  const c = C.createCamera(axes);
  assert.throws(() => C.zoom(c, 0), RangeError);
  assert.throws(() => C.zoom(c, 1, [Infinity, 0]), RangeError);
  assert.throws(() => C.pan(c, NaN, 0, vp), RangeError);
  assert.throws(() => C.rotateAzimuth(c, Infinity), RangeError);
  assert.throws(() => C.matrices(c, 0, 10), RangeError);
});
check("extreme finite limits normalise without overflow", () => {
  const a = structuredClone(axes);
  for (const key of ["x", "y", "z"]) a[key + "lim"] = [-1e308, 1e308];
  a.data_aspect_ratio = [1, 1, 1];
  vector(C.dataToNormalised(a, [0, 0, 0]), [0, 0, 0]);
  vector(C.dataToNormalised(a, [1e308, -1e308, 0]), [1, -1, 0]);
  vector(C.normalisedToData(a, [0, 0, 0]), [0, 0, 0]);
  assert.deepEqual(C.normalisedToData(a, [1, -1, 0]), [1e308, -1e308, 0]);
});
console.log(`${count} camera cases passed.`);
