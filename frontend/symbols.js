import shared from "./state.js";
import registry from "./registry.js";
import editorIntelUtils from "./editor_intel_utils.cjs";
import { t as tr, onLanguageChange } from "./i18n.js";

const { identifierAt, occurrencesInSource, parseEditorSymbols, variableDefinitionInSource } = editorIntelUtils;
let folderIndex = { folder: "", functions: [], truncated: false };
let indexPromise = null;

function invalidateSymbolIndex() {
  folderIndex = { folder: "", functions: [], truncated: false };
  indexPromise = null;
  return ensureSymbolIndex().catch(() => folderIndex);
}

async function ensureSymbolIndex() {
  if (folderIndex.folder === shared.currentFolder) return folderIndex;
  if (!indexPromise) {
    const folder = shared.currentFolder;
    const request = registry.api("symbols").then((result) => {
      if (folder === shared.currentFolder) folderIndex = result;
      if (folder === shared.currentFolder) return result;
      if (indexPromise === request) indexPromise = null;
      return ensureSymbolIndex();
    }).finally(() => {
      if (indexPromise === request) indexPromise = null;
    });
    indexPromise = request;
  }
  return indexPromise;
}

function editorIntelCompletionLabels(source) {
  ensureSymbolIndex().catch(() => {});
  const local = parseEditorSymbols(source, tr).functions.map((item) => item.name);
  const folder = folderIndex.folder === shared.currentFolder ? folderIndex.functions.map((item) => item.name) : [];
  const history = shared.commands.slice(-100).flatMap((command) => command.match(/[A-Za-z_][A-Za-z0-9_]*/g) || []);
  return [...new Set([...history, ...local, ...folder])];
}

function symbolUnderCursor() {
  const source = shared.editor.state.doc.toString();
  return identifierAt(source, shared.editor.state.selection.main.head);
}

async function openLocation(location) {
  if (location.path !== shared.active?.path) await registry.openFile(location.path);
  registry.revealLine(location.line);
  shared.editor.focus();
}

async function goToDefinition() {
  const name = symbolUnderCursor();
  if (!name) return registry.toast(tr("No function or variable name at the cursor."));
  const active = shared.active;
  const source = shared.editor.state.doc.toString();
  const folder = shared.currentFolder;
  const local = parseEditorSymbols(source, tr).functions.find((item) => item.name === name);
  if (local) return openLocation({ ...local, path: shared.active.path });
  const variable = variableDefinitionInSource(shared.editor.state.doc.toString(), name);
  if (variable) return openLocation({ ...variable, path: shared.active.path });
  const index = await ensureSymbolIndex();
  if (shared.active !== active || shared.editor.state.doc.toString() !== source || shared.currentFolder !== folder) return;
  const definition = index.functions.find((item) => item.name === name);
  if (!definition) return registry.toast(tr("No definition found for “{name}”.", { name }));
  return openLocation(definition);
}

async function findOccurrences() {
  const name = symbolUnderCursor();
  if (!name) return registry.toast(tr("No function or variable name to search for at the cursor."));
  const active = shared.active;
  const source = shared.editor.state.doc.toString();
  const activePath = active.path;
  const folder = shared.currentFolder;
  const ticket = registry.beginOccurrenceSearch(name);
  const current = () => shared.active === active && shared.editor.state.doc.toString() === source && shared.currentFolder === folder && registry.occurrenceSearchCurrent(ticket);
  const local = occurrencesInSource(source, name).map((item) => ({ ...item, path: activePath }));
  // Local variables do not depend on the folder index or the saved-file hash.
  if (variableDefinitionInSource(source, name)) {
    registry.showOccurrenceResults(name, local, false, false);
    return;
  }
  let index;
  try {
    index = await ensureSymbolIndex();
  } catch (error) {
    if (current()) registry.failOccurrenceSearch(error.message);
    return;
  }
  if (!current()) return;
  const isFunction = parseEditorSymbols(source, tr).functions.some((item) => item.name === name) || !variableDefinitionInSource(source, name) && index.functions.some((item) => item.name === name);
  let results = local;
  let truncated = false;
  if (isFunction) {
    let remote;
    try {
      remote = await registry.api("symbols?name=" + encodeURIComponent(name));
    } catch (error) {
      if (current()) registry.failOccurrenceSearch(error.message);
      return;
    }
    if (!current()) return;
    results = [...local, ...remote.occurrences.filter((item) => item.path !== activePath)];
    truncated = remote.truncated;
  }
  registry.showOccurrenceResults(name, results, truncated, isFunction);
}

Object.assign(registry, { editorIntelCompletionLabels, ensureSymbolIndex, findOccurrences, goToDefinition, invalidateSymbolIndex, openSymbolLocation: openLocation });
const shortcuts = [
  { id: "editor.definition", label: () => tr("Go to Definition"), bindings: ["F12", "Alt+F7"], command: () => registry.safe(registry.goToDefinition) },
  { id: "editor.occurrences", label: () => tr("Find Usages"), bindings: ["Shift+F12", "Alt+F8"], command: () => registry.safe(registry.findOccurrences) }
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
