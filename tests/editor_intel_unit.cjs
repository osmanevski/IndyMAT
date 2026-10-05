const assert = require("node:assert/strict");
const { identifierAt, occurrencesInSource, parseEditorSymbols, variableDefinitionInSource } = require("../frontend/editor_intel_utils.cjs");

const source = `%% Başlangıç
value = local_fn(1);
text = "function fake(x)";
% %% sahte
% function ignored(x)
%{
%% kapalı
function blocked(x)
%}
function [out, other] = local_fn(value)
out = value + local_fn(value');
end`;
const parsed = parseEditorSymbols(source);
assert.deepEqual(parsed.sections.map((item) => [item.name, item.line]), [["Başlangıç", 1]]);
assert.deepEqual(parsed.functions.map((item) => [item.name, item.line]), [["local_fn", 10]]);
assert.equal(parseEditorSymbols("%%\n", (source) => source)["sections"][0].name, "Untitled section");
assert.equal(parseEditorSymbols("%%\n", (source) => ({ "Untitled section": "Adsız bölüm" })[source])["sections"][0].name, "Adsız bölüm");
assert.equal(identifierAt("alpha = beta;", 9), "beta");
assert.equal(identifierAt("alpha = beta;", 0), "alpha");
assert.equal(identifierAt("alpha = beta;", 6), "");
assert.deepEqual(occurrencesInSource(source, "local_fn").map((item) => [item.line, item.column]), [[2, 9], [10, 25], [11, 15]]);
assert.deepEqual(occurrencesInSource(source, "fake"), []);
assert.deepEqual(variableDefinitionInSource("x = 1; y = x + 2;\nfor index = 1:3", "y"), { kind: "variable", name: "y", line: 1, column: 8, from: 7 });
assert.equal(variableDefinitionInSource("[left, right] = deal(1);", "right").line, 1);
assert.equal(variableDefinitionInSource("for index = 1:3", "index").column, 5);
assert.equal(variableDefinitionInSource("function out = sample(inputValue)\nout = inputValue;\nend", "inputValue").line, 1);
assert.equal(variableDefinitionInSource("function [first, second] = sample(inputValue)", "second").line, 1);
assert.equal(variableDefinitionInSource("if x == 1\n disp(x)\nend", "x"), null);
assert.equal(variableDefinitionInSource("text = 'z = 2'; % z = 3", "z"), null);
console.log("EDITOR INTEL UNIT PASS: lexical outline, function detection, cursor words and exact occurrences.");

Object.defineProperty(globalThis, "navigator", { configurable: true, value: { platform: "MacIntel", userAgent: "", vendor: "" } });
const { EditorState } = require("@codemirror/state");
const { keymap, runScopeHandlers } = require("@codemirror/view");
const { editorIntelBindings } = require("../frontend/editor_intel_shortcuts.cjs");
const invoked = [];
let dialogOpen = false;
const shortcutRegistry = {
  safe: (fn) => fn(),
  goToDefinition: () => invoked.push("definition"),
  findOccurrences: () => invoked.push("occurrences")
};
const shortcutView = { state: EditorState.create({ extensions: [keymap.of(editorIntelBindings(shortcutRegistry, { querySelector: () => dialogOpen }))] }) };
for (const layout of ["US", "Turkish Q"]) {
  for (const [key, keyCode, modifiers, expected] of [
    ["F12", 123, {}, "definition"],
    ["F12", 123, { shiftKey: true }, "occurrences"],
    ["F7", 118, { altKey: true }, "definition"],
    ["F8", 119, { altKey: true }, "occurrences"]
  ]) {
    const event = { key, keyCode, ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...modifiers };
    invoked.length = 0;
    assert.equal(runScopeHandlers(shortcutView, event, "editor"), true, layout + " " + key);
    assert.deepEqual(invoked, [expected]);
    assert.equal(runScopeHandlers(shortcutView, event, "foreign-input"), false);
    dialogOpen = true;
    assert.equal(runScopeHandlers(shortcutView, event, "editor"), false);
    dialogOpen = false;
    assert.deepEqual(invoked, [expected], "foreign scope or dialog invoked a command");
  }
}
console.log("EDITOR INTEL KEYMAP PASS: actual CodeMirror dispatch for F12, Shift-F12 and Alt-F7/F8; editor/dialog scopes.");

// Execute the production modules with a small DOM adapter and controlled RAF /
// API promises: no browser or fake Octave computation is needed for panel races.
const vm = require("node:vm");
const esbuild = require("esbuild");
class Node {
  constructor(text = "") {
    this.textContent = text;
    this.children = [];
    this.hidden = false;
    this.dataset = {};
    this.classList = { toggle() {}, remove() {} };
  }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  setAttribute() {}
}

