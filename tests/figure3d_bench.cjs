"use strict";

// CPU-only benchmark. A cached NOOP WebGL2 proxy captures command counts;
// timings do not represent browser rendering or GPU execution.
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const Module = require("node:module");
const esbuild = require("esbuild");
const { validate } = require("../frontend/figure_data_utils.cjs");
const geometry = require("../frontend/figure_geometry_utils.cjs");
const camera = require("../frontend/figure_camera_utils.cjs");

const quick = process.argv.includes("--quick");
const refArg = process.argv.find((arg) => arg.startsWith("--renderer-ref="));
const rendererRef = refArg?.slice("--renderer-ref=".length);
const warmups = 2;
const samples = 5;
const base = JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3/plot3_gap.json"), "utf8"));
const scatterBase = JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3/scatter3_sizes.json"), "utf8"));
const surfaceBase = JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3/colorbar.json"), "utf8"));

function setCameraAndTicks(axes, zlim = [0, 1], dataAspect = [1, 1, 1]) {
  axes.xlim = [0, 1];
  axes.ylim = [0, 1];
  axes.zlim = zlim;
  axes.camera.target = [0.5, 0.5, zlim[0] / 2 + zlim[1] / 2];
  axes.data_aspect_ratio = dataAspect;
  axes.xtick = [0, 0.25, 0.5, 0.75, 1];
  axes.ytick = [0, 0.25, 0.5, 0.75, 1];
  axes.ztick = zlim[0] === -1 ? [-1, -0.5, 0, 0.5, 1] : [0, 0.25, 0.5, 0.75, 1];
  axes.xticklabel = axes.xtick.map(String);
  axes.yticklabel = axes.ytick.map(String);
  axes.zticklabel = axes.ztick.map(String);
}

function axesFor(series) {
  const payload = structuredClone(base);
  payload.axes[0].series = series;
  setCameraAndTicks(payload.axes[0]);
  payload.vertex_count = series.reduce((sum, item) => sum + item.x.length, 0);
  payload.triangle_count = 0;
  return payload;
}

function lineSeries(count, kind = "line") {
  const x = [];
  const y = [];
  const z = [];
  const source_indices = [];
  const sizes = [];
  for (let i = 0; i < count; i++) {
    x.push((i % 1000) / 999);
    y.push((Math.floor(i / 1000) % 1000) / 999);
    z.push((i % 97) / 96);
    source_indices.push(i);
    sizes.push(16);
  }
  const source = kind === "scatter" ? scatterBase.axes[0].series[0] : base.axes[0].series[0];
  return { ...structuredClone(source), kind, x, y, z, source_indices, sizes,
    decimated: false, original_points: count, rendered_points: count, id: 0 };
}

function surface(size) {
  const values = Array.from({ length: size * size }, (_, i) => Math.sin(i * 0.003));
  const payload = structuredClone(surfaceBase);
  const series = payload.axes[0].series[0];
  series.shape = [size, size];
  series.x = { shape: [1, size], order: "column-major", values: Array.from({ length: size }, (_, i) => i / (size - 1)) };
  series.y = { shape: [size, 1], order: "column-major", values: Array.from({ length: size }, (_, i) => i / (size - 1)) };
  series.z = { shape: [size, size], order: "column-major", values };
  series.cdata = { shape: [size, size], order: "column-major", values };
  series.face_color.data = { shape: [size, size], order: "column-major", values };
  series.original_points = size * size;
  series.rendered_points = size * size;
  series.cell_origins = [];
  for (let column = 0; column < size - 1; column++) {
    for (let row = 0; row < size - 1; row++) series.cell_origins.push(row + column * size);
  }
  series.triangle_count = series.cell_origins.length * 2;
  const edges = [];
  for (let column = 0; column < size; column++) {
    for (let row = 0; row < size - 1; row++) edges.push([row + column * size, row + 1 + column * size]);
  }
  for (let column = 0; column < size - 1; column++) {
    for (let row = 0; row < size; row++) edges.push([row + column * size, row + (column + 1) * size]);
  }
  series.edge_indices = { shape: [edges.length, 2], order: "column-major",
    values: [...edges.map((edge) => edge[0]), ...edges.map((edge) => edge[1])] };
  series.edge_color.mode = "constant";
  series.mesh_style = "both";
  payload.vertex_count = size * size;
  payload.triangle_count = series.triangle_count;
  setCameraAndTicks(payload.axes[0], [-1, 1], [1, 1, 2]);
  return payload;
}

