import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";

function requireIdle() {
  if (shared.busy || shared.starting || shared.engine.status === "paused") throw new Error(t("Finish the running operation or debugging session first."));
}
function pointAt(path, line) {
  return shared.breakpoints.get(path)?.get(line);
}
function breakpointPathAffected(path, source) {
  return path === source || path.startsWith(source + "/");
}
function relocateBreakpointPaths(source, destination) {
  let relocated = new Map();
  for (let [path, points] of shared.breakpoints) {
    if (breakpointPathAffected(path, source)) {
      if (destination === null) continue;
      path = destination + path.slice(source.length);
    }
    let existing = relocated.get(path);
    relocated.set(path, existing ? new Map([...existing, ...points]) : points);
  }
  shared.breakpoints = relocated;
  shared.debugPanelKey = "";
  registry.renderDebuggerPanel(shared.engine);
  registry.refreshDebugEditor();
}
function breakpointPayload(path, line, action, condition, enabled) {
  let payload = { path, line, action };
  if (condition !== void 0) payload.condition = condition;
  if (enabled !== void 0) payload.enabled = enabled;
  return payload;
}
async function changeBreakpoint(path, line, action, condition, enabled) {
  requireIdle();
  let result = await registry.api("breakpoint", breakpointPayload(path, line, action, condition, enabled));
  shared.breakpointKey = "";
  shared.debugPanelKey = "";
  shared.lastJob = result.job;
  shared.activeOutput = null;
  registry.setStatus({ ...shared.engine, status: "running" });
}
async function toggleBreakpoint(line) {
  if (!shared.active || !shared.active.path.endsWith(".m") || !shared.active.hash || shared.active.dirty) throw new Error(t("Save the .m file before setting a breakpoint."));
  let current = pointAt(shared.active.path, line);
  await changeBreakpoint(shared.active.path, line, current ? "remove" : "set", current ? void 0 : "", current ? void 0 : true);
  registry.refreshDebugEditor();
}
async function editBreakpointCondition(line, path = shared.active?.path) {
  if (!path || !path.endsWith(".m")) throw new Error(t("Select a saved .m file for the condition."));
  let current = pointAt(path, line), value = prompt(t("Breakpoint condition (leave blank to stop unconditionally):"), current?.condition || "");
  if (value === null) return;
  await changeBreakpoint(path, line, "set", value, current?.enabled ?? true);
}
async function debugCommand(command, code = "") {
  await registry.api("debug", { command, code });
  registry.setStatus({ ...shared.engine, status: "running" });
  registry.refreshDebugEditor();
}
async function clearBreakpoints() {
  requireIdle();
  let result = await registry.api("breakpoints-clear", {});
  shared.breakpointKey = "";
  shared.debugPanelKey = "";
  shared.lastJob = result.job;
  registry.setStatus({ ...shared.engine, status: "running" });
}
async function openDebugLocation(path, line) {
  await registry.openFile(path);
  registry.revealLine(line);
}
async function switchDebugFrame(index) {
  await debugCommand("frame", index);
}
async function runToCursor() {
  if (!shared.active || !shared.active.path.endsWith(".m") || !shared.active.hash || shared.active.dirty) throw new Error(t("Save the .m file before running to the cursor."));
  if (shared.engine.status !== "paused" || !shared.engine.debug?.ready) throw new Error(t("Run to Cursor is available only while the debugger is paused."));
  let line = shared.editor.state.doc.lineAt(shared.editor.state.selection.main.head).number;
  await registry.api("run-to-cursor", { path: shared.active.path, line });
  registry.setStatus({ ...shared.engine, status: "running" });
  registry.refreshDebugEditor();
}
function renderStack(stack, ready) {
  let root = registry.$("#debug-stack");
  root.replaceChildren();
  for (let frame of stack) {
    let button = registry.el("button", "debug-frame" + (frame.current ? " current" : ""));
    button.disabled = !ready;
    button.dataset.frame = frame.index;
    button.setAttribute("aria-current", frame.current ? "true" : "false");
    button.append(registry.el("b", "", frame.name));
    button.append(registry.el("span", "", `${frame.file.split("/").pop()}:${frame.line}`));
    button.title = `${frame.file}:${frame.line}`;
    button.onclick = () => registry.safe(() => switchDebugFrame(frame.index));
    root.append(button);
  }
}
function renderBreakpointRows() {
  let root = registry.$("#debug-breakpoints");
  root.replaceChildren();
  let editable = shared.engine.status === "idle", total = 0;
  for (let [path, points] of [...shared.breakpoints].sort((a, b) => a[0].localeCompare(b[0]))) {
    let group = registry.el("section", "debug-file-group");
    let heading = registry.el("button", "debug-file-name", path.split("/").pop());
    heading.title = path;
    heading.onclick = () => registry.safe(() => openDebugLocation(path, [...points.keys()].sort((a, b) => a - b)[0]));
    group.append(heading);
    for (let [line, point] of [...points].sort((a, b) => a[0] - b[0])) {
      total++;
      let row = registry.el("div", "debug-breakpoint-row" + (point.enabled ? "" : " disabled"));
      let enabled = registry.el("input");
      enabled.type = "checkbox";
      enabled.checked = point.enabled;
      enabled.disabled = !editable;
      enabled.setAttribute("aria-label", t("Enable breakpoint at {path}:{line}", { path, line }));
      enabled.onchange = () => registry.safe(() => changeBreakpoint(path, line, enabled.checked ? "enable" : "disable"));
      let location = registry.el("button", "debug-breakpoint-location", t("Line {line}", { line }));
      location.title = `${path}:${line}`;
      location.onclick = () => registry.safe(() => openDebugLocation(path, line));
      let condition = registry.el("button", "debug-breakpoint-condition", point.condition || t("Add Condition"));
      condition.disabled = !editable;
      condition.title = point.condition ? t("Condition: {condition}. Octave stops here if it errors.", { condition: point.condition }) : t("Add Condition");
      condition.onclick = () => registry.safe(() => editBreakpointCondition(line, path));
      let remove = registry.el("button", "debug-breakpoint-remove", "\xD7");
      remove.disabled = !editable;
      remove.title = t("Remove Breakpoint");
      remove.setAttribute("aria-label", t("Remove breakpoint at {path}:{line}", { path, line }));
      remove.onclick = () => registry.safe(() => changeBreakpoint(path, line, "remove"));
      row.append(enabled, location, condition, remove);
      group.append(row);
    }
    root.append(group);
  }
  registry.$("#debug-breakpoint-count").textContent = total;
  registry.$("#clear-breakpoints").disabled = !editable || !total;
}
function renderDebuggerPanel(state = shared.engine) {
  let rows = [...shared.breakpoints].map(([file, points]) => [file, [...points].map(([line, point]) => [line, point.enabled, point.condition])]);
  let key = JSON.stringify([state.status, state.debug?.serial, state.run_to_cursor, rows]);
  if (key === shared.debugPanelKey) return;
  shared.debugPanelKey = key;
  let paused = state.status === "paused", hasBreakpoints = rows.some(([, points]) => points.length);
  registry.$("#debugger-panel").hidden = !paused && !hasBreakpoints;
  registry.$("#debug-stack-section").hidden = !paused;
  registry.$("#debugger-state").textContent = paused ? state.run_to_cursor ? t("Waiting to run to cursor") : t("Paused") : t("Breakpoints");
  renderStack(state.debug?.stack || [], !!state.debug?.ready);
  renderBreakpointRows();
}
Object.assign(registry, { requireIdle, pointAt, breakpointPayload, relocateBreakpointPaths, changeBreakpoint, toggleBreakpoint, editBreakpointCondition, debugCommand, clearBreakpoints, openDebugLocation, switchDebugFrame, runToCursor, renderStack, renderBreakpointRows, renderDebuggerPanel });
onLanguageChange(() => { shared.debugPanelKey = ""; renderDebuggerPanel(shared.engine); });
export { relocateBreakpointPaths };
