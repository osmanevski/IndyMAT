import { EditorView } from "@codemirror/view";

import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";

const workspaceSelection = new Set();
let workspaceAnchor = null;
let workspaceFocus = null;
let workspaceSort = { key: "name", direction: 1 };
let workspaceEdit = null;
const editableScalarClasses = new Set(["double", "single", "logical", "char", "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64"]);
let workspaceDialogRenderer = null;
let workspaceDialogNode = null;

// 3×3 glyph of the variable's shape: scalar, row, column, matrix; N-d lights the corner accent.
function shapeGlyph(size) {
  let d = String(size).split("x").map(Number), g = registry.el("span", "shape" + (d.length > 2 ? " nd" : ""));
  let [r, c] = [d[0] || 0, d[1] || 0];
  let rows = Math.min(3, r), cols = Math.min(3, c);
  for (let i = 0; i < 9; i++) {
    let cell = registry.el("i");
    if (Math.floor(i / 3) < rows && i % 3 < cols) cell.className = "on";
    g.append(cell);
  }
  g.setAttribute("aria-hidden", "true");
  return g;
}
function renderVariables() {
  if (workspaceEdit) {
    let live = shared.variables.find((item) => item.name === workspaceEdit.name);
    let same = live && JSON.stringify([live.class, live.size, live.preview, live.global]) === workspaceEdit.signature;
    if (same && shared.engine.epoch === workspaceEdit.epoch && shared.engine.status !== "paused") return;
    workspaceEdit.input.onblur = null;
    workspaceEdit = null;
    registry.toast(t("The variable or session changed; the open edit was canceled. Reopen the variable."));
  }
  let list = registry.$("#variables");
  list.replaceChildren();
  let q = registry.$("#variable-search").value.toLowerCase();
  let visible = shared.variables.filter((v2) => v2.name.toLowerCase().includes(q));
  visible.sort((a, b) => {
    let av = a[workspaceSort.key] || "", bv = b[workspaceSort.key] || "";
    return workspaceSort.direction * String(av).localeCompare(String(bv), "tr", { numeric: true });
  });
  let available = new Set(shared.variables.map((item) => item.name));
  for (let name of workspaceSelection) if (!available.has(name)) workspaceSelection.delete(name);
  if (!visible.some((item) => item.name === workspaceFocus)) workspaceFocus = visible[0]?.name || null;
  document.querySelectorAll(".workspace-sort").forEach((button) => {
    let active = button.dataset.key === workspaceSort.key;
    button.textContent = t(button.getAttribute("data-i18n") || button.dataset.label) + (active ? workspaceSort.direction > 0 ? " ↑" : " ↓" : "");
    button.setAttribute("aria-sort", active ? workspaceSort.direction > 0 ? "ascending" : "descending" : "none");
  });
  for (let [index, v] of visible.entries()) {
    let tr = registry.el("tr");
    tr.tabIndex = workspaceFocus === v.name || !workspaceFocus && index === 0 ? 0 : -1;
    tr.dataset.name = v.name;
    tr.classList.toggle("selected", workspaceSelection.has(v.name));
    tr.setAttribute("aria-selected", workspaceSelection.has(v.name) ? "true" : "false");
    tr.title = t("{name}: {size} {class} · {bytes} bytes", { name: v.name, size: v.size.replaceAll("x", "\xD7"), class: v.class, bytes: v.bytes });
    let name = registry.el("td");
    name.setAttribute("aria-label", v.name);
    let inner = registry.el("span", "var-name");
    inner.append(shapeGlyph(v.size), registry.el("span", "", v.name));
    name.append(inner);
    let value = registry.el("td", "workspace-value", v.preview || v.size);
    tr.append(name, value, registry.el("td", "var-size", v.size.replaceAll("x", "\xD7")), registry.el("td", "", v.class));
    tr.onclick = (event) => selectWorkspaceRow(event, v.name, visible);
    tr.ondblclick = () => registry.safe(() => inspect(v.name));
    tr.onfocus = () => {
      workspaceFocus = v.name;
      updateWorkspaceSelection();
    };
    tr._workspaceShortcutContext = { variable: v, visible };
    if (!registry.registerShortcut) tr.onkeydown = (event) => workspaceKeydown(event, v, visible);
    list.append(tr);
  }
  registry.$("#variable-count").textContent = shared.variables.length;
  registry.$("#workspace-empty").hidden = !!shared.variables.length;
  updateWorkspaceActions();
}

