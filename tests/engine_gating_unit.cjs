// state.unsupported hides/disables UI entry points; an absent or
// empty list must leave every feature exactly as before. Runs the real
// frontend sources in vm contexts with a minimal DOM (no browser, no server).
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const i18n = require("../frontend/i18n_utils.cjs");

const root = path.resolve(__dirname, "..");
const t = (source, params) => i18n.translate(source, params);
const NOT_SUPPORTED = "This feature is not supported by the Rust engine yet.";

class Element {
  constructor(tag = "div") {
    this.tagName = tag;
    this.children = [];
    this.dataset = {};
    this.attributes = {};
    this.style = {};
    this.className = "";
    this.hidden = false;
    this.disabled = false;
    this.value = "";
    this.textContent = "";
    this.innerHTML = "";
    const classes = new Set();
    this.classList = {
      toggle: (name, on) => { if (on === undefined ? !classes.has(name) : on) classes.add(name); else classes.delete(name); },
      add: (name) => classes.add(name),
      remove: (...names) => names.forEach((name) => classes.delete(name)),
      contains: (name) => classes.has(name)
    };
  }
  append(...nodes) { for (const node of nodes) { node.parent = this; this.children.push(node); } }
  replaceChildren(...nodes) { this.children = []; this.append(...nodes); }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  addEventListener() {}
  focus() {}
  select() {}
  close() { this.open = false; }
}

const nodes = new Map();
const node = (selector) => {
  if (!nodes.has(selector)) nodes.set(selector, new Element());
  return nodes.get(selector);
};
const document = {
  body: new Element("body"),
  querySelectorAll: (selector) => nodes.has(selector) ? [nodes.get(selector)] : [],
  querySelector: () => null
};

const shared = {
  engine: { status: "idle", epoch: 1 }, unsupported: new Set(), unsupportedKey: "[]", busy: false, starting: false,
  breakpoints: new Map(), breakpointKey: "", debugPanelKey: "", debugSnapshotKey: "", uiGeneration: 1, lastEpoch: null,
  publishRenders: new Map(), variables: [], figures: [], figureIndex: 0, lintSequence: 0, tabs: []
};
const calls = [], toasts = [], shortcuts = [], modals = [];
let state;
const registry = {
  $: node,
  el: (tag, cls, text) => Object.assign(new Element(tag), { className: cls || "", textContent: text === undefined ? "" : text }),
  api: async (route, payload) => {
    calls.push({ route, payload });
    return route === "state" ? state : { job: "job-" + calls.length };
  },
  toast: (text) => toasts.push(text),
  safe: (fn) => Promise.resolve().then(fn).catch((error) => toasts.push(error.message)),
  on: () => {},
  requireIdle: () => {},
  setStatus: () => {},
  refreshDebugEditor: () => {},
  renderVariables: () => {},
  openDebugLocation: async () => {},
  registerShortcut: (definition) => shortcuts.push(definition),
  modal: (title, content) => modals.push({ title, content })
};

// Strip imports/exports; real .cjs helpers are required, every other imported
// binding (CodeMirror, sibling modules) becomes an inert stub.
function stub() {
  const fn = function () {};
  return new Proxy(fn, {
    get: (target, key) => key === "prototype" ? target.prototype : key === "toString" || key === "valueOf" ? () => "" : typeof key === "symbol" ? undefined : stub(),
    apply: () => stub(),
    construct: () => stub()
  });
}
function load(file, extra = {}, expose = "") {
  let source = fs.readFileSync(path.join(root, "frontend", file), "utf8");
  const globals = { shared, registry, document, t, tr: t, console, CSS: { escape: (value) => value }, getLanguage: () => "en", onLanguageChange: () => () => {}, requestAnimationFrame: () => 0, cancelAnimationFrame: () => {}, confirm: () => true, prompt: () => null };
  for (const [, clause, from] of source.matchAll(/^import\s+(.+?)\s+from\s+"([^"]+)";$/gm)) {
    const names = [];
    const braces = clause.match(/\{([^}]*)\}/);
    if (braces) for (const part of braces[1].split(",").map((item) => item.trim()).filter(Boolean)) names.push(part.split(/\s+as\s+/).pop());
    const head = clause.replace(/\{[^}]*\}/, "").replace(/,/g, "").trim();
    if (head) names.push(head);
    for (const name of names) {
      if (name in globals) continue;
      globals[name] = from.endsWith(".cjs") ? require(path.join(root, "frontend", from)) : stub();
    }
  }
  source = source.replace(/^import .*;\n/gm, "").replace(/^export .*;\n/gm, "") + "\n" + expose;
  vm.runInNewContext(source, { ...globals, ...extra });
}