function medianMs(fn) {
  const values = [];
  for (let i = 0; i < warmups + samples; i++) {
    const start = performance.now();
    fn();
    const elapsed = performance.now() - start;
    if (i >= warmups) values.push(elapsed);
  }
  values.sort((a, b) => a - b);
  return +values[Math.floor(values.length / 2)].toFixed(3);
}

function validation(payload) {
  const result = validate(payload);
  if (!result.ok || !result.supported) {
    return { accepted: false, reason: result.reason_code || result.reason || "unsupported", path: result.path };
  }
  return { accepted: true, data: result.data };
}

// Transform a real renderer module without bundling, retaining its real
// CommonJS imports and filename. A git ref freezes the baseline implementation.
const rendererPath = path.resolve("frontend/figure_webgl.js");
const rendererSource = rendererRef
  ? execFileSync("git", ["show", `${rendererRef}:frontend/figure_webgl.js`], { encoding: "utf8" })
  : fs.readFileSync(rendererPath, "utf8");
const rendererCjs = esbuild.transformSync(rendererSource, { format: "cjs", target: "node18" }).code;
const rendererModule = new Module(rendererPath, module);
rendererModule.filename = rendererPath;
rendererModule.paths = Module._nodeModulePaths(path.dirname(rendererPath));
rendererModule._compile(rendererCjs, rendererPath);
const { FigureWebGL, screenPolyline } = rendererModule.exports;

const counters = { uploadBytes: 0, bufferData: 0, drawArrays: 0, drawElements: 0 };
let nextObject = 1;
const noopCache = new Map();
const glMethods = {
  createBuffer: () => ({ id: nextObject++ }),
  createVertexArray: () => ({ id: nextObject++ }),
  createProgram: () => ({ id: nextObject++ }),
  createShader: () => ({ id: nextObject++ }),
  getShaderParameter: () => true,
  getProgramParameter: () => true,
  getUniformLocation: (_program, name) => ({ name }),
  getExtension: () => null,
  isContextLost: () => false,
  bufferData: (_target, values) => {
    counters.bufferData++;
    counters.uploadBytes += values?.byteLength || 0;
  },
  drawArrays: () => counters.drawArrays++,
  drawElements: () => counters.drawElements++
};
const glConstants = {
  ARRAY_BUFFER: 1, ELEMENT_ARRAY_BUFFER: 2, STATIC_DRAW: 3, DYNAMIC_DRAW: 4,
  FLOAT: 5, TRIANGLES: 6, UNSIGNED_INT: 7, VERTEX_SHADER: 8, FRAGMENT_SHADER: 9,
  COMPILE_STATUS: 10, LINK_STATUS: 11, SCISSOR_TEST: 12, COLOR_BUFFER_BIT: 13,
  DEPTH_BUFFER_BIT: 14, DEPTH_TEST: 15, LEQUAL: 16, CULL_FACE: 17, BLEND: 18,
  POLYGON_OFFSET_FILL: 19, RGBA: 20, UNSIGNED_BYTE: 21
};
const gl = new Proxy({}, {
  get(_target, property) {
    if (property in glMethods) return glMethods[property];
    if (property in glConstants) return glConstants[property];
    if (!noopCache.has(property)) noopCache.set(property, () => {});
    return noopCache.get(property);
  }
});

function createCanvas() {
  return { addEventListener() {}, removeEventListener() {}, getContext: () => gl, width: 800, height: 600 };
}

function resetCounters() {
  Object.assign(counters, { uploadBytes: 0, bufferData: 0, drawArrays: 0, drawElements: 0 });
}