function selectWorkspaceRow(event, name, visible) {
  workspaceFocus = name;
  if (event.shiftKey && workspaceAnchor && visible.some((item) => item.name === workspaceAnchor)) {
    let start = visible.findIndex((item) => item.name === workspaceAnchor), end = visible.findIndex((item) => item.name === name);
    if (!event.metaKey && !event.ctrlKey) workspaceSelection.clear();
    for (let index = Math.min(start, end); index <= Math.max(start, end); index++) workspaceSelection.add(visible[index].name);
  } else if (event.metaKey || event.ctrlKey) {
    if (workspaceSelection.has(name)) workspaceSelection.delete(name);
    else workspaceSelection.add(name);
    workspaceAnchor = name;
  } else {
    workspaceSelection.clear();
    workspaceSelection.add(name);
    workspaceAnchor = name;
  }
  updateWorkspaceSelection();
  registry.$(`#variables tr[data-name="${CSS.escape(name)}"]`)?.focus();
}

function updateWorkspaceSelection() {
  // Keep row nodes intact between the two clicks of a double click.
  for (let row of document.querySelectorAll("#variables tr")) {
    let selected = workspaceSelection.has(row.dataset.name);
    row.classList.toggle("selected", selected);
    row.setAttribute("aria-selected", String(selected));
    row.tabIndex = row.dataset.name === workspaceFocus ? 0 : -1;
  }
  updateWorkspaceActions();
}

function workspaceKeydown(event, variable, visible) {
  if (["ArrowUp", "ArrowDown"].includes(event.key)) {
    event.preventDefault();
    moveWorkspaceSelection(event, variable, visible, event.key === "ArrowUp" ? -1 : 1);
  } else if (event.key === " " || event.key === "Spacebar") {
    event.preventDefault();
    toggleWorkspaceSelection(variable, visible);
  } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
    event.preventDefault();
    selectAllWorkspace(visible);
  } else if (event.key === "Delete" || event.key === "Backspace") {
    event.preventDefault();
    registry.safe(deleteWorkspaceSelection);
  } else if (event.key === "F2") {
    event.preventDefault();
    renameWorkspaceShortcut(event, variable);
  } else if (event.key === "Enter") {
    event.preventDefault();
    registry.safe(() => inspect(variable.name));
  }
}

function moveWorkspaceSelection(event, variable, visible, direction) {
  let index = visible.findIndex((item) => item.name === variable.name), next = visible[Math.max(0, Math.min(visible.length - 1, index + direction))];
  selectWorkspaceRow({ shiftKey: event.shiftKey, metaKey: event.metaKey, ctrlKey: event.ctrlKey }, next.name, visible);
}

function toggleWorkspaceSelection(variable, visible) {
  selectWorkspaceRow({ metaKey: true }, variable.name, visible);
}

function selectAllWorkspace(visible) {
  visible.forEach((item) => workspaceSelection.add(item.name));
  updateWorkspaceSelection();
}

function renameWorkspaceShortcut(event, variable) {
  let row = event.currentTarget?.closest?.("tr");
  if (!row && event.currentTarget?.children) row = event.currentTarget;
  if (!row) row = event.target?.closest?.("tr");
  beginWorkspaceRename(row.children[0], variable);
}

function editableScalar(variable) {
  return variable.size === "1x1" && !variable.complex && editableScalarClasses.has(variable.class);
}

function inlineWorkspaceInput(cell, initial, label, variable, commit) {
  let input = registry.el("input", "workspace-inline");
  input.value = initial;
  input.setAttribute("aria-label", label);
  const edit = { name: variable.name, signature: JSON.stringify([variable.class, variable.size, variable.preview, variable.global]), epoch: shared.engine.epoch, draft: initial, input };
  workspaceEdit = edit;
  input.oninput = () => edit.draft = input.value;
  cell.replaceChildren(input);
  let finished = false;
  const finish = (save) => {
    if (finished || workspaceEdit !== edit) return;
    finished = true;
    workspaceEdit = null;
    if (save && input.value !== initial) Promise.resolve().then(() => commit(input.value)).catch((error) => registry.toast(error.message)).finally(renderVariables);
    else renderVariables();
  };
  input.onkeydown = (event) => {
    event.stopPropagation();
    if (event.key === "Enter") finish(true);
    if (event.key === "Escape") finish(false);
  };
  input.onclick = (event) => event.stopPropagation();
  input.ondblclick = (event) => event.stopPropagation();
  input.onblur = () => finish(true);
  input.focus();
  input.select();
}