(async () => {
  const bundled = esbuild.buildSync({
    stdin: { contents: 'import shared from "./frontend/state.js"; import registry from "./frontend/registry.js"; import { setLanguage } from "./frontend/i18n.js"; import "./frontend/symbols.js"; import "./frontend/outline.js"; globalThis.testIntel = { shared, registry, setLanguage };', resolveDir: require("node:path").resolve(__dirname, "..") },
    bundle: true, write: false, platform: "node", format: "cjs"
  }).outputFiles[0].text;
  const nodes = new Map();
  const node = (selector) => {
    if (!nodes.has(selector)) nodes.set(selector, new Node());
    return nodes.get(selector);
  };
  let nextFrame = 0;
  const frames = new Map();
  const context = vm.createContext({
    document: { documentElement: { lang: "" }, matches: () => false, querySelectorAll: () => [] },
    requestAnimationFrame: (fn) => { frames.set(++nextFrame, fn); return nextFrame; },
    cancelAnimationFrame: (id) => frames.delete(id)
  });
  vm.runInContext(bundled, context);
  const { shared, registry, setLanguage } = context.testIntel;
  setLanguage("tr");
  registry.$ = node;
  registry.el = (tag, cls, text) => { const result = new Node(text); result.className = cls; return result; };
  const localSource = "localValue = 1;\nresult = localValue + 2;";
  const select = (source, head) => shared.editor = { state: EditorState.create({ doc: source, selection: { anchor: head } }) };
  const label = () => node("#editor-intel-content").children.filter((item) => item.className === "editor-intel-label");
  const rows = () => node("#editor-intel-content").children.filter((item) => item.className === "editor-intel-row");
  const flush = () => { const pending = [...frames.values()]; frames.clear(); for (const fn of pending) fn(); };
  shared.currentFolder = "/test";
  shared.active = { path: "/test/yeni_1.m", hash: null };
  select(localSource, localSource.lastIndexOf("localValue"));
  registry.api = () => { throw new Error("local variable lookup must not await the folder index"); };
  registry.setPanelView("outline");
  assert.equal(label()[0].textContent, "Etkin dosya anahattı");
  setLanguage("en");
  assert.equal(label()[0].textContent, "Outline for the active file", "open panel should rerender in English");
  setLanguage("tr");
  assert.equal(label()[0].textContent, "Etkin dosya anahattı", "open panel should rerender in Turkish");
  registry.scheduleEditorIntel(); // A queued pre-search draw previously cleared results.
  await registry.findOccurrences();
  flush();
  assert.equal(label().length, 1);
  assert.equal(label()[0].textContent, "Yalnız etkin dosya");
  assert.equal(rows().length, 2);
  shared.active.lintState = { kind: "ok" };
  registry.scheduleEditorIntel();
  flush();
  assert.equal(label()[0].textContent, "Yalnız etkin dosya");
  assert.equal(rows().length, 2);
  shared.active.hash = "saved";
  await registry.findOccurrences();
  assert.equal(label()[0].textContent, "Yalnız etkin dosya", "saved and Yeni buffers have the same local scope");

  select("folder_fn(3);", 0);
  let answerIndex;
  let answerUses;
  registry.api = (route) => new Promise((resolve) => {
    if (route === "symbols") answerIndex = resolve;
    else answerUses = resolve;
  });
  const search = registry.findOccurrences();
  registry.scheduleEditorIntel();
  flush();
  assert.equal(label()[0].textContent, "Kullanımlar aranıyor…");
  assert.equal(rows().length, 0, "pending search must not leave old outline/usage rows");
  answerIndex({ folder: "/test", functions: [{ name: "folder_fn", path: "/test/folder_fn.m", line: 1 }] });
  for (let turn = 0; turn < 10 && !answerUses; turn++) await Promise.resolve();
  assert(answerUses, "function search should request folder occurrences");
  answerUses({ occurrences: [{ path: "/test/folder_fn.m", line: 1, column: 14 }], truncated: false });
  await search;
  registry.scheduleEditorIntel();
  flush();
  assert.equal(label()[0].textContent, "Etkin dosya ve geçerli klasör");
  assert.equal(rows().length, 2);

  const staleSearch = registry.findOccurrences();
  for (let turn = 0; turn < 10; turn++) await Promise.resolve();
  shared.active = { path: "/test/new.m" };
  select("x = 1;", 0);
  registry.scheduleEditorIntel();
  flush();
  answerUses({ occurrences: [], truncated: false });
  await staleSearch;
  assert.equal(rows().length, 0, "late response must not repopulate a different buffer");
  assert.equal(label().length, 0);
  console.log("EDITOR INTEL PANEL PASS: queued redraw/lint, pending API, saved/unsaved scopes, one label and stale response fencing.");
})().catch((error) => { console.error(error); process.exitCode = 1; });
