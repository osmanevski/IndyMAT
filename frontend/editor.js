import { basicSetup } from "codemirror";
import { EditorView, Decoration, ViewPlugin, gutter, GutterMarker } from "@codemirror/view";
import { EditorState, Compartment, RangeSet, RangeSetBuilder, StateEffect, StateField } from "@codemirror/state";
import { StreamLanguage, HighlightStyle, syntaxHighlighting, foldService } from "@codemirror/language";
import { undo, redo } from "@codemirror/commands";
import { tags } from "@lezer/highlight";
import { octave } from "@codemirror/legacy-modes/mode/octave";
import { autocompletion } from "@codemirror/autocomplete";
import { linter, lintGutter } from "@codemirror/lint";

import shared from "./state.js";
import registry from "./registry.js";
import editorCommandUtils from "./editor_command_utils.cjs";
import { t as tr, getLanguage, onLanguageChange } from "./i18n.js";

const lexicalDocuments = new WeakMap();
function editorLexicalLines(doc) {
  if (!lexicalDocuments.has(doc)) lexicalDocuments.set(doc, editorCommandUtils.scanEditorSource(doc.toString()));
  return lexicalDocuments.get(doc);
}

const themeCompartment = new Compartment();
const words = "abs acos asin atan atan2 axis bar barh bode butter cell ceil cholesky clear clc close complex conj conv cos csvread csvwrite det diag diff disp eig exp eye fft fftshift figure filter find fix floor for fprintf freqz grid hist histogram hold if ifft imagesc input interp1 inv legend length linspace load log log10 magic max mean median mesh meshgrid min norm numel ode45 ones peaks pkg plot poly polyval rand randn rank readmatrix reshape roots round save semilogx semilogy sgtitle sin size sort sparse sprintf sqrt std stem step strcmp struct subplot sum surf svd tan tf title transpose trapz unique view while xlabel xlim ylabel ylim zeros zlabel".split(" ");
const completion = (ctx) => {
  let word = ctx.matchBefore(/[A-Za-z_][A-Za-z_0-9]*/);
  if (!word || !ctx.explicit && word.from === word.to) return null;
  return { from: word.from, options: [.../* @__PURE__ */ new Set([...words, ...shared.variables.map((v) => v.name), ...registry.editorIntelCompletionLabels(ctx.state.doc.toString())])].map((label) => ({ label, type: shared.variables.some((v) => v.name === label) ? "variable" : "function" })) };
};
// Colours come from CSS variables so one style serves both themes.
const highlight = HighlightStyle.define([{ tag: tags.comment, color: "var(--syn-comment)" }, { tag: tags.keyword, color: "var(--syn-keyword)" }, { tag: tags.standard(tags.variableName), color: "var(--syn-builtin)" }, { tag: tags.string, color: "var(--syn-string)" }, { tag: tags.number, color: "var(--syn-number)" }, { tag: [tags.atom, tags.bool], color: "var(--syn-number)" }]);
const sectionLine = Decoration.line({ class: "cm-section" });
const debugLine = Decoration.line({ class: "cm-debug-line" }), debugFrameLine = Decoration.line({ class: "cm-debug-frame-line" }), debugCurrentFrameLine = Decoration.line({ class: "cm-debug-line cm-debug-frame-line" }), setDebugLine = StateEffect.define(), setBreakpointMarkers = StateEffect.define();
class BreakpointMarker extends GutterMarker {
  constructor(point) {
    super();
    this.enabled = point.enabled;
    this.conditional = !!point.condition;
    this.language = getLanguage();
  }
  eq(other) {
    return this.enabled === other.enabled && this.conditional === other.conditional && this.language === other.language;
  }
  toDOM() {
    let n = registry.el("span", "cm-breakpoint-marker" + (this.enabled ? "" : " disabled") + (this.conditional ? " conditional" : ""));
    n.title = this.conditional ? tr("Conditional breakpoint; right-click to edit condition") : tr("Breakpoint; right-click to add a condition");
    n.setAttribute("aria-label", tr("Breakpoint"));
    return n;
  }
}
const breakpointField = StateField.define({ create: () => RangeSet.empty, update(value, tr) {
  value = value.map(tr.changes);
  for (let effect of tr.effects) if (effect.is(setBreakpointMarkers)) value = effect.value;
  return value;
} });
const debugLineField = StateField.define({ create: () => Decoration.none, update(value, tr) {
  value = value.map(tr.changes);
  for (let effect of tr.effects) if (effect.is(setDebugLine)) value = effect.value;
  return value;
}, provide: (field) => EditorView.decorations.from(field) });
function breakpointRanges(doc) {
  let builder = new RangeSetBuilder(), points = [...shared.breakpoints.get(shared.active?.path) || []].filter(([line]) => line <= doc.lines).sort((a, b) => a[0] - b[0]);
  for (let [number, point] of points) builder.add(doc.line(number).from, doc.line(number).from, new BreakpointMarker(point));
  return builder.finish();
}
const breakpointGutter = gutter({ class: "cm-breakpoint-gutter", renderEmptyElements: true, markers: (view) => view.state.field(breakpointField), domEventHandlers: { mousedown(view, line, event) {
  if (event.button !== 0) return false;
  event.preventDefault();
  let number = view.state.doc.lineAt(line.from).number;
  registry.safe(() => event.shiftKey ? registry.editBreakpointCondition(number) : registry.toggleBreakpoint(number));
  return true;
}, contextmenu(view, line, event) {
  event.preventDefault();
  let number = view.state.doc.lineAt(line.from).number;
  registry.safe(() => registry.editBreakpointCondition(number));
  return true;
} } });
function refreshDebugEditor() {
  if (!shared.editor) return;
  let marks = Decoration.none, debug = shared.engine.status === "paused" ? shared.engine.debug : null, stack = debug?.stack || [], top = stack[0] || debug, current = stack.find((frame) => frame.current) || debug;
  if (debug) {
    let ranges = [], same = top?.file === current?.file && top?.line === current?.line;
    if (top?.file === shared.active?.path && top.line <= shared.editor.state.doc.lines) ranges.push((same ? debugCurrentFrameLine : debugLine).range(shared.editor.state.doc.line(top.line).from));
    if (!same && current?.file === shared.active?.path && current.line <= shared.editor.state.doc.lines) ranges.push(debugFrameLine.range(shared.editor.state.doc.line(current.line).from));
    marks = ranges.length ? Decoration.set(ranges.sort((a, b) => a.from - b.from)) : Decoration.none;
  }
  shared.editor.dispatch({ effects: [setBreakpointMarkers.of(breakpointRanges(shared.editor.state.doc)), setDebugLine.of(marks)] });
}
function revealLine(number) {
  if (!shared.editor) return;
  number = Math.max(1, Math.min(shared.editor.state.doc.lines, Number(number) || 1));
  let position = shared.editor.state.doc.line(number).from;
  shared.editor.dispatch({ selection: { anchor: position }, effects: EditorView.scrollIntoView(position, { y: "center" }) });
  updateCursor();
}
// Mark %% cell headers, the same boundaries runSection uses.
const sections = ViewPlugin.fromClass(class {
  constructor(view) {
    this.decorations = this.build(view);
  }
  update(u) {
    if (u.docChanged || u.viewportChanged) this.decorations = this.build(u.view);
  }
  build(view) {
    let b = new RangeSetBuilder();
    const lexicalLines = editorLexicalLines(view.state.doc);
    for (let { from, to } of view.visibleRanges) {
      for (let pos = from; pos <= to; ) {
        let line = view.state.doc.lineAt(pos);
        if (lexicalLines[line.number - 1].section) b.add(line.from, line.from, sectionLine);
        pos = line.to + 1;
      }
    }
    return b.finish();
  }
}, { decorations: (v) => v.decorations });
function updateLintCaption() {
  let node = registry.$("#code-issues"), state = shared.active?.lintState;
  node.classList.toggle("error", state?.kind === "error");
  node.textContent = state?.kind === "ok" ? tr("No issues") : state?.kind === "error" ? tr("1 syntax error, line {line}", { line: state.line }) : state?.kind === "unavailable" ? tr("Unable to check") : tr("Checking…");
  registry.scheduleEditorIntel?.();
}
function lintDiagnostic(issue, doc) {
  let number = Math.max(1, Math.min(doc.lines, Number(issue.line) || 1)), line = doc.line(number), column = Math.max(1, Number(issue.column) || 1), from = Math.min(line.to, line.from + column - 1), to = Math.min(line.to, from + 1);
  return { from, to, severity: "error", message: issue.message || tr("Syntax error."), source: "Octave" };
}
async function lintSource(view) {
  let tab = shared.active, source = view.state.doc.toString(), request = ++shared.lintSequence;
  if (!tab) return [];
  tab.lintState = { kind: "checking", issues: [] };
  if (shared.active === tab) updateLintCaption();
  try {
    let result = await registry.api("lint", { code: source });
    if (request !== shared.lintSequence || shared.active !== tab || view.state.doc.toString() !== source) return [];
    let issues = Array.isArray(result.issues) ? result.issues : [];
    tab.lintState = issues.length ? { kind: "error", line: issues[0].line, issues } : { kind: "ok", issues: [] };
    updateLintCaption();
    return issues.map((issue) => lintDiagnostic(issue, view.state.doc));
  } catch {
    if (request === shared.lintSequence && shared.active === tab && view.state.doc.toString() === source) {
      tab.lintState = { kind: "unavailable", issues: [] };
      updateLintCaption();
    }
    return [];
  }
}
const octaveFold = foldService.of((state, lineStart) => {
  let doc = state.doc, line = doc.lineAt(lineStart), lexicalLines = editorLexicalLines(doc), text = lexicalLines[line.number - 1].code;
  if (lexicalLines[line.number - 1].section) {
    for (let n = line.number + 1; n <= doc.lines; n++) {
      let next = doc.line(n);
      if (lexicalLines[n - 1].section) {
        let previous = doc.line(n - 1);
        return previous.to > line.to ? { from: line.to, to: previous.to } : null;
      }
    }
    return doc.length > line.to ? { from: line.to, to: doc.length } : null;
  }
  if (!/^\s*(function|for|parfor|if|while|switch|try|unwind_protect|classdef|methods|properties|events|enumeration|spmd|do)\b/i.test(text)) return null;
  let depth = 1;
  for (let n = line.number + 1; n <= doc.lines; n++) {
    let next = lexicalLines[n - 1].code;
    if (/^\s*(function|for|parfor|if|while|switch|try|unwind_protect|classdef|methods|properties|events|enumeration|spmd|do)\b/i.test(next)) depth++;
    if (/^\s*(end|endfunction|endfor|endparfor|endif|endwhile|endswitch|end_try_catch|end_unwind_protect|endclassdef|endmethods|endproperties|endevents|endenumeration|endspmd|until)\b/i.test(next) && !--depth) {
      let close = doc.line(n);
      return close.from > line.to ? { from: line.to, to: close.from } : null;
    }
  }
  return null;
});
const themeFor = (dark) => EditorView.theme({}, { dark });
const isDark = () => document.body.classList.contains("dark");
function makeState(content) {
  return EditorState.create({ doc: content, extensions: [basicSetup, StreamLanguage.define(octave), syntaxHighlighting(highlight), sections, octaveFold, autocompletion({ override: [completion] }), linter(lintSource, { delay: 500 }), lintGutter(), breakpointField, debugLineField, breakpointGutter, themeCompartment.of(themeFor(isDark())), EditorView.updateListener.of((update) => {
    if (update.docChanged && shared.active) {
      shared.active.content = update.state.doc.toString();
      shared.active.dirty = shared.active.content !== shared.active.saved;
      shared.active.lintState = { kind: "checking", issues: [] };
      renderTabs();
      persistDrafts();
    }
    if (update.selectionSet || update.docChanged) updateCursor();
  })] });
}
function setEditorFont(size, persist = true) {
  shared.editorFontSize = Math.min(24, Math.max(10, size));
  document.documentElement.style.setProperty("--editor-font-size", shared.editorFontSize + "px");
  if (persist && shared.settings) {
    shared.settings.preferences.editorFontSize = shared.editorFontSize;
    registry.saveSettings();
  }
  if (persist && !shared.settings) {
    try {
      localStorage.setItem("mf-editor-font-size", String(shared.editorFontSize));
    } catch {
    }
  }
  return true;
}
function changeEditorFont(delta) {
  return setEditorFont(shared.editorFontSize + delta);
}
function resetEditorFont() {
  return setEditorFont(13);
}
function applyTheme(dark, persist = true) {
  document.body.classList.toggle("dark", dark);
  shared.editor.dispatch({ effects: themeCompartment.reconfigure(themeFor(dark)) });
  shared.interactivePlot?.draw();
  if (persist && shared.settings) {
    shared.settings.preferences.theme = dark ? "dark" : "light";
    registry.saveSettings();
  }
  if (persist && !shared.settings) {
    try {
      localStorage.setItem("mf-theme", dark ? "dark" : "light");
    } catch {
    }
  }
}
function persistDrafts() {
  clearTimeout(shared.draftTimer);
  shared.draftTimer = setTimeout(() => {
    if (!shared.draftScope) return;
    try {
      let data = registry.draftStore();
      data.scopes[shared.draftScope] = { tabs: shared.tabs.filter((t) => registry.within(t.path, shared.draftScope)).map((t) => ({ path: t.path, content: t.content, saved: t.saved, hash: t.hash, dirty: t.dirty })), active: registry.within(shared.active?.path || "", shared.draftScope) ? shared.active.path : null };
      localStorage.setItem("mf-drafts-v2", JSON.stringify(data));
    } catch (e) {
      registry.toast(tr("Browser draft storage is full. Save your files."));
    }
  }, 200);
}
function migrateLegacyDrafts() {
  let migrated = false;
  try {
    let data = registry.draftStore(), legacy = JSON.parse(localStorage.getItem("mf-drafts") || "null");
    if (!data.legacyMigrated && legacy?.tabs?.length) {
      let scoped = data.scopes[shared.draftScope] || { tabs: [], active: null }, known = new Set(scoped.tabs.map((t) => t.path));
      for (let t of legacy.tabs) {
        if (!t?.path || String(t.path).split("/").includes("..")) continue;
        let path = registry.joinPath(shared.defaultWorkspace, t.path);
        if (registry.within(path, shared.draftScope) && !known.has(path)) {
          scoped.tabs.push({ ...t, path });
          known.add(path);
        }
      }
      if (!scoped.active && legacy.active) scoped.active = registry.joinPath(shared.defaultWorkspace, legacy.active);
      data.scopes[shared.draftScope] = scoped;
      migrated = true;
    }
    data.legacyMigrated = true;
    localStorage.setItem("mf-drafts-v2", JSON.stringify(data));
  } catch {
  }
  return migrated;
}
function updateCursor() {
  let pos = shared.editor.state.selection.main.head, line = shared.editor.state.doc.lineAt(pos);
  registry.$("#cursor").textContent = tr("Line {line}, Column {column}", { line: line.number, column: pos - line.from + 1 });
  registry.$("#lines").textContent = tr("{count} lines", { count: shared.editor.state.doc.lines });
  registry.scheduleEditorIntel?.();
}
function fileLabel(path) {
  let name = path.split("/").pop(), dot = name.lastIndexOf(".");
  let wrap = registry.el("span", "base");
  if (dot > 0) {
    wrap.append(name.slice(0, dot), registry.el("span", "ext", name.slice(dot)));
  } else wrap.textContent = name;
  return wrap;
}
function renderTabs() {
  let root = registry.$("#tabs");
  root.replaceChildren();
  for (let t of shared.tabs) {
    let b = registry.el("button", "editor-tab" + (t === shared.active ? " active" : ""));
    b.setAttribute("role", "tab");
    b.setAttribute("aria-selected", String(t === shared.active));
    b.title = t.path;
    b.append(fileLabel(t.path));
    if (t.dirty) b.append(registry.el("span", "dirty"));
    let x = registry.el("span", "close-tab", "\xD7");
    x.setAttribute("role", "button");
    x.setAttribute("aria-label", tr("Close {name} tab", { name: t.path }));
    x.onclick = (e) => {
      e.stopPropagation();
      closeTab(t);
    };
    b.append(x);
    b.onclick = () => switchTab(t);
    root.append(b);
  }
  let add = registry.el("button", "add-tab", "+");
  add.id = "new-tab";
  add.title = tr("New File");
  add.setAttribute("aria-label", tr("New file tab"));
  add.onclick = createUntitled;
  root.append(add);
  registry.$("#save-state").textContent = shared.active?.dirty ? tr("Unsaved changes") : tr("Saved");
  registry.$("#editor-label").textContent = shared.active ? shared.active.path.split("/").pop() : tr("New File");
  registry.$("#editor-label").title = shared.active?.path || "";
  updateLintCaption();
  registry.renderFiles();
}
function switchTab(t) {
  if (shared.active) shared.active.state = shared.editor.state;
  shared.active = null;
  shared.editor.setState(t.state || makeState(t.content));
  shared.editor.dispatch({ effects: themeCompartment.reconfigure(themeFor(isDark())) });
  shared.active = t;
  renderTabs();
  refreshDebugEditor();
  updateCursor();
  shared.editor.focus();
  persistDrafts();
}
function closeTab(t) {
  if (t.dirty && !confirm(tr("Discard unsaved changes in {name}?", { name: t.path }))) return;
  let index = shared.tabs.indexOf(t);
  shared.tabs.splice(index, 1);
  if (t === shared.active) {
    shared.active = null;
    if (shared.tabs.length) switchTab(shared.tabs[Math.max(0, index - 1)]);
    else createUntitled();
  }
  renderTabs();
  persistDrafts();
}
async function openFile(path) {
  let t = shared.tabs.find((t2) => t2.path === path);
  if (t) {
    switchTab(t);
    return;
  }
  let data = await registry.api("file?path=" + encodeURIComponent(path));
  t = { ...data, saved: data.content, dirty: false };
  shared.tabs.push(t);
  switchTab(t);
}
function createUntitled() {
  let i = 1, path;
  do {
    path = registry.joinPath(shared.currentFolder, `yeni_${i++}.m`);
  } while (shared.tabs.some((t2) => t2.path === path));
  let t = { path, content: "%% Yeni \xE7al\u0131\u015Fma\n\n", saved: "", hash: null, dirty: true };
  shared.tabs.push(t);
  switchTab(t);
}
function modal(title, node, titleSource) {
  const heading = registry.$("#modal-title");
  heading.textContent = title;
  if (titleSource) heading.setAttribute("data-i18n", titleSource);
  else heading.removeAttribute("data-i18n");
  if (titleSource) registry.$("#modal").addEventListener("close", () => {
    if (heading.getAttribute("data-i18n") === titleSource) heading.removeAttribute("data-i18n");
  }, { once: true });
  registry.$("#modal-body").replaceChildren(node);
  if (!registry.$("#modal").open) registry.$("#modal").showModal();
}
async function askName(initial) {
  return new Promise((resolve) => {
    let form = registry.el("form", "save-dialog");
    let note = registry.el("p", "", tr("File name relative to the current folder. You can use a subfolder: experiments/analysis.m"));
    note.setAttribute("data-i18n", "File name relative to the current folder. You can use a subfolder: experiments/analysis.m");
    form.append(note);
    let input = registry.el("input");
    input.value = initial.split("/").pop();
    input.required = true;
    input.setAttribute("aria-label", tr("File name"));
    input.setAttribute("data-i18n-aria-label", "File name");
    let button = registry.el("button", "", tr("Save"));
    button.setAttribute("data-i18n", "Save");
    button.type = "submit";
    let done = false;
    form.append(input, button);
    form.onsubmit = (e) => {
      e.preventDefault();
      done = true;
      resolve(input.value.trim());
      registry.$("#modal").close();
    };
    registry.$("#modal").addEventListener("close", () => {
      if (!done) resolve(null);
    }, { once: true });
    modal(tr("Save File"), form, "Save File");
    input.focus();
    input.select();
  });
}
async function saveActive() {
  if (!shared.active) return false;
  let t = shared.active, path = t.path;
  if (!t.hash) {
    let name = await askName(path);
    if (!name) return false;
    path = registry.joinPath(shared.currentFolder, name);
  }
  const written = t.content;
  let data = await registry.api("file", { path, content: written, hash: t.hash || "absent" });
  t.path = path;
  t.hash = data.hash;
  t.saved = written;
  t.dirty = t.content !== written;
  renderTabs();
  persistDrafts();
  await registry.refreshFiles(false);
  return t;
}
async function saveAs() {
  if (!shared.active) return;
  const t = shared.active, name = await askName(t.path);
  if (!name) return;
  const path = registry.joinPath(shared.currentFolder, name);
  if (path === t.path && t.hash) return saveActive();
  if (shared.tabs.some((other) => other !== t && other.path === path)) throw new Error(tr("This file is already open in another tab."));
  const written = t.content;
  const data = await registry.api("file", { path, content: written, hash: "absent" });
  t.path = path;
  t.hash = data.hash;
  t.saved = written;
  t.dirty = t.content !== written;
  renderTabs();
  persistDrafts();
  await registry.refreshFiles(false);
}

