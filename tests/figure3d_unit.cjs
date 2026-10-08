"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
const { validate } = require("../frontend/figure_data_utils.cjs");
const cameraMath = require("../frontend/figure_camera_utils.cjs");
const { colorbarGradient } = require("../frontend/figure_color_utils.cjs");

class Target {
  constructor() { this.listeners = new Map(); }
  addEventListener(name, fn) {
    if (!this.listeners.has(name)) this.listeners.set(name, new Set());
    this.listeners.get(name).add(fn);
  }
  removeEventListener(name, fn) { this.listeners.get(name)?.delete(fn); }
  get listenerCount() { return [...this.listeners.values()].reduce((sum, set) => sum + set.size, 0); }
}
class Node extends Target {
  constructor(tag) {
    super();
    this.tag = tag;
    this.children = [];
    this.attrs = {};
    this.dataset = {};
    const calls = this.calls = [];
    this.context = new Proxy({ measureText: (text) => ({ width: text.length * 6 }) }, {
      get(target, name) {
        return target[name] || ((...args) => calls.push([name, ...args, target.fillStyle, target.strokeStyle]));
      }
    });
  }
  append(...nodes) { nodes.forEach((node) => { node.parent = this; this.children.push(node); }); }
  remove() {
    if (this.parent) this.parent.children = this.parent.children.filter((node) => node !== this);
    this.parent = null;
  }
  getBoundingClientRect() { return { left: 0, top: 0, width: 400, height: 300 }; }
  setAttribute(name, value) { this.attrs[name] = value; }
  getContext() { return this.context; }
  closest() { return null; }
}

