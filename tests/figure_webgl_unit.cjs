"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
const geometry = require("../frontend/figure_geometry_utils.cjs");
const cameraMath = require("../frontend/figure_camera_utils.cjs");
const { validate } = require("../frontend/figure_data_utils.cjs");

const filename = path.resolve("frontend/figure_webgl.js");
const bundle = esbuild.buildSync({ entryPoints: [filename], bundle: true, write: false, platform: "node", format: "cjs" });
const loaded = new Module(filename, module);
loaded.paths = module.paths;
loaded._compile(bundle.outputFiles[0].text, filename);
const { FigureWebGL, screenPolyline } = loaded.exports;

const callbacks = new Map();
let nextFrame = 0;
global.requestAnimationFrame = (callback) => {
  callbacks.set(++nextFrame, callback);
  return nextFrame;
};
global.cancelAnimationFrame = (frame) => callbacks.delete(frame);
function flush() {
  const pending = [...callbacks.values()];
  callbacks.clear();
  pending.forEach((callback) => callback());
}
function glMock({ compile = true, link = true } = {}) {
  const calls = [];
  const live = new Map();
  let serial = 0;
  let lost = false;
  const gl = new Proxy({
    calls, live,
    getShaderParameter: () => compile,
    getProgramParameter: () => link,
    getUniformLocation: (_, name) => name,
    isContextLost: () => lost,
    getExtension: () => ({ loseContext: () => { lost = true; calls.push(["loseContext"]); } }),
    readPixels: (...args) => { calls.push(["readPixels", ...args.slice(0, 6)]); args[6].set([10, 20, 30, 255]); }
  }, {
    get(target, name) {
      if (name in target) return target[name];
      if (/^[A-Z_]+$/.test(name)) return name;
      if (name.startsWith("create")) return () => {
        const value = { type: name.slice(6), serial: ++serial };
        live.set(value, value.type);
        calls.push([name, value]);
        return value;
      };
      if (name.startsWith("delete")) return (value) => {
        assert(live.delete(value), "resource deleted twice or unknown");
        calls.push([name, value]);
      };
      return (...args) => calls.push([name, ...args]);
    }
  });
  return gl;
}
function canvas(gl) {
  const listeners = new Map();
  return { width: 800, height: 600, listeners,
    getContext: (name, options) => { assert.equal(name, "webgl2"); assert(!options.preserveDrawingBuffer); return gl; },
    addEventListener: (name, callback) => listeners.set(name, callback),
    removeEventListener: (name) => listeners.delete(name) };
}
const fixture = (name) => {
  const result = validate(JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json"), "utf8")));
  assert(result.ok && result.supported);
  return result.data.axes[0];
};
const palette = { background: [0, 0, 0], text: [1, 1, 1], grid: [0.2, 0.2, 0.2], axis: [0.3, 0.3, 0.3] };

let count = 0;
function test(name, callback) {
  callback();
  count++;
  console.log("PASS " + name);
}
test("context creation failure is localized by code and leaves no listener", () => {
  const node = canvas(null);
  assert.throws(() => new FigureWebGL(node, [], () => {}), (error) => error.reason_code === "webgl_unavailable");
  assert.equal(node.listeners.size, 0);
});
for (const stage of ["compile", "link"]) test(stage + " failure releases every partial resource", () => {
  const gl = glMock({ [stage]: false });
  const node = canvas(gl);
  assert.throws(() => new FigureWebGL(node, [], () => {}), (error) => error.reason_code === "shader_failure");
  assert.equal(gl.live.size, 0);
  assert.equal(node.listeners.size, 0);
});
test("RAF coalesces changes without a perpetual loop, dispose cancels", () => {
  const renderer = new FigureWebGL(canvas(glMock()), [], () => {});
  let draws = 0;
  renderer.requestRender(() => draws++);
  renderer.requestRender(() => draws++);
  assert.equal(callbacks.size, 1);
  flush();
  assert.equal(draws, 1);
  assert.equal(callbacks.size, 0);
  renderer.requestRender(() => draws++);
  renderer.dispose();
  flush();
  assert.equal(draws, 1);
});
test("only one context remains active on replacement; disposal idempotent", () => {
  const oldGL = glMock();
  const old = new FigureWebGL(canvas(oldGL), [], () => {});
  const nextGL = glMock();
  const next = new FigureWebGL(canvas(nextGL), [], () => {});
  assert(old.disposed);
  assert.equal(oldGL.live.size, 0);
  old.dispose();
  next.dispose();
  assert.equal(nextGL.live.size, 0);
});
test("context loss emits reason once, cancels pending draw and releases", () => {
  const gl = glMock();
  const node = canvas(gl);
  const failures = [];
  const renderer = new FigureWebGL(node, [], (code) => failures.push(code));
  renderer.requestRender(() => assert.fail("draw after loss"));
  let prevented = false;
  node.listeners.get("webglcontextlost")({ preventDefault: () => { prevented = true; } });
  renderer.fail("context_lost");
  flush();
  assert(prevented);
  assert.deepEqual(failures, ["context_lost"]);
  assert.equal(gl.live.size, 0);
});
for (const name of ["plot3_gap", "scatter3_sizes", "surf_vector", "mesh_default", "surf_matrix_nan", "surf_integer_clim", "colorbar", "reversed_view"]) test("real fixture draw commands: " + name, () => {
  const axes = fixture(name);
  const scene = geometry.buildScene(axes);
  const gl = glMock();
  const renderer = new FigureWebGL(canvas(gl), [scene], () => assert.fail("unexpected fallback"));
  const frame = { scene, camera: cameraMath.createCamera(axes), viewport: { x: 10, y: 20, width: 300, height: 200 } };
  const resources = gl.live.size;
  renderer.render([frame], palette, 2);
  renderer.render([frame], palette, 2);
  assert.equal(gl.live.size, resources, "draw allocated GL resources");
  assert(gl.calls.some(([method, cap]) => method === "enable" && cap === gl.DEPTH_TEST));
  assert(gl.calls.some(([method, cap]) => method === "disable" && cap === gl.CULL_FACE));
  assert(gl.calls.some(([method, uniform, values]) => method === "uniform3fv" && uniform === "low" && values === scene.bounds.min));
  assert(gl.calls.some(([method, uniform, values]) => method === "uniform3fv" && uniform === "high" && values === scene.bounds.max));
  assert(gl.calls.some(([method, x, y, w, h]) => method === "viewport" && x === 20 && y === 160 && w === 600 && h === 400));
  if (scene.meshes.length) {
    assert(gl.calls.some(([method, mode, count, type]) => method === "drawElements" && count === scene.meshes[0].indices.length && type === gl.UNSIGNED_INT));
    assert(gl.calls.some(([method, factor, units]) => method === "polygonOffset" && factor === 1 && units === 1));
    const uploaded = gl.calls.filter(([method, , values]) => method === "bufferData" && values === scene.meshes[0].colors);
    assert.equal(uploaded.length, 1, "surface colors must be uploaded unchanged");
  }
  if (scene.lines.length || scene.points.length || scene.edges.length) assert(gl.calls.some(([method]) => method === "drawArrays"));
  assert.deepEqual(renderer.readPixels(10, 20, 2), [10, 20, 30, 255]);
  assert(gl.calls.some(([method, x, y]) => method === "readPixels" && x === 20 && y === 559));
  renderer.dispose();
  assert.equal(gl.live.size, 0);
});
test("viewport clipping bounds work at extreme zoom without losing dash phase", () => {
  const points = [{ x: -1e8, y: 50, depth: 0.5 }, { x: 1e8, y: 50, depth: 0.5 }];
  for (const style of ["-", "--", ":", "-."]) {
    const expanded = screenPolyline(points, 2, undefined, style, { width: 100, height: 100 });
    assert(expanded.length > 0 && expanded.length < 1000);
    assert([...expanded].every(Number.isFinite));
  }
  assert.equal(screenPolyline(points.map((point) => ({ ...point, y: 200 })), 2, undefined, "-", { width: 100, height: 100 }).length, 0);
});
test("screen polyline keeps finite-run gaps and dash continuity", () => {
  const points = [0, 8, 16, 30, 38].map((x) => ({ x, y: 5, depth: 0.5 }));
  const expected = geometry.expandPolyline(points, 2, [0, 3, 5], "--");
  assert.deepEqual(screenPolyline(points, 2, [0, 3, 5], "--", { width: 100, height: 100 }), expected);
});
delete global.requestAnimationFrame;
delete global.cancelAnimationFrame;
console.log(`${count} WebGL command/lifecycle cases passed. GL is mocked; shader compilation and pixels are NOT verified here.`);
