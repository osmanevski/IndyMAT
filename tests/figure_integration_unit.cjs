"use strict";

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
  const shared = { uiGeneration: 1, figureIndex: 0, figureURLs: [], figureData: [], figures: [], lastFiguresKey: "", plotMode: "interactive" };
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
    blobAPI: async (url) => "blob:" + url,
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
  const jobA = "a".repeat(32);
  const jobB = "b".repeat(32);
  const manifest = (job, names = ["first"], epoch = 1) => ({ job, epoch, figures: names.map((name, i) => ({ name, file: `figure-${i + 1}.png`, data_file: `figure-${i + 1}.json`, number: 90 + i })) });
  const payload = (job, ordinal = 1, name = "plot3_gap") => {
    const data = fixture(name);
    data.source.job = job;
    data.source.figure = ordinal;
    return data;
  };
  const latest = () => requests.at(-1);
  const caption = () => nodes.get("#figure-caption").textContent;
  let count = 0;
  const passed = (name) => { console.log("PASS " + name); count++; };
  try {
    await registry.updateFigures(manifest(jobA, ["first", "second"]));
    assert.equal(requests.length, 1);
    assert(latest().url.includes("file=figure-1.json"));
    latest().resolve(payload(jobA));
    await settle();
    assert(shared.interactivePlot instanceof Viewer);
    passed("only active JSON is fetched and admitted v3 creates viewer");
    const executeBefore = requests.length;
    const angle = shared.interactivePlot.angle;
    await registry.rotate(-15);
    assert.equal(shared.interactivePlot.angle, angle - 15);
    registry.resetFigure();
    assert(shared.interactivePlot.toolReset);
    assert.equal(requests.length, executeBefore);
    passed("rotate and Reset route locally through viewer/tools without execute");
    const original = shared.interactivePlot;
    registry.expandFigure();
    assert.equal(shared.interactivePlot, original);
    assert(nodes.get("#modal").open);
    assert.equal(nodes.get("#plot-area").children.length, 0);
    nodes.get("#modal").close();
    assert(original.destroyed);
    assert(shared.interactivePlot instanceof Viewer);
    assert.equal(nodes.get("#figure-tools").parent, parent);
    passed("Enlarge moves the same viewer and close disposes/restores panel controls");
    nodes.get("#figure-tabs").children[1].onclick();
    const second = latest();
    assert(second.url.includes("figure-2.json"));
    nodes.get("#figure-tabs").children[0].onclick();
    second.resolve(payload(jobA, 2));
    await settle();
    assert.equal(caption(), "first");
    nodes.get("#figure-tabs").children[1].onclick();
    assert.notEqual(latest(), second);
    latest().resolve(payload(jobA, 2));
    await settle();
    assert.equal(caption(), "second");
    passed("stale tab response cannot replace active figure and is retryable");
    await registry.updateFigures(manifest(jobB));
    const oldRequest = latest();
    const jobC = "c".repeat(32);
    await registry.updateFigures(manifest(jobC));
    const currentRequest = latest();
    currentRequest.resolve(payload(jobC));
    await settle();
    const current = shared.interactivePlot;
    oldRequest.resolve(payload(jobB));
    await settle();
    assert.equal(shared.interactivePlot, current);
    assert.equal(shared.figureData[0].source.job, jobC);
    passed("per-request revision rejects older job in the same UI generation");
    for (const [name, data, reason] of [
      ["unsupported", payload(jobA, 1, "transparency"), "transparency"],
      ["unknown version", { version: 99 }, "unknown_version"],
      ["invalid indices", (() => { const data = payload(jobA); data.axes[0].series[0].source_indices[1] = 99999; return data; })(), "invalid_index"],
      ["wrong source job", payload(jobB), "invalid_data"],
      ["wrong source ordinal", payload(jobA, 8), "invalid_data"]
    ]) {
      await registry.updateFigures(manifest(jobA, [name]));
      const constructors = created.length;
      latest().resolve(data);
      await settle();
      assert.equal(created.length, constructors);
      assert(caption().endsWith("reason:" + reason));
      assert.equal(nodes.get("#plot-area").children[0].tag, "img");
      passed(name + " is validated/bound before renderer and keeps reasoned PNG");
    }
    await registry.updateFigures(manifest(jobA, ["HTTP cap"]));
    latest().reject(Object.assign(new Error("too large"), { details: { reason_code: "json_budget", reason_args: { limit: 8388608 } } }));
    await settle();
    assert(caption().endsWith("reason:json_budget"));
    passed("HTTP 413 machine-readable reason survives the API error");
    const sparse = manifest(jobB, ["ordinal gap"]);
    sparse.figures[0].file = "figure-3.png";
    sparse.figures[0].data_file = "figure-3.json";
    await registry.updateFigures(sparse);
    latest().resolve(payload(jobB, 3));
    await settle();
    assert(shared.interactivePlot instanceof Viewer);
    assert.equal(shared.interactivePlot.options.identity.figure, 3);
    passed("identity uses export file ordinal, never Octave handle or array offset");
    await registry.updateFigures(manifest(jobA, ["v2"]));
    const legacy = fixture("line2d");
    assert.equal(legacy.version, 2);
    latest().resolve(legacy);
    await settle();
    assert(shared.interactivePlot instanceof registry.InteractiveFigure);
    assert.equal(shared.figureData[0].version, 2);
    passed("pure 2D v2 retains the existing Canvas path");
    await registry.updateFigures(manifest(jobB, ["generation"]));
    const generationRequest = latest();
    shared.uiGeneration++;
    generationRequest.resolve(payload(jobB));
    await settle();
    assert.equal(shared.interactivePlot, null);
    passed("UI-generation check rejects reset response");
    const png1 = deferred();
    const png2 = deferred();
    let pngCalls = 0;
    registry.blobAPI = () => (++pngCalls === 1 ? png1.promise : png2.promise);
    const oldManifest = registry.updateFigures(manifest(jobA, ["old PNG"]));
    const newManifest = registry.updateFigures(manifest(jobB, ["new PNG"]));
    const pendingRequests = requests.length;
    registry.renderFigures();
    assert.equal(requests.length, pendingRequests, "pending manifest fetched old active JSON");
    assert.equal(shared.interactivePlot, null, "pending manifest resurrected an old viewer");
    png2.resolve("blob:new");
    await newManifest;
    latest().resolve(payload(jobB));
    await settle();
    png1.resolve("blob:old");
    await oldManifest;
    assert.equal(shared.figures[0].name, "new PNG");
    assert.deepEqual(shared.figureURLs, ["blob:new"]);
    assert(revoked.includes("blob:old"));
    passed("manifest PNG race blocks old view, rejects older result and revokes stale URLs");
    shared.interactivePlot?.destroy();
    console.log(`${count} figure integration cases passed. DOM/viewer boundaries are mocked; browser behavior remains unverified.`);
  } finally {
    URL.revokeObjectURL = savedRevoke;
    for (const key of ["figureHarness", "document", "devicePixelRatio", "getComputedStyle", "ResizeObserver", "MutationObserver"]) delete global[key];
  }
})().catch((error) => { console.error(error); process.exit(1); });