function beginWorkspaceRename(cell, variable) {
  if (shared.engine.status === "paused") return registry.toast(t("Workspace is read-only while debugging is paused."));
  if (variable.global) return registry.toast(t("Global variable names cannot be changed in this interface."));
  inlineWorkspaceInput(cell, variable.name, t("New variable name"), variable, async (newName) => {
    newName = newName.trim();
    if (!newName || newName === variable.name) return renderVariables();
    await submitWorkspace({ action: "rename", old_name: variable.name, new_name: newName });
    workspaceSelection.delete(variable.name);
    workspaceSelection.add(newName);
    workspaceFocus = workspaceAnchor = newName;
  });
}

function parseWorkspaceScalar(variable, text) {
  if (variable.class === "logical") {
    if (/^(true|1)$/i.test(text.trim())) return true;
    if (/^(false|0)$/i.test(text.trim())) return false;
    throw new Error(t("Logical values must be true, false, 1, or 0."));
  }
  if (variable.class === "char") {
    if (text.length !== 1 || text.charCodeAt(0) > 127) throw new Error(t("A character value must be a single ASCII character."));
    return text;
  }
  if (["double", "single"].includes(variable.class) && ["NaN", "Inf", "-Inf"].includes(text.trim())) return { special: text.trim() };
  if (!text.trim()) throw new Error(t("Enter a number."));
  let value = Number(text.trim());
  if (!Number.isFinite(value)) throw new Error(t("Enter a finite number."));
  if (variable.class === "single" && Math.abs(value) > 3.4028234663852886e38) throw new Error(t("The value is outside the single-precision range."));
  if (/^(u?int)/.test(variable.class) && !Number.isSafeInteger(value)) throw new Error(t("Enter a safe integer for this integer class."));
  return value;
}

function beginWorkspaceValueEdit(cell, variable) {
  if (!editableScalar(variable)) return registry.toast(t("Only numeric, logical, or character scalars can be edited here."));
  if (shared.engine.status === "paused") return registry.toast(t("Workspace is read-only while debugging is paused."));
  let initial = variable.class === "char" ? variable.preview : variable.preview || "";
  inlineWorkspaceInput(cell, initial, t("{name} scalar value", { name: variable.name }), variable, async (text) => {
    await submitWorkspace({ action: "assign-scalar", name: variable.name, class: variable.class, value: parseWorkspaceScalar(variable, text) });
  });
}

async function submitWorkspace(payload) {
  if (shared.engine.status === "paused") throw new Error(t("Workspace is read-only while debugging is paused."));
  registry.requireIdle();
  let result = await registry.api("workspace", payload);
  shared.lastJob = result.job;
  shared.activeOutput = null;
  shared.busy = true;
  registry.setStatus({ status: "running", elapsed: 0 });
  return result;
}

function selectedWorkspaceNames() {
  return shared.variables.map((item) => item.name).filter((name) => workspaceSelection.has(name));
}

async function deleteWorkspaceSelection(all = false) {
  let names = all ? shared.variables.map((item) => item.name) : selectedWorkspaceNames();
  if (!names.length) throw new Error(t("No variables selected for deletion."));
  if (!confirm(t("Delete these variables?\n\n{names}", { names: names.join("\n") }))) return;
  await submitWorkspace({ action: "clear-names", names, confirm: true });
}

