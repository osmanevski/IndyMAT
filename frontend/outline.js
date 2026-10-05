import shared from "./state.js";
import registry from "./registry.js";
import editorIntelUtils from "./editor_intel_utils.cjs";
import { t as tr, onLanguageChange } from "./i18n.js";

const { parseEditorSymbols } = editorIntelUtils;
let activeView = "outline";
let scheduled = 0;
let occurrenceSearch = null;

function panel() {
  return registry.$("#editor-intel-panel");
}

function setPanelView(view, open = true) {
  activeView = view;
  panel().hidden = !open;
  for (const button of document.querySelectorAll("[data-editor-intel]")) {
    const selected = button.dataset.editorIntel === view && open;
    button.classList.toggle("active", selected);
    button.setAttribute("aria-expanded", String(selected));
  }
  renderEditorIntel();
}

function togglePanel(view) {
  setPanelView(view, panel().hidden || activeView !== view);
}

function locationButton(item, label, detail = "") {
  const button = registry.el("button", "editor-intel-row");
  button.dataset.line = item.line;
  button.append(registry.el("span", "editor-intel-name", label), registry.el("span", "editor-intel-detail", detail || tr("line {line}", { line: item.line })));
  button.onclick = () => registry.safe(() => item.path ? registry.openSymbolLocation(item) : registry.revealLine(item.line));
  return button;
}

function renderOutline(root) {
  const source = shared.editor.state.doc.toString();
  const symbols = parseEditorSymbols(source, tr);
  const cursorLine = shared.editor.state.doc.lineAt(shared.editor.state.selection.main.head).number;
  const all = [...symbols.sections, ...symbols.functions].sort((a, b) => a.line - b.line);
  let current = null;
  for (const item of all) if (item.line <= cursorLine) current = item;
  root.append(registry.el("div", "editor-intel-label", tr("Outline for the active file")));
  for (const item of all) {
    const button = locationButton(item, item.name, item.kind === "section" ? tr("Section · {line}", { line: item.line }) : tr("Function · {line}", { line: item.line }));
    button.classList.toggle("current", item === current);
    root.append(button);
  }
  if (!all.length) root.append(registry.el("div", "muted-empty", tr("No %% sections or function definitions.")));
}

function renderCodeIssues(root) {
  const state = shared.active?.lintState;
  root.append(registry.el("div", "editor-intel-label", tr("Octave syntax")));
  if (!state || state.kind === "checking") root.append(registry.el("div", "muted-empty", tr("Checking…")));
  else if (state?.kind === "unavailable") root.append(registry.el("div", "muted-empty", tr("Unable to check Octave syntax.")));
  else if (state?.kind === "error") {
    for (const issue of state.issues || []) root.append(locationButton(issue, issue.message || tr("Syntax error"), tr("Line {line}{columnPart}", { line: issue.line, columnPart: issue.column ? tr(", column {column}", { column: issue.column }) : "" })));
  } else root.append(registry.el("div", "muted-empty", tr("No Octave syntax issues.")));
}

function renderEditorIntel() {
  if (!shared.editor || panel().hidden) return;
  const root = registry.$("#editor-intel-content");
  if (occurrenceSearch && (occurrenceSearch.tab !== shared.active || occurrenceSearch.source !== shared.editor.state.doc.toString() || occurrenceSearch.folder !== shared.currentFolder)) occurrenceSearch = null;
  panel().dataset.view = activeView;
  root.replaceChildren();
  registry.$("#editor-intel-title").textContent = activeView === "issues" ? tr("Code Issues") : activeView === "occurrences" ? tr("Usages") : tr("Outline");
  if (activeView === "issues") renderCodeIssues(root);
  else if (activeView === "outline") renderOutline(root);
  else if (activeView === "occurrences") renderOccurrences(root);
}

function scheduleEditorIntel() {
  cancelAnimationFrame(scheduled);
  scheduled = requestAnimationFrame(renderEditorIntel);
}

function beginOccurrenceSearch(name) {
  occurrenceSearch = { name, tab: shared.active, source: shared.editor.state.doc.toString(), folder: shared.currentFolder, pending: true };
  activeView = "occurrences";
  panel().hidden = false;
  for (const button of document.querySelectorAll("[data-editor-intel]")) {
    button.classList.remove("active");
    button.setAttribute("aria-expanded", "false");
  }
  renderEditorIntel();
  return occurrenceSearch;
}

function occurrenceSearchCurrent(ticket) {
  return occurrenceSearch === ticket && activeView === "occurrences" && !panel().hidden;
}

function failOccurrenceSearch(message) {
  Object.assign(occurrenceSearch, { pending: false, error: message });
  renderEditorIntel();
}

function showOccurrenceResults(name, results, truncated, isFunction) {
  Object.assign(occurrenceSearch, { name, results, truncated, isFunction, pending: false });
  renderEditorIntel();
}

function renderOccurrences(root) {
  if (!occurrenceSearch) {
    root.append(registry.el("div", "muted-empty", tr("The file changed. Search for usages again.")));
    return;
  }
  const { name, results, truncated, isFunction, pending, error } = occurrenceSearch;
  registry.$("#editor-intel-title").textContent = tr("Usages of “{name}”", { name });
  if (pending || error) {
    root.append(registry.el("div", "editor-intel-label", pending ? tr("Searching for usages…") : tr("Unable to search for usages")));
    if (error) root.append(registry.el("div", "muted-empty", error));
    return;
  }
  root.append(registry.el("div", "editor-intel-label", isFunction ? tr("Active file and current folder") : tr("Active file only")));
  for (const item of results) root.append(locationButton(item, item.path.split("/").pop(), tr("Line {line}, column {column}", { line: item.line, column: item.column })));
  if (!results.length) root.append(registry.el("div", "muted-empty", tr("No usages found.")));
  if (truncated) root.append(registry.el("div", "editor-intel-limit", tr("Results were truncated at the safety limit.")));
}

function setupEditorIntel() {
  for (const button of document.querySelectorAll("[data-editor-intel]")) button.onclick = () => togglePanel(button.dataset.editorIntel);
  registry.$("#editor-intel-close").onclick = () => setPanelView(activeView, false);
  registry.$("#code-issues").onclick = () => togglePanel("issues");
  registry.$("#code-issues").setAttribute("role", "button");
  registry.$("#code-issues").tabIndex = 0;
  registry.$("#code-issues").onkeydown = (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      togglePanel("issues");
    }
  };
}

onLanguageChange(() => renderEditorIntel());

Object.assign(registry, { beginOccurrenceSearch, occurrenceSearchCurrent, failOccurrenceSearch, renderCodeIssues, renderEditorIntel, scheduleEditorIntel, setPanelView, setupEditorIntel, showOccurrenceResults });