// Only core's unsupported()/requireSupported() are under test; keep the DOM stubs.
const stubs = { ...registry };
load("core.js", { localStorage: { getItem: () => null }, fetch: async () => ({}) });
Object.assign(registry, stubs);
load("engine_gating.js");
load("debugger.js");
load("poll.js", { showAdaptation: () => {}, showSourceErrors: () => {} });
load("console.js", {}, "registry.testSetStatus = setStatus;");
load("workspace.js");
load("variable_editor.js", {}, "registry.testVariable = { writeAllowed, beginCellEdit, renderText };");
load("dialogs.js");
load("bootstrap.js", { setInterval: () => 0 });
load("editor.js", { gutter: (spec) => spec });
load("outline.js");
load("figures.js");
// Feature modules export their own modal/setStatus/...; tests drive the stubs.
for (const key of ["$", "el", "api", "toast", "safe", "on", "requireIdle", "setStatus", "refreshDebugEditor", "renderVariables", "openDebugLocation", "registerShortcut", "modal"]) registry[key] = stubs[key];
registry.setupVariableSearch();

const ALL = ["debugger", "breakpoints", "run_to_cursor", "packages", "profiler", "publish", "mat_files", "workspace_edit", "variable_editor_write", "figure_rotate", "help", "lint"];
const gate = (list) => registry.setUnsupported(list);
const editableVariable = { name: "a", size: "1x1", class: "double", preview: "1", bytes: 8 };
const matrixTab = () => ({ name: "a", path: [], data: { editable: true, real: true, class: "double", kind: "matrix", row: 1, column: 1 } });
const textTab = () => ({ name: "s", path: [], data: { editable: true, real: true, class: "char", kind: "char-text", text: "hi" } });
const rejects = async (fn, message = NOT_SUPPORTED) => assert.rejects(async () => fn(), (error) => error.message === message);

function assertUngated(label) {
  assert.equal(registry.unsupported("lint"), false, label);
  for (const selector of ["#run-profile", "#publish", ".run-more", "#run-to-cursor", '[data-action="packages"]']) assert.equal(node(selector).hidden, false, `${label}: ${selector} visible`);
  assert.equal(document.body.classList.contains("breakpoints-unsupported"), false, label);
  shared.variables = [editableVariable];
  registry.updateWorkspaceActions("idle");
  for (const id of ["#workspace-edit-value", "#workspace-save-all", "#workspace-save-selection", "#workspace-load"]) assert.equal(node(id).hidden, false, `${label}: ${id} visible`);
  assert.equal(node("#workspace-save-all").disabled, false, label);
  shared.figures = [{ number: 1 }];
  registry.updateRotateControls();
  assert.equal(node("#rotate-left").hidden, false, label);
  assert.equal(node("#rotate-left").disabled, false, label);
  const cell = new Element("td");
  registry.testVariable.writeAllowed(matrixTab());
  const wrap = new Element();
  registry.testVariable.renderText(wrap, textTab());
  assert.equal(wrap.children[0].tagName, "form", `${label}: char text stays editable`);
  registry.help();
  assert.equal(modals.at(-1).content.children[0].tagName, "form", `${label}: help search offered`);
  assert.match(modals.at(-1).content.children[1].innerHTML, /F1/);
  assert.equal(shortcuts.find((item) => item.id === "global.documentation").when(), true, label);
  registry.updateLintCaption();
  assert.notEqual(node("#code-issues").textContent, "Syntax check unavailable", label);
  return cell;
}

