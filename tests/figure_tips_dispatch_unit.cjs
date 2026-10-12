"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
const { validate } = require("../frontend/figure_data_utils.cjs");

class Target {
  constructor() { this.listeners = new Map(); this.capture = new Set(); }
  addEventListener(name, fn, options) {
    if (options === true || options?.capture) this.capture.add(fn);
    if (!this.listeners.has(name)) this.listeners.set(name, new Set());
    this.listeners.get(name).add(fn);
  }
  removeEventListener(name, fn) { this.listeners.get(name)?.delete(fn); }
  dispatch(name, event = {}) { for (const fn of [...(this.listeners.get(name) || [])].sort((a,b) => Number(this.capture.has(b))-Number(this.capture.has(a)))) fn(event); }
  get listenerCount() { return [...this.listeners.values()].reduce((sum, set) => sum + set.size, 0); }
}
class Node extends Target {
  constructor(tag) {
    super();
    this.tag = tag;
    this.children = [];
    this.attrs = {};
    this.dataset = {};
    this.ownerDocument = global.document;
    this.style = {};
    this.clientWidth = 400; this.clientHeight = 300;
    this.offsetWidth = 120; this.offsetHeight = 60;
    this.classList = { add() {}, remove() {} };
    this.captured = new Set();
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
  getAttribute(name) { return this.attrs[name] ?? null; }
  removeAttribute(name) { delete this.attrs[name]; }
  replaceChildren(...nodes) { this.children.forEach(node => node.parent = null); this.children = []; this.append(...nodes); }
  focus() {}
  setPointerCapture(id) { this.captured.add(id); }
  hasPointerCapture(id) { return this.captured.has(id); }
  releasePointerCapture(id) { this.captured.delete(id); }
}

// Only DOM/canvas/WebGL and translation are boundaries. Viewer, tools, state
// machine, geometry, camera, picking and tip formatting are production modules.
(async () => {
  class Renderer {
    requestRender(draw) { this.pending = draw; }
    render() {}
    dispose() {}
  }
  global.figureTipsHarness = { Renderer };
  global.window = new Target();
  const selector = new Node("select");
  const controls = { querySelector: name => name === "#figure-tool-select" ? selector : null,
    querySelectorAll: () => [] };
  global.document = Object.assign(new Target(), { body: {},
    createElement: tag => new Node(tag), querySelector: name => name === "#figure-tools" ? controls : null });
  global.devicePixelRatio = 2;
  global.matchMedia = () => new Target();
  global.getComputedStyle = () => ({ getPropertyValue: () => "#ffffff" });
  global.ResizeObserver = class { observe() {} disconnect() {} };
  const filename = path.resolve("frontend/figure3d.js");
  const build = await esbuild.build({ entryPoints: [filename], bundle: true, write: false, platform: "node", format: "cjs",
    plugins: [{ name: "render-boundaries", setup(build) {
      build.onResolve({ filter: /^\.\/(?:figure_webgl|i18n)\.js$/ }, args => ({ path: path.basename(args.path), namespace: "boundary" }));
      build.onLoad({ filter: /.*/, namespace: "boundary" }, args => ({ contents: args.path === "figure_webgl.js" ?
        "export const FigureWebGL=globalThis.figureTipsHarness.Renderer;" :
        "export const t=k=>k; export const onLanguageChange=()=>()=>{}; export const applyStaticTranslations=()=>{};", loader: "js" }));
    } }]
  });
  const loaded = new Module(filename, module);
  loaded._compile(build.outputFiles[0].text, filename);
  const { Figure3D } = loaded.exports;
  const { triangulatePatch } = require("../frontend/figure_patch3d_utils.cjs");
  const matrix = rows => ({ shape: [rows.length, rows[0].length], order: "column-major",
    values: rows[0].flatMap((_, c) => rows.map(row => row[c])) });
  function fixture(kind, visible) {
    const f = JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/" + (kind === "surface" ? "colorbar" : "surf_vector") + ".json"));
    const a = f.axes[0];
    if (kind !== "surface") {
      Object.assign(a, { xlim: [0,1], ylim: [0,1], zlim: [0,2], view: [0,90] });
      Object.assign(a.camera, { position: [.5,.5,10], target: [.5,.5,1], up_vector: [0,1,0] });
      if (kind === "patch") {
        const vertices = [[0,0,1],[1,0,1],[1,1,1],[0,1,1]], faces = [[0,1,2,3]];
        const g = triangulatePatch(vertices, faces);
        a.series = [{ id:0, kind:"patch3d", role:"patch", index_base:0,
          triangulation:"first-valid-ear-v1", polygon_vertex_limit:64, vertices:matrix(vertices),
          faces:matrix(faces), triangle_indices:matrix(g.triangles), triangle_faces:g.faceOfTriangle,
          edge_indices:matrix(g.edges), triangle_count:2,
          face_color:{mode:"constant",association:"constant",rgb:[.2,.6,.8]},
          edge_color:{mode:"none",association:"constant"}, line_style:"-", line_width:1,
          display_name:"Patch", decimated:false, original_points:4, rendered_points:4 }];
        f.vertex_count=4; f.triangle_count=2;
      } else {
        a.series = [{ id:0, kind:"line", role:"contour", level:1, contour_closed:false,
          x:[.1,.5,.9],y:[.1,.5,.9],z:[1,1,1],source_indices:[0,1,2],
          line_color:[0,1,0],marker:"none",marker_size:0,marker_edge_color:"none",marker_face_color:"none",
          marker_face_auto:false,line_width:2,line_style:"-",display_name:"Contour",decimated:false,
          original_points:3,rendered_points:3,
          contour_color:{mode:"constant",association:"constant",rgb:[0,1,0]} }];
        f.vertex_count=3; f.triangle_count=0;
      }
    }
    a.visible = visible;
    a.position = [0,0,1,1];
    const checked = validate(f);
    assert(checked.ok && checked.supported, JSON.stringify(checked));
    return checked.data;
  }
  const descendants = node => [node,...node.children.flatMap(descendants)];
  let clicks = 0;
  for (const kind of ["patch","contour","surface"]) for (const visible of [true,false]) {
    const root = new Node("div"), data = fixture(kind, visible);
    const viewer = new Figure3D(root, data, { identity:{epoch:1,job:"f".repeat(32),figure:1},
      save() {}, failed() { assert.fail("unexpected fallback"); } });
    assert.equal(selector.value, "rotate", "new viewer resets mode after prior pins");
    selector.value = "tips";
    selector.dispatch("change");
    const points = kind === "patch" ? [[.5,.5,1],[.1,.1,1],[.05,.05,1],[.02,.02,1]] :
      kind === "contour" ? [[.5,.5,1]] : [[1.5,1.5,2.5]];
    for (const xyz of points) {
      viewer.wrap.dispatch("keydown", {key:"Escape",preventDefault(){},stopPropagation(){}});
      const pixel = viewer.wrap.figureTest.project(0,xyz);
      const state = viewer.states();
      const event = {clientX:pixel.x,clientY:pixel.y,pointerId:1,button:0,preventDefault(){}};
      viewer.wrap.dispatch("pointerdown",event);
      viewer.wrap.dispatch("pointerup",event);
      const pins = descendants(viewer.wrap).filter(node => node.className === "figure-pinned-tip");
      assert.equal(pins.length,1,kind + " visible=" + visible + " point=" + xyz);
      const hit = viewer.tools.getState().pins[0].hit;
      assert.equal(hit.axis,0);
      assert.deepEqual(viewer.states(),state,"tip click must not move camera");
      if (kind === "patch") {
        assert.match(pins[0].textContent,/Face: 1/);
        assert.match(pins[0].textContent,/Z: 1/);
        if (xyz[0] < .5) {
          assert.match(pins[0].textContent,/Vertex: 1/);
          assert.deepEqual([hit.x,hit.y,hit.z],[0,0,1]);
        }
      } else if (kind === "contour") {
        assert.match(pins[0].textContent,/Level: 1/);
        assert.match(pins[0].textContent,/Index: 2/);
        assert.deepEqual([hit.x,hit.y,hit.z],[.5,.5,1]);
      } else {
        assert.match(pins[0].textContent,/Row: \d/);
        assert.match(pins[0].textContent,/Column: \d/);
        assert("colorValue" in hit);
      }
      clicks++;
    }
    if (!visible) {
      const state=viewer.states(), vp=viewer.frames()[0].viewport;
      assert.equal(viewer.axisAt(vp.x+vp.width/2,vp.y+vp.height/2),-1);
      viewer.pan(10,20); viewer.zoom(2);
      assert.deepEqual(viewer.states(),state);
    }
    viewer.dispose();
    assert.equal(root.children.length,0);
    assert.equal(selector.listenerCount,0);
  }
  console.log("PASS " + clicks + " real click-to-pinned-tip dispatches: patch, contour, surface; visible and axis off");
})().catch(error => { console.error(error); process.exit(1); });
