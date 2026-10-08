// Transport and transparency only. The Python adapter owns all rewriting/maps.
import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";

export function editorSourceContext(origin, document, from, to, cursor) {
  if (!registry.getSetting("preferences", "adaptEditorLiterals")) return null;
  return { origin, document, span: [from, to], cursor, path: shared.active.path, revision: String(shared.uiGeneration) + ":" + String(Date.now()), profile: "matlab" };
}

function diagnosticText(diagnostic) {
  if (diagnostic.code === "file-debug") return t("Entry scripts run unchanged while debugging or breakpoints are active.");
  if (diagnostic.code === "file-profile") return t("Entry scripts run unchanged while profiling is active.");
  if (diagnostic.code === "file-hierarchy") return t("Private folders and package or class hierarchies run unchanged.");
  if (diagnostic.code === "file-declaration") return t("Function, class and local-function files run unchanged.");
  if (diagnostic.code === "quoted-command") return t("Quoted command-form arguments execute unchanged.");
  if (diagnostic.code === "package-unverified") return t("The patched datatypes string constructor is unavailable or could not be verified.");
  if (diagnostic.code === "semantic-limit") return t("Only measured literal positions are adapted; other overloads, dynamic source, or reflection can differ.");
  if (diagnostic.code === "unicode-limit") return t("Supplementary Unicode operations may differ from MATLAB UTF-16 semantics.");
  return t("Unsupported or uncertain source ({reason}).", { reason: diagnostic.code });
}

export function showAdaptation(entry, metadata) {
  if (!entry || !metadata) return;
  entry.adapterMetadata = metadata;
  let note = entry.wrap.querySelector(".source-adapter-note");
  if (metadata.status === "unchanged" && metadata.scope !== "entry-file") return;
  if (!note) {
    note = registry.el("div", "source-adapter-note");
    entry.wrap.querySelector(".console-command")?.after(note);
  }
  if (metadata.status === "adapted") {
    const replacements = metadata.replacements.filter((item) => item.original_text.startsWith('"'));
    const document = entry.sourceContext?.document || "";
    const lines = [...new Set(replacements.map((item) => document.slice(0, item.original.start_utf16).split(/\r\n|\r|\n/).length))];
    note.textContent = metadata.scope === "entry-file" ? t("Adapted: {count} literals · entry script only; dependencies run natively.", { count: replacements.length }) : t("Adapted: {count} literals", { count: replacements.length });
    note.title = t("Original lines: {lines}", { lines: lines.join(", ") });
    if (metadata.diagnostics.length) note.title += "\n" + metadata.diagnostics.map(diagnosticText).join("\n");
  } else if (metadata.status === "unchanged") {
    note.textContent = t("Entry script executed unchanged; dependencies run natively.");
  } else {
    const diagnostic = metadata.diagnostics[0];
    note.textContent = t("Executed unchanged: {reason}", { reason: diagnosticText(diagnostic) });
    const line = (entry.sourceContext?.document || "").slice(0, diagnostic.start || 0).split(/\r\n|\r|\n/).length;
    note.title = t("Original lines: {lines}", { lines: line }) + "\n" + diagnostic.message;
  }
}

function openSourceLocation(entry, location) {
  const context = entry.sourceContext;
  const tab = shared.tabs.find((item) => item.path === context.path);
  const text = tab === shared.active ? shared.editor.state.doc.toString() : tab?.content;
  if (tab && text === context.document) {
    registry.switchTab(tab);
    shared.editor.dispatch({ selection: { anchor: location.end_utf16, head: location.start_utf16 }, scrollIntoView: true });
    registry.updateCursor();
    shared.editor.focus();
    return;
  }
  const snapshot = registry.el("textarea", "source-snapshot");
  snapshot.readOnly = true;
  snapshot.value = context.document;
  snapshot.setAttribute("aria-label", t("Submitted editor snapshot"));
  registry.modal(t("Submitted editor snapshot"), snapshot);
  snapshot.focus();
  snapshot.setSelectionRange(location.start_utf16, location.end_utf16);
}

export function showSourceErrors(entry, state) {
  if (!entry?.sourceContext || entry.job !== state.job || entry.epoch !== state.source_adapter?.epoch) return;
  if (entry.sourceErrorKey === JSON.stringify(state.source_error_locations || [])) return;
  entry.sourceErrorKey = JSON.stringify(state.source_error_locations || []);
  entry.wrap.querySelector(".source-error-locations")?.remove();
  if (!state.source_error_locations?.length) return;
  const links = registry.el("div", "source-error-locations");
  for (const location of state.source_error_locations) {
    if (location.job !== entry.job || location.epoch !== entry.epoch) continue;
    const button = registry.el("button", "source-error-location", t("Original source: line {line}, column {column}", location));
    if (location.line_only) button.textContent = t("Original source: line {line}", location);
    button.type = "button";
    button.dataset.lineOnly = String(Boolean(location.line_only));
    button.dataset.line = String(location.line);
    button.dataset.column = String(location.column);
    button.onclick = () => openSourceLocation(entry, location);
    links.append(button);
  }
  entry.wrap.append(links);
}

onLanguageChange(() => {
  for (const wrap of document.querySelectorAll(".console-entry")) {
    const entry = wrap.sourceEntry;
    if (entry?.adapterMetadata) showAdaptation(entry, entry.adapterMetadata);
  }
  for (const button of document.querySelectorAll(".source-error-location")) {
    button.textContent = button.dataset.lineOnly === "true" ? t("Original source: line {line}", button.dataset) : t("Original source: line {line}, column {column}", button.dataset);
  }
});