async function workspacePathDialog(title, buttonText, initial) {
  return new Promise((resolve) => {
    let form = registry.el("form", "save-dialog workspace-path-dialog");
    form.append(registry.el("p", "", t("Enter a .mat file path relative to the current folder.")));
    let input = registry.el("input");
    input.value = initial;
    input.required = true;
    input.setAttribute("aria-label", t("MAT-file path"));
    let button = registry.el("button", "", buttonText);
    button.type = "submit";
    form.append(input, button);
    let done = false;
    form.onsubmit = (event) => {
      event.preventDefault();
      done = true;
      resolve(input.value.trim());
      registry.$("#modal").close();
    };
    registry.$("#modal").addEventListener("close", () => {
      if (!done) resolve(null);
    }, { once: true });
    registry.modal(title, form);
    input.focus();
    input.select();
  });
}

async function saveWorkspace(all) {
  let names = all ? [] : selectedWorkspaceNames();
  if (!all && !names.length) throw new Error(t("Select variables to save."));
  let path = await workspacePathDialog(all ? t("Save Workspace") : t("Save Selection"), t("Save"), all ? "workspace.mat" : "selection.mat");
  if (!path) return;
  let payload = { action: "save", path, all, names, overwrite: false };
  try {
    await submitWorkspace(payload);
  } catch (error) {
    if (error.code !== "mat_overwrite_required") throw error;
    if (!confirm(t("{path} already exists. Overwrite it?", { path }))) return;
    let expectedHash = error.details?.expected_hash, approval = error.details?.approval;
    if (!/^[a-f0-9]{64}$/.test(expectedHash || "") || !/^[a-f0-9]{48}$/.test(approval || "")) throw new Error(t("Could not authorize the existing MAT-file; try again."));
    await submitWorkspace({ ...payload, overwrite: true, confirm_overwrite: true, expected_hash: expectedHash, approval });
  }
}

async function loadWorkspace() {
  let path = await workspacePathDialog(t("Load MAT-file"), t("Inspect"), "workspace.mat");
  if (!path) return;
  await submitWorkspace({ action: "load-inspect", path });
}

async function workspaceJobCompleted(state) {
  let action = state.workspace_action;
  if (!action || state.job !== shared.lastJob || state.error) return;
  if (state.kind === "workspace-load-inspect") {
    let replacements = action.replacements || [];
    let shown = replacements.join("\n") || "(Yok)";
    let message = t("{count} variables will be loaded. Existing variables whose values will be replaced:\n\n{names}\n\nLoad them?", { count: action.variables?.length || 0, names: shown });
    if (confirm(message)) await submitWorkspace({ action: "load", path: action.path, hash: action.hash, inspection: action.inspection, replacements, confirm: true });
  } else if (state.kind === "workspace-load") registry.toast(t("MAT-file loaded into the Workspace."));
  else if (state.kind === "workspace-save") registry.toast(t("MAT-file saved atomically."));
}

function updateWorkspaceActions(status = shared.engine.status) {
  let names = selectedWorkspaceNames();
  let count = names.length;
  let variable = count === 1 ? shared.variables.find((item) => item.name === names[0]) : null;
  let unavailable = shared.busy || shared.starting || status !== "idle";
  registry.$("#clear-workspace").disabled = !shared.variables.length || unavailable;
  registry.$("#workspace-delete").disabled = count === 0 || unavailable;
  registry.$("#workspace-edit-value").disabled = !variable || !editableScalar(variable) || unavailable;
  registry.$("#workspace-save-all").disabled = unavailable;
  registry.$("#workspace-save-selection").disabled = count === 0 || unavailable;
  registry.$("#workspace-load").disabled = unavailable;
}

