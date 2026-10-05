// Full 1 MB renderer benchmark. DOM nodes are lightweight stand-ins; browser
// layout/paint is measured separately by ui_command_window.cjs. --before PATH
// benchmarks the saved pre-fix renderer, with a 10 s interrupt per input.
const fs = require("node:fs");
const vm = require("node:vm");
const assert = require("node:assert/strict");
const { performance } = require("node:perf_hooks");
const args = process.argv.slice(2);
const before = args[0] === "--before";
const source = fs.readFileSync(before ? args[1] : "frontend/command_window.js", "utf8").replace(/^import .*;\n/gm, "");
const utils = require("../frontend/command_window_utils.cjs");
class Node {
  constructor(text = "") { this.text = text; this.children = []; this.dataset = {}; this.value = ""; this.isConnected = true; this.classList = { contains: () => false }; }
  get textContent() { return this.text + this.children.map((node) => node.textContent).join(""); }
  set textContent(value) { this.text = value; this.children = []; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.text = ""; this.children = nodes; }
}
for (const [name, raw] of [["dense", "a".repeat(1_000_000)], ["20000 matches", ("a" + ".".repeat(49)).repeat(20000)]]) {
  let created = 0;
  const output = new Node(raw), controls = new Map();
  for (const id of ["#console-search", "#console-match-count", "#console-search-previous", "#console-search-next"]) controls.set(id, new Node());
  controls.set("#console", { children: [{ get textContent() { return output.textContent; } }] });
  controls.get("#console-search").value = "a";
  const registry = { $: (id) => controls.get(id), el: () => { created++; return new Node(); } };
  const document = { querySelectorAll: () => [output], createDocumentFragment: () => new Node(), createTextNode: (text) => { created++; return new Node(text); } };
  // No location exists in either fixture. Skipping the old parser isolates the
  // reported quadratic decoration loop (and gives the old code an advantage).
  const commandWindowUtils = before ? { ...utils, parseErrorLocations: () => [] } : utils;
  const context = vm.createContext({ registry, document, commandWindowUtils, t: (source) => source, onLanguageChange: () => () => {}, shared: {}, setTimeout, clearTimeout, TextEncoder, TextDecoder });
  vm.runInContext(source, context);
  const start = performance.now();
  let timedOut = false;
  try { vm.runInContext("registry.renderOutputSearch()", context, { timeout: 10000 }); }
  catch (error) { if (error.code === "ERR_SCRIPT_EXECUTION_TIMEOUT") timedOut = true; else throw error; }
  const elapsed = performance.now() - start;
  console.log(JSON.stringify({ renderer: before ? "before" : "after", input: name, bytes: Buffer.byteLength(raw), ms: Number(elapsed.toFixed(2)), timedOut, nodesCreated: created }));
  if (!before) {
    assert(!timedOut);
    assert(created <= 3100, "decoration DOM must stay bounded");
    assert.equal(output.textContent, raw);
    assert.equal(controls.get("#console-match-count").textContent, "1 / 1000+");
  }
}
