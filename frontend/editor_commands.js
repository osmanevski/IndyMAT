import { foldAll, unfoldAll } from "@codemirror/language";

import shared from "./state.js";
import registry from "./registry.js";
import editorCommandUtils from "./editor_command_utils.cjs";
import { t as tr, onLanguageChange } from "./i18n.js";

const { smartIndent, toggleComment } = editorCommandUtils;

function applyTextCommand(transform) {
  let view = shared.editor, range = view.state.selection.main, edit = transform(view.state.doc.toString(), range.from, range.to);
  view.dispatch({ changes: { from: edit.from, to: edit.to, insert: edit.text }, selection: { anchor: edit.selectionFrom, head: edit.selectionTo }, scrollIntoView: true });
  view.focus();
  return true;
}

function toggleEditorComment() {
  return applyTextCommand(toggleComment);
}

function smartIndentSelection() {
  let width = registry.getSetting("preferences", "indentWidth"), tabs = registry.getSetting("preferences", "useTabs");
  return applyTextCommand((source, from, to) => smartIndent(source, from, to, tabs ? "\t" : " ".repeat(width)));
}

function goToLine() {
  let view = shared.editor, form = registry.el("form", "go-line-dialog"), label = registry.el("label", "", tr("Line number")), input = registry.el("input"), button = registry.el("button", "", tr("Go"));
  label.setAttribute("data-i18n", "Line number");
  input.type = "number";
  input.min = "1";
  input.max = String(view.state.doc.lines);
  input.value = String(view.state.doc.lineAt(view.state.selection.main.head).number);
  input.required = true;
  input.setAttribute("aria-label", tr("Line number"));
  input.setAttribute("data-i18n-aria-label", "Line number");
  button.type = "submit";
  button.setAttribute("data-i18n", "Go");
  label.append(input);
  form.append(label, button);
  form.onsubmit = (event) => {
    event.preventDefault();
    let number = Number(input.value);
    if (!Number.isInteger(number) || number < 1 || number > view.state.doc.lines) {
      input.setCustomValidity(tr("Enter a line between 1 and {count}.", { count: view.state.doc.lines }));
      input.reportValidity();
      return;
    }
    input.setCustomValidity("");
    let position = view.state.doc.line(number).from;
    registry.$("#modal").close();
    view.dispatch({ selection: { anchor: position }, scrollIntoView: true });
    view.focus();
  };
  registry.$("#modal-title").setAttribute("data-i18n", "Go To Line");
  registry.$("#modal").addEventListener("close", () => {
    if (registry.$("#modal-title").getAttribute("data-i18n") === "Go To Line") registry.$("#modal-title").removeAttribute("data-i18n");
  }, { once: true });
  registry.modal(tr("Go To Line"), form);
  input.focus();
  input.select();
}

function foldAllEditor() {
  let folded = foldAll(shared.editor);
  shared.editor.focus();
  return folded;
}

function unfoldAllEditor() {
  let unfolded = unfoldAll(shared.editor);
  shared.editor.focus();
  return unfolded;
}

function bindEditorCommand(selector, command) {
  let button = registry.$(selector);
  button.onclick = () => {
    button.closest("details").open = false;
    registry.safe(command);
  };
}

function setupEditorCommands() {
  bindEditorCommand("#editor-run-section", registry.runSection);
  bindEditorCommand("#editor-run-advance", registry.runAndAdvance);
  bindEditorCommand("#editor-run-to-end", registry.runToEnd);
  bindEditorCommand("#editor-run-selection", registry.runSelection);
  bindEditorCommand("#editor-go-line", goToLine);
  bindEditorCommand("#editor-toggle-comment", toggleEditorComment);
  bindEditorCommand("#editor-smart-indent", smartIndentSelection);
  bindEditorCommand("#editor-fold-all", foldAllEditor);
  bindEditorCommand("#editor-unfold-all", unfoldAllEditor);
}

Object.assign(registry, { foldAllEditor, goToLine, setupEditorCommands, smartIndentSelection, toggleEditorComment, unfoldAllEditor });
const shortcuts = [
  { id: "editor.run-section", label: () => tr("Run the current section"), bindings: ["Mod+Enter"], command: () => registry.safe(registry.runSection) },
  { id: "editor.run-advance", label: () => tr("Run and Advance"), bindings: ["Mod+Shift+Enter"], command: () => registry.safe(registry.runAndAdvance) },
  { id: "editor.run-to-end", label: () => tr("Run to End"), bindings: ["Mod+Alt+Shift+Enter"], command: () => registry.safe(registry.runToEnd) },
  { id: "editor.go-line", label: () => tr("Go To Line"), bindings: ["Ctrl+KeyG"], command: () => registry.safe(registry.goToLine) },
  { id: "editor.comment", label: () => tr("Toggle Comment"), bindings: ["Mod+Slash", "Shift+F9"], command: () => registry.safe(registry.toggleEditorComment) },
  { id: "editor.indent", label: () => tr("Smart Indent"), bindings: ["Ctrl+KeyI"], command: () => registry.safe(registry.smartIndentSelection) },
  { id: "editor.fold-all", label: () => tr("Collapse All"), bindings: ["Ctrl+Comma"], command: () => registry.foldAllEditor(), ignoreShift: true },
  { id: "editor.unfold-all", label: () => tr("Expand All"), bindings: ["Ctrl+Period"], command: () => registry.unfoldAllEditor(), ignoreShift: true }
];
for (const definition of shortcuts) registry.registerShortcut?.({ ...definition, get label() { return definition.label(); }, scope: "editor", get scopeLabel() { return tr("Editor"); } });
let previousShortcutLabels = shortcuts.map((definition) => definition.label());
let previousScopeLabel = tr("Editor");
onLanguageChange(() => {
  const nextLabels = shortcuts.map((definition) => definition.label());
  for (const row of document.querySelectorAll(".shortcut-row")) {
    const label = row.querySelector(".shortcut-name");
    const index = previousShortcutLabels.indexOf(label?.textContent);
    if (index >= 0) label.textContent = nextLabels[index];
    const scope = row.querySelector(".shortcut-scope");
    if (scope?.textContent === previousScopeLabel) scope.textContent = tr("Editor");
  }
  previousShortcutLabels = nextLabels;
  previousScopeLabel = tr("Editor");
});
