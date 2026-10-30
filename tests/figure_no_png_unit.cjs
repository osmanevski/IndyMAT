"use strict";
// When the Rust engine could not draw a PNG: a figure without one shows its localized
// reason_code message (never a broken image), and a drawable one is shown
// interactively even in PNG mode. With a PNG nothing changes.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
const fixture = (name) => JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json"), "utf8"));
const deferred = () => {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const settle = () => new Promise((resolve) => setImmediate(resolve));

class Node {
  constructor(tag = "div") {
    this.tag = tag;
    this.children = [];
    this.style = {};
    this.dataset = {};
    this.attrs = {};
    this.listeners = new Map();
    this.textContent = "";
    this.classes = new Set();
    this.classList = {
      toggle: (name, set) => (set ?? !this.classes.has(name)) ? this.classes.add(name) : this.classes.delete(name),
      remove: (name) => this.classes.delete(name),
      add: (name) => this.classes.add(name)
    };
  }
  append(...nodes) {
    for (const node of nodes) {
      node.remove();
      node.parent = this;
      this.children.push(node);
    }
  }
  remove() {
    if (this.parent) this.parent.children = this.parent.children.filter((node) => node !== this);
    this.parent = null;
  }
  before(node) {
    node.remove();
    const index = this.parent.children.indexOf(this);
    node.parent = this.parent;
    this.parent.children.splice(index, 0, node);
  }
  replaceChildren(...nodes) {
    this.children.forEach((node) => { node.parent = null; });
    this.children = [];
    this.append(...nodes);
  }
  setAttribute(name, value) { this.attrs[name] = value; }
  addEventListener(name, callback) { this.listeners.set(name, callback); }
  removeEventListener(name) { this.listeners.delete(name); }
  close() { this.open = false; this.listeners.get("close")?.(); }
  closest() { return null; }
  get isConnected() { return !!this.parent; }
  getBoundingClientRect() { return { width: 400, height: 240, x: 0, y: 0 }; }
  getContext() {
    return new Proxy({ measureText: (text) => ({ width: text.length * 6 }) }, { get: (target, name) => target[name] || (() => {}) });
  }
}

(async () => {
  let pngs = false;
  const requests = [];
  const revoked = [];
  const created = [];
  const nodes = new Map();
  for (const id of ["figure-tabs", "figure-count", "plot-area", "figure-caption", "plot-interactive", "plot-png", "plot-download", "plot-expand", "plot-fit", "rotate-left", "rotate-right", "figure-tools", "modal", "modal-body"]) nodes.set("#" + id, new Node());
  const parent = new Node();
  parent.append(nodes.get("#figure-tools"), nodes.get("#plot-area"));
  nodes.get("#modal").append(nodes.get("#modal-body"));
  global.devicePixelRatio = 1;
  global.document = Object.assign(new Node(), { body: new Node(), createElement: (tag) => new Node(tag) });
  global.getComputedStyle = () => ({ getPropertyValue: () => "#101010" });
  global.ResizeObserver = class { observe() {} disconnect() {} };
  global.MutationObserver = class { observe() {} disconnect() {} };
  const savedRevoke = URL.revokeObjectURL;
  URL.revokeObjectURL = (url) => revoked.push(url);
  const shared = { engine: { engine: { name: "indy", stub: false } }, uiGeneration: 1, figureIndex: 0, figureURLs: [], figureData: [], figures: [], lastFiguresKey: "", plotMode: "interactive" };
  const registry = {
    $: (id) => nodes.get(id),
    el: (tag, cls, text) => {
      const node = new Node(tag);
      node.className = cls;
      if (text !== undefined) node.textContent = text;
      return node;
    },
    api: (url, data) => {
      const response = deferred();
      requests.push({ url, data, ...response });
      return response.promise;
    },
    blobAPI: async (url) => {
      if (!pngs) throw new Error("The image could not be read.");
      return "blob:" + url;
    },
    persistActiveTab: () => {},
    requireIdle: () => {},
    setStatus: () => {},
    modal: (_, node) => {
      nodes.get("#modal-body").replaceChildren(node);
      nodes.get("#modal").open = true;
    }
  };
  class Viewer {
    constructor(root, data, options) {
      this.data = data;
      this.options = options;
      this.wrap = new Node();
      root.append(this.wrap);
      this.angle = data.axes[0].view[0];
      this.tools = { reset: () => { this.toolReset = true; } };
      created.push(this);
    }
    rotateAzimuth(angle) { this.angle += angle; }
    resize() {}
    requestRender() {}
    destroy() { this.destroyed = true; this.wrap.remove(); }
  }
  global.figureHarness = { registry, shared, Viewer };
  const build = await esbuild.build({
    stdin: { contents: 'import "./figures.js";', resolveDir: path.resolve("frontend"), sourcefile: "figure_harness.js" },
    bundle: true, write: false, platform: "node", format: "cjs",
    plugins: [{ name: "browser-boundaries", setup(build) {
      build.onResolve({ filter: /\/(?:state|registry|figure3d|figure_tools|i18n)\.js$|^\.\/(?:state|registry|figure3d|figure_tools|i18n)\.js$/ }, (args) => ({ path: path.basename(args.path), namespace: "boundary" }));
      build.onLoad({ filter: /.*/, namespace: "boundary" }, (args) => ({ contents: {
        "state.js": "export default globalThis.figureHarness.shared;",
        "registry.js": "export default globalThis.figureHarness.registry;",
        "figure3d.js": "export const Figure3D = globalThis.figureHarness.Viewer; export function drawColorbars() {}",
        "i18n.js": "export const t = (text) => text; export const onLanguageChange = () => () => {};",
        "figure_tools.js": "export const figureReasonText = (code) => 'reason:' + code; export const figureReductionText = (a,b) => 'reduced:' + a + ':' + b; export function unmountFigureTools() {}"
      }[args.path], loader: "js" }));
    } }]
  });
  const loaded = new Module(path.resolve("frontend/figure_harness.js"), module);
  loaded._compile(build.outputFiles[0].text, loaded.filename || "figure_harness.js");
  const job = "d".repeat(32);
  const manifest = () => ({ job, epoch: 1, figures: [{ name: "Figure 1", file: "figure-1.png", data_file: "figure-1.json", number: 1, interactive: false, fallback_reason: "Only the PNG view is available: no_axes." }] });
  const payload = (name) => {
    const data = fixture(name);
    data.source.job = job;
    data.source.figure = 1;
    return data;
  };
  const area = () => nodes.get("#plot-area");
  const kinds = () => area().children.map((node) => node.tag + (node.className ? "." + node.className : ""));
  let count = 0;
  const passed = (name) => { console.log("PASS " + name); count++; };
  try {
    await registry.updateFigures(manifest());
    requests.at(-1).resolve(payload("no_axes"));
    await settle();
    assert.deepEqual(kinds(), ["div.figure-unavailable"]);
    assert.equal(area().children[0].textContent, "reason:no_axes");
    assert.equal(area().children[0].attrs.role, "status");
    assert.equal(nodes.get("#figure-caption").textContent, "Figure 1 · reason:no_axes");
    for (const id of ["#plot-png", "#plot-download", "#plot-expand", "#plot-interactive"]) assert.equal(nodes.get(id).disabled, true, id);
    registry.expandFigure();
    assert.notEqual(nodes.get("#modal").open, true);
    passed("a fallback figure without PNG shows its reason_code message, not an image");

    shared.plotMode = "png";
    shared.lastFiguresKey = "";
    await registry.updateFigures({ ...manifest(), epoch: 2 });
    requests.at(-1).resolve(payload("plot3_gap"));
    await settle();
    assert(shared.interactivePlot instanceof Viewer, "drawable figure is shown interactively");
    assert(!kinds().includes("img"));
    assert.equal(nodes.get("#plot-png").disabled, true);
    assert.equal(nodes.get("#plot-interactive").disabled, false);
    passed("without a PNG a drawable figure stays interactive in PNG mode");

    pngs = true;
    shared.lastFiguresKey = "";
    await registry.updateFigures({ ...manifest(), epoch: 3 });
    requests.at(-1).resolve(payload("no_axes"));
    await settle();
    assert.deepEqual(kinds(), ["img"]);
    assert.equal(area().children[0].src, "blob:figure?job=" + job + "&file=figure-1.png");
    assert.equal(nodes.get("#plot-png").disabled, false);
    assert.equal(nodes.get("#plot-download").disabled, false);
    passed("with a PNG (Octave) the fallback image is shown as before");
  } finally {
    URL.revokeObjectURL = savedRevoke;
  }
  assert.equal(count, 3);
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