function editWorkspaceSelection() {
  registry.requireIdle();
  let names = selectedWorkspaceNames();
  if (names.length !== 1) throw new Error(t("Select one scalar variable to edit."));
  let variable = shared.variables.find((item) => item.name === names[0]);
  let cell = registry.$(`#variables tr[data-name="${CSS.escape(names[0])}"] .workspace-value`);
  if (!cell) throw new Error(t("The variable must be visible to edit it."));
  beginWorkspaceValueEdit(cell, variable);
}
async function inspect(name) {
  if (shared.engine.status === "paused") return registry.debugCommand("inspect", name);
  registry.requireIdle();
  let variable = shared.variables.find((item) => item.name === name);
  if (registry.canOpenVariable(variable)) return registry.openVariable(name);
  let result = await registry.api("execute", { mode: "inspect", argument: name });
  shared.lastJob = result.job;
  shared.activeOutput = null;
  shared.busy = true;
  registry.setStatus({ status: "running" });
}
function showDetail(d) {
  workspaceDialogRenderer = () => showDetail(d);
  let wrap = registry.el("div");
  workspaceDialogNode = wrap;
  wrap.append(registry.el("p", "subtle", `${d.class} · ${d.size.join(" × ")}${d.truncated ? " · " + t("Preview limited to 100 rows × 30 columns") : ""}`));
  if (d.rows?.length) {
    let table = registry.el("table");
    let head = registry.el("tr");
    head.append(registry.el("th", "", ""));
    d.rows[0].forEach((_, i) => head.append(registry.el("th", "", i + 1)));
    table.append(head);
    d.rows.forEach((row, i) => {
      let tr = registry.el("tr");
      tr.append(registry.el("th", "", i + 1));
      row.forEach((value) => tr.append(registry.el("td", "", value)));
      table.append(tr);
    });
    wrap.append(table);
  } else wrap.append(registry.el("pre", "", d.text || t("Empty variable")));
  registry.modal(t("{name} — Variable View", { name: d.name }), wrap);
}
function showProfile(rows) {
  workspaceDialogRenderer = () => showProfile(rows);
  let wrap = registry.el("div"), note = registry.el("p", "subtle", t("GNU Octave profiler measurements. Times are in seconds; small values may vary between runs.")), table = registry.el("table", "profile-table"), sort = { key: "total", direction: -1 };
  workspaceDialogNode = wrap;
  wrap.append(note, table);
  const columns = [["name", "Function"], ["calls", "Calls"], ["total", "Total time"], ["self", "Self time"]];
  function draw() {
    table.replaceChildren();
    let head = registry.el("tr");
    for (let [key, source] of columns) {
      let title = t(source), th = registry.el("th"), button = registry.el("button", "", title + (sort.key === key ? sort.direction > 0 ? " \u2191" : " \u2193" : ""));
      button.setAttribute("aria-label", t("Sort by {title}", { title }));
      button.onclick = () => {
        sort = sort.key === key ? { key, direction: -sort.direction } : { key, direction: key === "name" ? 1 : -1 };
        draw();
      };
      th.append(button);
      head.append(th);
    }
    table.append(head);
    [...rows || []].sort((a, b) => sort.direction * (sort.key === "name" ? String(a.name).localeCompare(String(b.name), "tr") : Number(a[sort.key]) - Number(b[sort.key]))).forEach((row) => {
      let tr = registry.el("tr", registry.within(row.file || "", shared.folderRoot) ? "openable" : "");
      tr.append(registry.el("td", "", row.name), registry.el("td", "", row.calls), registry.el("td", "", Number(row.total).toFixed(6)), registry.el("td", "", Number(row.self).toFixed(6)));
      if (registry.within(row.file || "", shared.folderRoot)) {
        tr.tabIndex = 0;
        tr.title = t("Go to {path}:{line}", { path: row.file, line: row.line || 1 });
        let open = async () => {
          await registry.openFile(row.file);
          let line = shared.editor.state.doc.line(Math.max(1, Math.min(shared.editor.state.doc.lines, row.line || 1)));
          shared.editor.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: "center" }) });
          shared.editor.focus();
          registry.$("#modal").close();
        };
        tr.onclick = () => registry.safe(open);
        tr.onkeydown = (e) => {
          if (e.key === "Enter") registry.safe(open);
        };
      }
      table.append(tr);
    });
    if (!(rows || []).length) table.after(registry.el("div", "muted-empty", t("No profiling data.")));
  }
  draw();
  registry.modal(t("Profiler Results"), wrap);
}