function drawBench(scene) {
  resetCounters();
  const renderer = new FigureWebGL(createCanvas(), [scene], () => {});
  const uploads = { uploadBytes: counters.uploadBytes, bufferData: counters.bufferData };
  const cam = camera.createCamera(scene.axes);
  const viewport = { x: 0, y: 0, width: 800, height: 600 };
  const frames = [{ scene, camera: cam, viewport }];
  const palette = { background: [0, 0, 0], text: [1, 1, 1], axis: [0.3, 0.3, 0.3], grid: [0.2, 0.2, 0.2] };
  const render = () => renderer.render(frames, palette, 1);
  const unchanged = medianMs(render);
  const rotate = medianMs(() => { camera.orbit(cam, 13, 7); render(); });
  let zoomFactor = 1.01;
  const zoom = medianMs(() => {
    zoomFactor = zoomFactor === 1.01 ? 1 / 1.01 : 1.01;
    camera.zoom(cam, zoomFactor);
    render();
  });
  resetCounters();
  // Instrument only the command-count sample, outside the timed medians.
  const originalProject = camera.project;
  let projectionCalls = 0;
  camera.project = (...args) => { projectionCalls++; return originalProject(...args); };
  try { render(); } finally { camera.project = originalProject; }
  const oneRedraw = { uploadBytes: counters.uploadBytes, bufferData: counters.bufferData,
    drawArrays: counters.drawArrays, drawElements: counters.drawElements, projectionCalls };
  renderer.dispose();
  return { unchanged, rotate, zoom, uploads, oneRedraw };
}

function measureScene(label, payload) {
  const jsonBytes = Buffer.byteLength(JSON.stringify(payload), "utf8");
  const validationMs = medianMs(() => validation(payload));
  const validated = validation(payload);
  if (!validated.accepted) return { fixture: label, accepted: false, reason: validated.reason,
    path: validated.path, validationMs, jsonBytes };
  const sceneBuildMs = medianMs(() => geometry.buildScene(validated.data.axes[0]));
  const scene = geometry.buildScene(validated.data.axes[0]);
  const cam = camera.createCamera(scene.axes);
  const viewport = { x: 0, y: 0, width: 800, height: 600 };
  const projectionMs = medianMs(() => {
    for (const mesh of scene.meshes) {
      for (let i = 0; i < mesh.positions.length; i += 3) {
        camera.project(cam, viewport, mesh.positions.subarray(i, i + 3));
      }
    }
    const points = scene.lines[0]?.positions || scene.points[0]?.positions;
    if (points) {
      for (let i = 0; i < points.length; i += 3) camera.project(cam, viewport, points.subarray(i, i + 3));
    }
  });
  const expansionInput = Array.from({ length: Math.min(20000, scene.counts.vertices) }, (_, i) => ({
    x: i % 800, y: (i * 7) % 600, depth: 0.5
  }));
  const screenPolylineExpansionMs = medianMs(() => screenPolyline(expansionInput, 1, undefined, "-", { width: 800, height: 600 }));
  return { fixture: label, accepted: true, jsonBytes, validationMs, sceneBuildMs, projectionMs,
    screenPolylineExpansionMs, counts: scene.counts, redraw: drawBench(scene) };
}

const results = [];
for (const size of (quick ? [50, 200] : [50, 200, 500])) {
  results.push(measureScene(`surf ${size}x${size}`, surface(size)));
}
for (const [label, count, kind] of [
  ["plot3", 2000, "line"], ["scatter3", 2000, "scatter"],
  ["plot3 diagnostic", 20000, "line"], ["scatter3 diagnostic", 20000, "scatter"]
]) {
  results.push(measureScene(`${label} ${count}`, axesFor([lineSeries(count, kind)])));
}

const output = {
  benchmark: "figure3d-local-noop-gl",
  rendererRef: rendererRef || "working tree",
  runtime: process.version,
  iterations: { warmups, samples },
  note: "NOOP WebGL2 command sink; CPU JS timings only; no browser/GPU claim. Renderer uses the depth buffer; no sorting.",
  results
};
console.log(JSON.stringify(output, null, 2));