(async () => {
  // 1. Absent / empty list: nothing gated, no gating pass at all for the absent field.
  assert.equal(gate(undefined), false, "absent field is the initial empty state");
  assert.equal(gate([]), false);
  assertUngated("absent");

  // 2. Everything unsupported.
  assert.equal(gate(ALL), true);
  assert.equal(gate([...ALL]), false, "unchanged list does not re-apply");
  for (const feature of ALL) assert.equal(registry.unsupported(feature), true, feature);
  assert.throws(() => registry.requireSupported("help"), { message: NOT_SUPPORTED });
  for (const selector of ["#run-profile", "#publish", ".run-more", "#run-to-cursor", '[data-action="packages"]']) assert.equal(node(selector).hidden, true, selector + " hidden");
  assert.equal(document.body.classList.contains("breakpoints-unsupported"), true);

  // Debugger: panel and controls stay hidden even with breakpoints or a pause.
  shared.breakpoints = new Map([["/w/a.m", new Map([[3, { line: 3, enabled: true, condition: "" }]])]]);
  shared.debugPanelKey = "";
  registry.renderDebuggerPanel({ status: "idle" });
  assert.equal(node("#debugger-panel").hidden, true, "debugger panel hidden");
  registry.testSetStatus({ status: "paused", debug: { ready: true } });
  assert.equal(node("#debug-controls").hidden, true, "debug controls hidden");
  calls.length = 0;
  await rejects(() => registry.debugCommand("step"));
  await rejects(() => registry.toggleBreakpoint(3));
  await rejects(() => registry.editBreakpointCondition(3, "/w/a.m"));
  await rejects(() => registry.runToCursor());
  await rejects(() => registry.runProfile());
  await rejects(() => registry.publishFile());
  await rejects(() => registry.packages());
  await rejects(() => registry.packageSession("load", "signal"));
  await rejects(() => registry.showDocumentation("fft"));
  await rejects(() => registry.rotate(15));
  assert.equal(calls.length, 0, "gated features never reach the server");

  // Help: modal keeps shortcuts/engine notes, drops the lookup form and F1 row; F1 is not consumed.
  registry.help();
  assert.equal(modals.at(-1).content.children.length, 1);
  assert.doesNotMatch(modals.at(-1).content.children[0].innerHTML, /<kbd>F1<\/kbd>/);
  assert.equal(shortcuts.find((item) => item.id === "global.documentation").when(), false);

  // Workspace: scalar edit and MAT save/load hidden; rename/delete untouched.
  shared.variables = [editableVariable];
  registry.updateWorkspaceActions("idle");
  for (const id of ["#workspace-edit-value", "#workspace-save-all", "#workspace-save-selection", "#workspace-load"]) assert.equal(node(id).hidden, true, id + " hidden");
  assert.equal(node("#workspace-edit-value").disabled, true);
  assert.equal(node("#workspace-delete").hidden, false, "delete still offered");

  // Variables editor: cells are read-only, double-click does nothing.
  const cell = new Element("td");
  assert.equal(registry.testVariable.beginCellEdit(matrixTab(), cell, 0, 0, "1"), undefined);
  await Promise.resolve();
  assert.equal(cell.children.length, 0, "no inline editor opened");
  assert.throws(() => registry.testVariable.writeAllowed(matrixTab()), { message: "This view is read-only." });
  const wrap = new Element();
  registry.testVariable.renderText(wrap, textTab());
  assert.equal(wrap.children[0].tagName, "pre", "char text shown read-only");

  // Figures: PNG rotate buttons hidden and disabled.
  shared.figures = [{ number: 1 }];
  registry.updateRotateControls();
  assert.equal(node("#rotate-left").hidden, true);
  assert.equal(node("#rotate-right").disabled, true);

  // Editor: gutter clicks are inert; lint never calls the server and says so.
  const gutter = registry.breakpointGutter.domEventHandlers, toggled = [];
  const realToggle = registry.toggleBreakpoint, realCondition = registry.editBreakpointCondition;
  registry.toggleBreakpoint = registry.editBreakpointCondition = async (line) => toggled.push(line);
  const gutterView = { state: { doc: { lineAt: () => ({ number: 4 }) } } };
  const click = (button = 0) => ({ button, shiftKey: false, prevented: false, preventDefault() { this.prevented = true; } });
  let event = click();
  assert.equal(gutter.mousedown(gutterView, { from: 0 }, event), false, "gutter click not handled");
  assert.equal(event.prevented, false);
  event = click();
  assert.equal(gutter.contextmenu(gutterView, { from: 0 }, event), false, "gutter context menu not handled");
  await Promise.resolve();
  assert.equal(toggled.length, 0, "no breakpoint toggled");
  shared.active = { path: "/w/a.m", hash: "h", dirty: false, content: "x = 1;" };
  calls.length = 0;
  const view = { state: { doc: { toString: () => "x = 1;" } } };
  assert.equal((await registry.lintSource(view)).length, 0);
  assert.equal(calls.length, 0, "no lint request");
  assert.equal(shared.active.lintState.kind, "disabled");
  assert.equal(node("#code-issues").textContent, "Syntax check unavailable");
  shared.active.lintState = { kind: "checking", issues: [] };
  registry.updateLintCaption();
  assert.equal(node("#code-issues").textContent, "Syntax check unavailable", "caption never shows Checking…");

  // 3. Back to an empty list: everything is offered again.
  assert.equal(gate([]), true);
  shared.debugPanelKey = "";
  registry.renderDebuggerPanel({ status: "idle" });
  assert.equal(node("#debugger-panel").hidden, false, "panel back with breakpoints");
  registry.testSetStatus({ status: "paused", debug: { ready: true } });
  assert.equal(node("#debug-controls").hidden, false);
  assertUngated("empty");
  event = click();
  assert.equal(gutter.mousedown(gutterView, { from: 0 }, event), true, "gutter click toggles again");
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(toggled[0], 4);
  registry.toggleBreakpoint = realToggle;
  registry.editBreakpointCondition = realCondition;

  // 4. Poll stores the list and shows the engine version without a prefix.
  state = { status: "idle", job: null, epoch: 1, variables: [], unsupported: ["publish"], engine: { name: "indy", stub: false }, version: "IndyMAT Rust engine 0.2" };
  registry.renderDebuggerPanel = () => {};
  registry.updateFigures = async () => {};
  registry.refreshFiles = async () => {};
  await registry.poll();
  assert.equal(registry.unsupported("publish"), true);
  assert.equal(registry.unsupported("help"), false);
  assert.equal(node("#publish").hidden, true);
  assert.equal(node("#run-profile").hidden, false);
  assert.equal(node(".run-more").hidden, false, "More menu kept while Run and Time remains");
  assert.equal(node("#engine-version").textContent, "Rust engine (experimental)");
  assert.equal(document.body.classList.contains("engine-rust"), true);
  delete state.unsupported;
  delete state.engine;
  await registry.poll();
  assert.equal(registry.unsupported("publish"), false, "absent field ungates");
  assert.equal(node("#publish").hidden, false);
  assert.equal(node("#engine-version").textContent, "GNU Octave IndyMAT Rust engine 0.2", "no state.engine means the Octave label, as before");
  assert.equal(document.body.classList.contains("engine-rust"), false);

  // A poll sent during a switch answers for the kernel that was closing: the generation moves again after it.
  const gating = fs.readFileSync(path.join(root, "frontend/engine_gating.js"), "utf8");
  assert.match(gating, /await registry\.api\("engine", \{ engine: value, confirm: true \}\);\s*(\/\/[^\n]*\n\s*)?shared\.uiGeneration\+\+;/);

  // 5. Each new English source has exactly one Turkish text.
  const locale = (file) => vm.runInNewContext("(" + fs.readFileSync(path.join(root, "frontend/locales", file), "utf8").replace(/^\s*\/\/[^\n]*\n/, "").replace(/^export default\s*/, "").replace(/;\s*$/, "") + ")");
  assert.equal(locale("tr.js")[NOT_SUPPORTED], "Bu özellik Rust motorunda henüz desteklenmiyor.");
  assert.equal(locale("tr_editor.js")["Syntax check unavailable"], "Sözdizimi denetimi kullanılamıyor");
  console.log("ENGINE GATING UNIT PASS: absent/empty list gates nothing; debugger, breakpoints, run to cursor, profile, publish, packages, help/F1, MAT save/load, scalar edit, variable-editor writes, PNG rotate and lint gated without server calls; poll stores the list; engine version unprefixed.");
})().catch((error) => { console.error(error); process.exitCode = 1; });