registry.setupVariableSearch = () => {
  registry.$("#variable-search").oninput = renderVariables;
  for (let button of document.querySelectorAll(".workspace-sort")) {
    button.onclick = () => {
      let key = button.dataset.key;
      workspaceSort = workspaceSort.key === key ? { key, direction: -workspaceSort.direction } : { key, direction: 1 };
      renderVariables();
    };
  }
  registry.on("#workspace-delete", deleteWorkspaceSelection);
  registry.on("#workspace-edit-value", editWorkspaceSelection);
  registry.on("#workspace-save-all", () => saveWorkspace(true));
  registry.on("#workspace-save-selection", () => saveWorkspace(false));
  registry.on("#workspace-load", loadWorkspace);
};
Object.assign(registry, { shapeGlyph, renderVariables, updateWorkspaceActions, inspect, showDetail, showProfile, workspaceJobCompleted, clearWorkspace: () => deleteWorkspaceSelection(true) });
onLanguageChange(() => {
  renderVariables();
  if (registry.$("#modal")?.open && workspaceDialogRenderer && registry.$("#modal-body")?.contains?.(workspaceDialogNode)) workspaceDialogRenderer();
  const labels = {
    "workspace.move-up": "Move up in Workspace",
    "workspace.move-down": "Move down in Workspace",
    "workspace.toggle-selection": "Toggle variable selection",
    "workspace.select-all": "Select all variables",
    "workspace.delete": "Delete selected variables",
    "workspace.rename": "Rename Variable",
    "workspace.open": "Open Variable"
  };
  for (const definition of registry.shortcutDefinitions || []) {
    if (labels[definition.id]) definition.label = t(labels[definition.id]);
    if (definition.scope === "workspace") definition.scopeLabel = t("Workspace");
  }
});
registry.registerShortcut?.({ defaultMatch: {"keys":["ArrowUp"],"modifiers":"any"}, id: "workspace.move-up", label: t("Move up in Workspace"), scope: "workspace", scopeLabel: t("Workspace"), bindings: ["ArrowUp"], ignoreShift: true, when: (event) => !!event.target._workspaceShortcutContext, command: (event) => moveWorkspaceSelection(event, event.target._workspaceShortcutContext.variable, event.target._workspaceShortcutContext.visible, -1) });
registry.registerShortcut?.({ defaultMatch: {"keys":["ArrowDown"],"modifiers":"any"}, id: "workspace.move-down", label: t("Move down in Workspace"), scope: "workspace", scopeLabel: t("Workspace"), bindings: ["ArrowDown"], ignoreShift: true, when: (event) => !!event.target._workspaceShortcutContext, command: (event) => moveWorkspaceSelection(event, event.target._workspaceShortcutContext.variable, event.target._workspaceShortcutContext.visible, 1) });
registry.registerShortcut?.({ defaultMatch: {"keys":[" ","Spacebar"],"modifiers":"any"}, id: "workspace.toggle-selection", label: t("Toggle variable selection"), scope: "workspace", scopeLabel: t("Workspace"), bindings: ["Space"], when: (event) => !!event.target._workspaceShortcutContext, command: (event) => toggleWorkspaceSelection(event.target._workspaceShortcutContext.variable, event.target._workspaceShortcutContext.visible) });
registry.registerShortcut?.({ defaultMatch: {"keys":["a"],"lowercase":true,"modifiers":"primary"}, id: "workspace.select-all", label: t("Select all variables"), scope: "workspace", scopeLabel: t("Workspace"), bindings: ["Mod+KeyA"], when: (event) => !!event.target._workspaceShortcutContext, command: (event) => selectAllWorkspace(event.target._workspaceShortcutContext.visible) });
registry.registerShortcut?.({ defaultMatch: {"keys":["Delete","Backspace"],"modifiers":"any"}, id: "workspace.delete", label: t("Delete selected variables"), scope: "workspace", scopeLabel: t("Workspace"), bindings: ["Delete", "Backspace"], when: (event) => !!event.target._workspaceShortcutContext, command: () => registry.safe(deleteWorkspaceSelection) });
registry.registerShortcut?.({ defaultMatch: {"keys":["F2"],"modifiers":"any"}, id: "workspace.rename", label: t("Rename Variable"), scope: "workspace", scopeLabel: t("Workspace"), bindings: ["F2"], when: (event) => !!event.target._workspaceShortcutContext, command: (event) => renameWorkspaceShortcut(event, event.target._workspaceShortcutContext.variable) });
registry.registerShortcut?.({ defaultMatch: {"keys":["Enter"],"modifiers":"any"}, id: "workspace.open", label: t("Open Variable"), scope: "workspace", scopeLabel: t("Workspace"), bindings: ["Enter"], when: (event) => !!event.target._workspaceShortcutContext, command: (event) => registry.safe(() => inspect(event.target._workspaceShortcutContext.variable.name)) });
