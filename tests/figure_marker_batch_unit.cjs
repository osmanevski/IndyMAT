"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
const geometry = require("../frontend/figure_geometry_utils.cjs");
const cameraMath = require("../frontend/figure_camera_utils.cjs");
const { unbatchedSource } = require("./figure_marker_reference.cjs");
const filename = path.resolve("frontend/figure_webgl.js");
const source = fs.readFileSync(filename, "utf8");
function load(text) {
  const loaded = new Module(filename, module);
  loaded.filename = filename;
  loaded.paths = Module._nodeModulePaths(path.dirname(filename));
  loaded._compile(esbuild.transformSync(text, { format: "cjs", target: "node18" }).code, filename);
  return loaded.exports.FigureWebGL;
}
const Batched = load(source);
const Unbatched = load(unbatchedSource(source));
const fixture = JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3/scatter3_sizes.json"), "utf8"));
const palette = { background: [0, 0, 0], text: [1, 1, 1], axis: [.3, .3, .3], grid: [.2, .2, .2] };
function mockGL() {
  const draws = [];
  const uniforms = {};
  const buffers = new Map();
  const vaos = new Map();
  const enabled = new Map();
  const components = new Map();
  let vao;
  let buffer;
  let serial = 0;
  const gl = new Proxy({
    draws, uniforms,
    createBuffer: () => ++serial,
    createVertexArray: () => ++serial,
    createProgram: () => ++serial,
    createShader: () => ++serial,
    getShaderParameter: () => true,
    getProgramParameter: () => true,
    getUniformLocation: (_, name) => name,
    getExtension: () => null,
    isContextLost: () => false,
    bindVertexArray: (value) => { vao = value; if (!vaos.has(vao)) vaos.set(vao, []); },
    bindBuffer: (_, value) => { buffer = value; },
    bufferData: (_, values) => buffers.set(buffer, values.slice()),
    vertexAttribPointer: (index, size) => { vaos.get(vao)[index] = buffer; components.set(index, size); },
    enableVertexAttribArray: (index) => { if (!enabled.has(vao)) enabled.set(vao, new Set()); enabled.get(vao).add(index); },
    disableVertexAttribArray: (index) => enabled.get(vao)?.delete(index),
    uniform1i: (name, value) => { uniforms[name] = value; },
    uniform1f: (name, value) => { uniforms[name] = Math.fround(value); },
    uniform3fv: (name, value) => { uniforms[name] = new Float32Array(value); },
    drawArrays: (_, start, count) => {
      for (const index of enabled.get(vao) || []) {
        assert(buffers.get(vaos.get(vao)[index]).length >= count * components.get(index), "enabled attribute has insufficient storage");
      }
      if (!uniforms.marker) return;
      const attributes = vaos.get(vao).map((id) => buffers.get(id)?.slice());
      draws.push({ count, attributes, uniforms: structuredClone(uniforms) });
    }
  }, { get(target, key) {
    if (key in target) return target[key];
    return /^[A-Z_]+$/.test(key) ? key : () => {};
  } });
  return gl;
}
function render(Renderer, scene, camera, viewport) {
  const gl = mockGL();
  const canvas = { width: 800, height: 600, getContext: (type) => { assert.equal(type, "webgl2"); return gl; }, addEventListener() {}, removeEventListener() {} };
  const renderer = new Renderer(canvas, [scene], () => assert.fail("fallback"));
  const frame = [{ scene, camera, viewport }];
  renderer.render(frame, palette, 1);
  const storage = [renderer.markerPositions, renderer.markerUV, renderer.markerData];
  renderer.render(frame, palette, 1);
  storage.forEach((value, i) => assert.equal(value, [renderer.markerPositions, renderer.markerUV, renderer.markerData][i], "redraw reallocates batch storage"));
  renderer.dispose();
  return gl.draws.slice(0, gl.draws.length / 2);
}
function normalized(draws, batched) {
  const positions = [];
  const rgb = [];
  const uv = [];
  const data = [];
  const style = [];
  for (const draw of draws) {
    positions.push(...draw.attributes[0]);
    rgb.push(...draw.attributes[1]);
    uv.push(...draw.attributes[2]);
    if (batched) data.push(...draw.attributes[3]);
    for (let i = 0; i < draw.count; i++) {
      if (!batched) data.push(...draw.uniforms.origin, draw.uniforms.border);
      style.push({ marker: draw.uniforms.marker, hasFace: draw.uniforms.hasFace, hasEdge: draw.uniforms.hasEdge,
        faceInk: draw.uniforms.faceInk, edgeInk: draw.uniforms.edgeInk, clip: draw.uniforms.clip,
        low: draw.uniforms.low, high: draw.uniforms.high });
    }
  }
  return { positions, rgb, uv, data, style };
}
let cases = 0;
for (const marker of ["o", "s", "square", "^", "v", ">", "<", ".", "+", "x", "*"]) {
  for (const sizeUnits of ["points", "points-squared"]) {
    for (const style of ["face", "edge", "auto", "both"]) {
      const axes = structuredClone(fixture.axes[0]);
      axes.visible = false;
      const scene = geometry.buildScene(axes);
      scene.lines = [];
      const points = scene.points[0];
      Object.assign(points, { marker, sizeUnits, faceColor: style === "edge" ? "none" : [0.01, 0.01, 0.01],
        edgeColor: style === "face" ? "none" : [0.8, 0.1, 0.4], markerFaceAuto: style === "auto" });
      // Both limit planes, nearby Float32 centres, out-of-range centres,
      // zero and nonfinite sizes, and NaN gaps all use the old acceptance rule.
      points.positions = new Float32Array([
        ...scene.bounds.min, ...scene.bounds.max, 0, 0, 0,
        scene.bounds.min[0] - 0.000001, 0, 0,
        scene.bounds.max[0] + 0.000001, 0, 0,
        scene.bounds.max[0] + 0.01, 0, 0,
        NaN, 0, 0, 0, NaN, 0, 0, 0, NaN,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
      ]);
      points.sizes = new Float32Array([1, 64, 0.1, 9, 200, 9, 9, 9, 9, 0, NaN, Infinity, -1]);
      const camera = cameraMath.createCamera(axes);
      const viewport = { x: 0, y: 0, width: 300, height: 200 };
      for (const gesture of [() => {}, () => cameraMath.orbit(camera, 35, -12),
        () => cameraMath.pan(camera, 12, 5, viewport), () => cameraMath.zoom(camera, 1.4)]) {
        gesture();
        const before = render(Unbatched, scene, camera, viewport);
        const after = render(Batched, scene, camera, viewport);
        assert.equal(after.length, 1, "series must have exactly one marker draw");
        assert(before.length > 1);
        assert.deepEqual(normalized(after, true), normalized(before, false), `${marker}/${sizeUnits}/${style}`);
        cases++;
      }
    }
  }
}
const axes = structuredClone(fixture.axes[0]);
axes.visible = false;
const scene = geometry.buildScene(axes);
scene.lines = [];
scene.points[0].positions = new Float32Array(2000 * 3);
scene.points[0].sizes = new Float32Array(2000).fill(16);
const camera = cameraMath.createCamera(axes);
const viewport = { x: 0, y: 0, width: 800, height: 600 };
const before = render(Unbatched, scene, camera, viewport);
const after = render(Batched, scene, camera, viewport);
assert.equal(before.length, 2000);
assert.equal(after.length, 1);
assert.deepEqual(normalized(after, true), normalized(before, false));
// Separate series keep separate style uniforms and painter order.
scene.points.push({ ...scene.points[0], positions: new Float32Array(3), sizes: new Float32Array([4]),
  marker: "+", faceColor: "none", edgeColor: [0.1, 0.8, 0.4] });
const mixedBefore = render(Unbatched, scene, camera, viewport);
const mixedAfter = render(Batched, scene, camera, viewport);
assert.equal(mixedBefore.length, 2001);
assert.equal(mixedAfter.length, 2);
assert.deepEqual(normalized(mixedAfter, true), normalized(mixedBefore, false));
scene.points.pop();
// A following non-marker draw must disable the marker-data attribute: its
// buffer may be shorter than the line's vertex count, even though marker=0.
scene.points[0].positions = new Float32Array(3);
scene.points[0].sizes = new Float32Array([16]);
scene.lines = [{ positions: new Float32Array([-0.5, 0, 0, 0.5, 0, 0]), breaks: [0, 2],
  style: "--", width: 1, color: [1, 1, 1] }];
assert.equal(render(Batched, scene, camera, viewport).length, 1);
scene.lines = [];
// Empty/zero-only series upload no marker draw.
scene.points[0].sizes.fill(0);
assert.equal(render(Batched, scene, camera, viewport).length, 0);
console.log(`${cases} marker shape/size/style/camera reference cases passed; 2000 marker draws -> 1; zero-only -> 0. GL mocked; shader compilation/pixels unverified.`);