(async () => {
  const renderers = [];
  class Renderer {
    constructor(canvas, scenes, failed) {
      if (global.figure3dHarness.failRenderer) throw new Error("WebGL unavailable");
      this.canvas = canvas;
      this.scenes = scenes;
      this.failed = failed;
      this.renders = [];
      renderers.push(this);
    }
    requestRender(draw) { this.pending = draw; }
    flush() { const draw = this.pending; this.pending = null; draw?.(); }
    render(...args) { this.renders.push(args); }
    readPixels() { return [1, 2, 3, 255]; }
    dispose() { this.disposed = true; this.pending = null; }
  }
  const observers = [];
  const queries = [];
  const subscribers = new Set();
  let mounted = null;
  let toolDisposals = 0;
  global.figure3dHarness = { Renderer, subscribers,
    mount: (host, target) => {
      mounted = { host, target };
      return { reset: () => target.reset(), reprojectPins: () => {} };
    },
    unmount: () => { mounted = null; toolDisposals++; }
  };
  const win = global.window = new Target();
  global.document = Object.assign(new Target(), { body: {}, createElement: (tag) => new Node(tag) });
  global.devicePixelRatio = 2;
  global.matchMedia = () => {
    const query = new Target();
    queries.push(query);
    return query;
  };
  global.getComputedStyle = () => ({ getPropertyValue: (name) => ({ "--plot-bg": "#050719", "--plot-text": "#e0e0e0", "--plot-axis": "#465c6d", "--plot-grid": "#35424c" })[name] });
  global.ResizeObserver = class {
    constructor(fn) { this.fn = fn; observers.push(this); }
    observe(node) { this.node = node; }
    disconnect() { this.disconnected = true; }
  };
  const filename = path.resolve("frontend/figure3d.js");
  const build = await esbuild.build({ entryPoints: [filename], bundle: true, write: false, platform: "node", format: "cjs",
    plugins: [{ name: "viewer-boundaries", setup(build) {
      build.onResolve({ filter: /^\.\/(?:figure_webgl|figure_tools|i18n)\.js$/ }, (args) => ({ path: path.basename(args.path), namespace: "boundary" }));
      build.onLoad({ filter: /.*/, namespace: "boundary" }, (args) => ({ contents: {
        "figure_webgl.js": "export const FigureWebGL = globalThis.figure3dHarness.Renderer;",
        "figure_tools.js": "export const mountFigureTools=globalThis.figure3dHarness.mount; export const unmountFigureTools=globalThis.figure3dHarness.unmount; export const figureCanvasDescription=()=> 'Interactive 3D';",
        "i18n.js": "export const t=(key)=>key; export const onLanguageChange=(fn)=> { const s=globalThis.figure3dHarness.subscribers; s.add(fn); return ()=>s.delete(fn); };"
      }[args.path], loader: "js" }));
    } }]
  });
  const loaded = new Module(filename, module);
  loaded._compile(build.outputFiles[0].text, filename);
  const { Figure3D, drawColorbars } = loaded.exports;
  const fixture = (name) => {
    const result = validate(JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json"), "utf8")));
    assert(result.ok && result.supported);
    return result.data;
  };
  const identity = { epoch: 1, job: "f".repeat(32), figure: 1 };
  let saved;
  const root = new Node("div");
  const create = (name, options = {}) => new Figure3D(root, fixture(name), { identity, save: (state) => { saved = state; }, failed: () => assert.fail("unexpected failure"), ...options });
  let count = 0;
  const passed = (name) => { count++; console.log("PASS " + name); };
  try {
    let viewer = create("plot3_gap");
    assert.equal(mounted.target, viewer);
    assert.equal(viewer.canvas.width, 800);
    assert.equal(viewer.overlay.height, 600);
    assert(viewer.wrap.figureTest);
    viewer.renderer.flush();
    assert.equal(viewer.renderer.renders.length, 1);
    passed("viewer canvas stack, DPR and R2 target mount");
    const initial = viewer.states();
    viewer.orbit(40, 10);
    viewer.pan(12, -6);
    viewer.zoom(1.2, { x: 180, y: 160 });
    viewer.requestRender();
    assert.notDeepEqual(viewer.states(), initial);
    assert.deepEqual(saved, viewer.states());
    viewer.reset();
    assert.deepEqual(viewer.states(), initial);
    viewer.rotateAzimuth(15);
    assert.equal(viewer.cameras[0].azimuth, 337.5);
    passed("local gestures save state, reset exported baseline and rotate in degrees");
    viewer.reset();
    const point = viewer.wrap.figureTest.project(0, [2, 3, 4]);
    const hit = viewer.pick(point.x, point.y);
    assert.deepEqual(hit, { x: 2, y: 3, z: 4, seriesName: "", index: 1, axis: 0 });
    assert.equal(viewer.pick(-1, -1), null);
    passed("real gap fixture picker adapts original XYZ/index to R2 hit shape");
    viewer.orbit(30, 10);
    const changed = viewer.states();
    viewer.resize();
    subscribers.forEach((fn) => fn());
    assert.deepEqual(viewer.states(), changed);
    const renderer = viewer.renderer;
    const wrap = viewer.wrap;
    viewer.dispose();
    viewer.dispose();
    assert(renderer.disposed);
    assert.equal(root.children.length, 0);
    assert.equal(wrap.listenerCount, 0);
    assert.equal(win.listenerCount, 0);
    assert.equal(document.listenerCount, 0);
    assert.equal(subscribers.size, 0);
    assert(queries.every((query) => query.listenerCount === 0));
    assert(observers.every((observer) => observer.disconnected));
    assert.equal(wrap.figureTest, undefined);
    assert.equal(toolDisposals, 1);
    passed("resize/language preserve view; dispose removes hooks/listeners/observers/tools");
    viewer = create("plot3_gap", { saved: changed });
    assert.deepEqual(viewer.states(), changed);
    viewer.dispose();
    viewer = create("plot3_gap", { saved: changed, identity: { ...identity, job: "a".repeat(32) } });
    assert.equal(viewer.cameras[0].azimuth, initial[0].azimuth);
    viewer.dispose();
    passed("identity-scoped restore preserves tabs but rejects another artifact");
    viewer = create("colorbar");
    const surfacePoint = [1.5, 1.5, 2.5];
    const pixel = viewer.wrap.figureTest.project(0, surfacePoint);
    const surface = viewer.pick(pixel.x, pixel.y);
    assert(surface && Number.isInteger(surface.row) && Number.isInteger(surface.column));
    assert("colorValue" in surface && !("index" in surface));
    viewer.renderer.flush();
    assert(viewer.overlay.calls.some(([method, text]) => method === "fillText" && text === "Intensity"));
    viewer.dispose();
    passed("real surface picker returns row/column/raw color; colorbar label drawn");
    const axes = fixture("colorbar").axes[0];
    const ctxNode = new Node("canvas");
    const palette = { background: [0, 0, 0], text: [1, 1, 1], axis: [0.2, 0.2, 0.2] };
    drawColorbars(ctxNode.context, axes, 400, 300, palette);
    const swatches = ctxNode.calls.filter(([method]) => method === "fillRect");
    const gradient = colorbarGradient(axes.colorbars[0]);
    assert.equal(swatches.length, axes.colorbars[0].colormap.shape[0]);
    assert.equal(swatches[0][5], `rgb(${gradient[0].rgb.map((v) => Math.round(v * 255)).join(",")})`);
    assert.equal(ctxNode.calls.filter(([method]) => method === "fillText").length, axes.colorbars[0].ticks.length + 1);
    passed("colorbar discrete swatches remain unchanged and ticks/label are drawn");
    const mixed = fixture("colorbar");
    const two = fixture("line2d").axes[0];
    two.dimension = 2;
    mixed.axes[0].title_layout_position = [.1, .11, .35, .815];
    mixed.axes[0].position = [.1, .099, .35, .7335];
    two.title_layout_position = [.55, .11, .35, .815];
    two.position = [.55, .099, .35, .7335];
    two.series.push({ kind: "text", figure_title: true, font_size: 12, lines: ["Shared", "Second line"], margin: 2 });
    mixed.axes.push(two);
    let painted = 0;
    viewer = new Figure3D(root, mixed, { identity, save: () => {}, failed: () => {}, paint2D: (ctx, axes, width, height, size, allAxes) => { assert.equal(allAxes, mixed.axes); painted++; } });
    viewer.renderer.flush();
    assert.equal(painted, 1);
    assert.equal(viewer.scenes[1].lines.length, 0);
    const frames = viewer.frames();
    assert.equal(frames[0].viewport.y, frames[1].viewport.y);
    assert.equal(frames[0].viewport.height, frames[1].viewport.height);
    assert(frames[1].viewport.height < two.position[3] * viewer.height, "mixed frames failed to reserve extra shared-title space");
    viewer.dispose();
    passed("mixed admitted figure delegates 2D axes without silently dropping them");
    global.figure3dHarness.failRenderer = true;
    for (let attempt = 0; attempt < 3; attempt++) {
      assert.throws(() => create("plot3_gap"), /WebGL unavailable/);
      assert.equal(document.listenerCount, 0, "failed renderer leaked fullscreen listener");
      assert.equal(subscribers.size, 0, "failed renderer leaked language subscriber");
      assert.equal(root.children.length, 0, "failed renderer left viewer DOM behind");
    }
    passed("repeated failed WebGL startup cleans window controls and subscriptions");
    console.log(`${count} viewer API/overlay/lifecycle cases passed. Canvas, tools and WebGL boundaries are mocked; browser behavior remains unverified.`);
  } finally {
    for (const key of ["figure3dHarness", "window", "document", "devicePixelRatio", "matchMedia", "getComputedStyle", "ResizeObserver"]) delete global[key];
  }
})().catch((error) => { console.error(error); process.exit(1); });