registry.setupEditor = () => {
  shared.editor = new EditorView({ state: makeState(""), parent: registry.$("#editor") });
  shared.editorFontSize = registry.getSetting("preferences", "editorFontSize");
  setEditorFont(shared.editorFontSize, false);
  applyTheme(registry.getSetting("preferences", "theme") !== "light", false);
  registry.$("#theme").onclick = () => applyTheme(!document.body.classList.contains("dark"));
  registry.$("#undo").onclick = () => {
    undo(shared.editor);
    shared.editor.focus();
  };
  registry.$("#redo").onclick = () => {
    redo(shared.editor);
    shared.editor.focus();
  };
};

registry.setupModal = () => {
  registry.$("#modal-close").onclick = () => registry.$("#modal").close();
  registry.$("#modal").addEventListener("click", (e) => {
    if (e.target === registry.$("#modal")) registry.$("#modal").close();
  });

};
onLanguageChange(() => {
  if (!shared.editor) return;
  renderTabs();
  updateCursor();
  refreshDebugEditor();
});
Object.assign(registry, { themeCompartment, words, completion, highlight, sectionLine, debugLine, debugFrameLine, debugCurrentFrameLine, setDebugLine, setBreakpointMarkers, BreakpointMarker, breakpointField, debugLineField, breakpointRanges, breakpointGutter, refreshDebugEditor, revealLine, sections, updateLintCaption, lintDiagnostic, lintSource, octaveFold, themeFor, isDark, makeState, setEditorFont, changeEditorFont, resetEditorFont, applyTheme, persistDrafts, migrateLegacyDrafts, updateCursor, fileLabel, renderTabs, switchTab, closeTab, openFile, createUntitled, modal, askName, saveActive, saveAs });
