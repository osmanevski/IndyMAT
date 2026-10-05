const assert = require("node:assert/strict");
const settings = require("../frontend/shortcut_registry_utils.cjs");
const { sectionRange } = require("../frontend/editor_command_utils.cjs");
assert.equal(settings.cloneDefaults().preferences.adaptEditorLiterals, false);
assert.equal(settings.sanitizeSettings({ version: 1, preferences: { adaptEditorLiterals: true } }, [], true).preferences.adaptEditorLiterals, true);
assert.equal(settings.sanitizeSettings({ version: 1, preferences: { adaptEditorLiterals: "true" } }, [], true).preferences.adaptEditorLiterals, false);
const source = '%% first\nr="tail\\";\n%% next\ns="next";';
assert.equal(sectionRange(source, 12, false, "matlab").nextFrom, source.indexOf("%% next"));
assert.equal(sectionRange(source, 12, false, "matlab").to, source.indexOf("\n%% next"));
assert.equal(sectionRange(source, 12, true, "matlab").to, source.length);
// Exercise transport/transparency without a network server or browser engine.
const vm = require("node:vm");
const fs = require("node:fs");
class Element {
  constructor(tag, className = "", text = "") { this.tag = tag; this.className = className; this.textContent = text; this.children = []; this.dataset = {}; }
  append(child) { child.parent = this; this.children.push(child); }
  after(child) { child.parent = this.parent; this.parent.children.splice(this.parent.children.indexOf(this) + 1, 0, child); }
  remove() { this.parent.children.splice(this.parent.children.indexOf(this), 1); }
  querySelector(selector) { return this.children.find((child) => child.className === selector.slice(1)) || null; }
  setAttribute(name, value) { this[name] = value; }
  focus() {}
  setSelectionRange(start, end) { this.selection = [start, end]; }
}
let enabled = false, dispatched, modal;
const document = '% 😀\nr="a"+"b";';
const tab = { path: "/work/dirty.m", content: document };
const shared = { active: tab, tabs: [tab], uiGeneration: 3,
  editor: { source: document, state: { doc: { toString: () => shared.editor.source } }, dispatch: (value) => { dispatched = value; }, focus() {} } };
const registry = { getSetting: () => enabled, el: (...args) => new Element(...args), switchTab() {}, updateCursor() {},
  modal: (title, node) => { modal = { title, node }; } };
const context = vm.createContext({ shared, registry, Date, t: (key, params = {}) => key.replace(/\{(\w+)\}/g, (_, name) => params[name]), onLanguageChange() {} });
vm.runInContext(fs.readFileSync(require.resolve("../frontend/source_adapter.js"), "utf8").replace(/^import .*;\n/gm, "").replace(/export function /g, "function "), context);
assert.equal(context.editorSourceContext("editor-selection", document, 0, document.length, 0), null);
enabled = true;
const snapshot = context.editorSourceContext("editor-selection", document, 0, document.length, 0);
assert.equal(snapshot.document, document);
assert.equal(snapshot.path, tab.path);
assert.equal(snapshot.profile, "matlab");
const wrap = new Element("div"), command = new Element("div", "console-command");
wrap.append(command);
const entry = { wrap, sourceContext: snapshot, job: "accepted", epoch: 2 };
const metadata = { status: "adapted", diagnostics: [], replacements: ['"a"','"b"'].map((text) => ({ original_text: text, original: { start_utf16: document.indexOf(text) } })) };
context.showAdaptation(entry, metadata);
assert.equal(wrap.querySelector(".source-adapter-note").textContent, "Adapted: 2 literals");
assert.equal(wrap.querySelector(".source-adapter-note").title, "Original lines: 2");
context.showAdaptation(entry, { status: "fallback", diagnostics: [{ code: "quoted-command", message: "command reason" }] });
assert.equal(wrap.querySelector(".source-adapter-note").textContent, "Executed unchanged: Quoted command-form arguments execute unchanged.");
assert.equal(wrap.querySelector(".source-adapter-note").title, "Original lines: 1\ncommand reason");
const location = { job: "accepted", epoch: 2, line: 2, column: 3, start_utf16: document.indexOf('"a"'), end_utf16: document.indexOf('"a"') + 3 };
context.showSourceErrors(entry, { job: "stale", source_adapter: { epoch: 2 }, source_error_locations: [location] });
assert.equal(wrap.querySelector(".source-error-locations"), null);
context.showSourceErrors(entry, { job: "accepted", source_adapter: { epoch: 2 }, source_error_locations: [location] });
const link = wrap.querySelector(".source-error-locations").children[0];
assert.equal(link.textContent, "Original source: line 2, column 3");
link.onclick();
assert.equal(dispatched.selection.head, location.start_utf16);
shared.editor.source = 'r="changed";';
link.onclick();
assert.equal(modal.node.readOnly, true);
assert.equal(modal.node.value, document);
assert.deepEqual(modal.node.selection, [location.start_utf16, location.end_utf16]);
console.log("SOURCE ADAPTER UNIT PASS: 20 assertions; strict opt-in/schema, MATLAB sections, notes, stale jobs and original/historical source links.");
