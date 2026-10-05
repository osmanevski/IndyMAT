import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";
import variableUtils from "./variable_editor_utils.cjs";

const { integerClasses, typedCell, pasteGrid, selectionRect } = variableUtils;
const supportedClasses = new Set(["double", "single", "logical", "char", "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64", "cell", "struct"]);
const tabs = new Map();
let activeName = null;
let pending = null;
let editorEpoch = null;
let modalSession = null;
let displayedSession = null;
let modalHooksInstalled = false;

function startModalSession() {
  if (!modalHooksInstalled) {
    let modal = registry.$('#modal');
    const invalidate = () => {
      if (displayedSession) displayedSession.active = false;
    };
    // close is queued: a new inspection may already be pending or visible.
    // It belongs to the displayed dialog, never a new not-yet-shown session.
    modal.addEventListener('close', () => {
      if (!modal.open) invalidate();
    });
    modal.addEventListener('cancel', invalidate);
    registry.$('#modal-close').addEventListener('pointerdown', invalidate);
    modal.addEventListener('pointerdown', (event) => {
      if (event.target === modal) invalidate();
    });
    modalHooksInstalled = true;
  }
  if (modalSession) modalSession.active = false;
  modalSession = { active: true, shown: false };
}

function sessionIsCurrent(session) {
  // Detect a synchronous close even before its queued event is dispatched.
  return session === modalSession && session?.active && (!session.shown || registry.$('#modal').open);
}

function defaultTab(name) {
  return { name, path: [], row: 1, column: 1, rows: 100, columns: 30, slices: [], data: null, readJob: null, selection: null };
}

function canOpenVariable(variable) {
  return !!variable && supportedClasses.has(variable.class);
}

function setJob(result, request, session) {
  pending = { job: result.job, request, session };
  shared.lastJob = result.job;
  shared.activeOutput = null;
  shared.busy = true;
  registry.setStatus({ status: "running", elapsed: 0 });
}

async function readTab(tab) {
  registry.requireIdle();
  let session = modalSession;
  let request = { action: "read", name: tab.name, path: tab.path, row: tab.row, column: tab.column, rows: tab.rows, columns: tab.columns, slices: tab.slices, epoch: shared.engine.epoch };
  let result = await registry.api("variable", request);
  setJob(result, request, session);
}

async function openVariable(name) {
  if (shared.engine.status === "paused") return registry.debugCommand("eval", name);
  let variable = shared.variables.find((item) => item.name === name);
  if (!canOpenVariable(variable)) return false;
  startModalSession();
  if (editorEpoch !== shared.engine.epoch) {
    tabs.clear();
    editorEpoch = shared.engine.epoch;
  }
  let tab = tabs.get(name) || defaultTab(name);
  tabs.set(name, tab);
  activeName = name;
  await readTab(tab);
  return true;
}

function variableEpochChanged(epoch) {
  if (editorEpoch === null || editorEpoch === epoch) return;
  tabs.clear();
  activeName = null;
  pending = null;
  if (modalSession) modalSession.active = false;
  editorEpoch = epoch;
  if (registry.$("#modal")?.open && registry.$("#modal-body .variable-editor")) registry.$("#modal").close();
}

function normalizeRows(rows, rowCount) {
  if (!rowCount) return [];
  if (rowCount === 1 && Array.isArray(rows) && !Array.isArray(rows[0])) return [rows];
  return rows || [];
}

function normalizeSlices(value) {
  if (value === void 0 || value === null) return [];
  return Array.isArray(value) ? value : [value];
}

function indicesFor(tab, row, column) {
  return [tab.data.row + row, tab.data.column + column, ...normalizeSlices(tab.data.slices)];
}

function breadcrumbLabel(step) {
  if (step.kind === "field") return "." + step.name;
  return (step.kind === "cell" ? "{" : "(") + step.indices.join(",") + (step.kind === "cell" ? "}" : ")");
}

function renderTabs(wrap) {
  let bar = registry.el("div", "variable-tabs");
  bar.setAttribute("role", "tablist");
  bar.setAttribute("aria-label", t("Open variables"));
  for (let tab of tabs.values()) {
    let button = registry.el("button", tab.name === activeName ? "active" : "", tab.name);
    button.setAttribute("role", "tab");
    button.setAttribute("aria-selected", String(tab.name === activeName));
    button.onclick = () => registry.safe(async () => {
      activeName = tab.name;
      await readTab(tab);
    });
    bar.append(button);
  }
  wrap.append(bar);
}

function renderBreadcrumb(wrap, tab) {
  let bar = registry.el("nav", "variable-breadcrumb");
  bar.setAttribute("aria-label", t("Variable path"));
  let root = registry.el("button", "", tab.name);
  root.onclick = () => registry.safe(async () => {
    tab.path = [];
    tab.row = tab.column = 1;
    tab.slices = [];
    await readTab(tab);
  });
  bar.append(root);
  tab.path.forEach((step, index) => {
    let button = registry.el("button", "", breadcrumbLabel(step));
    button.onclick = () => registry.safe(async () => {
      tab.path = tab.path.slice(0, index + 1);
      tab.row = tab.column = 1;
      tab.slices = [];
      await readTab(tab);
    });
    bar.append(button);
  });
  wrap.append(bar);
}

function pageButton(label, disabled, action) {
  let button = registry.el("button", "small-button", label);
  button.disabled = disabled;
  button.onclick = () => registry.safe(action);
  return button;
}

function renderPageControls(wrap, tab) {
  let data = tab.data;
  if (!["matrix", "collection"].includes(data.kind)) return;
  let controls = registry.el("div", "variable-page-controls");
  let rowEnd = data.row + data.row_count - 1;
  let columnEnd = data.column + data.column_count - 1;
  controls.append(pageButton(t("← Column"), data.column <= 1, async () => {
    tab.column = Math.max(1, tab.column - tab.columns);
    await readTab(tab);
  }), pageButton(t("Column →"), columnEnd >= data.column_total, async () => {
    tab.column += tab.columns;
    await readTab(tab);
  }), pageButton(t("↑ Row"), data.row <= 1, async () => {
    tab.row = Math.max(1, tab.row - tab.rows);
    await readTab(tab);
  }), pageButton(t("Row ↓"), rowEnd >= data.row_total, async () => {
    tab.row += tab.rows;
    await readTab(tab);
  }));
  let position = registry.el("span", "variable-position", t("{rowStart}–{rowEnd} of {rowTotal} rows · {columnStart}–{columnEnd} of {columnTotal} columns", { rowStart: data.row_count ? data.row : 0, rowEnd, rowTotal: data.row_total, columnStart: data.column_count ? data.column : 0, columnEnd, columnTotal: data.column_total }));
  controls.append(position);
  wrap.append(controls);
}

function renderSlices(wrap, tab) {
  let sizes = tab.data.size || [];
  if (sizes.length <= 2) return;
  let bar = registry.el("div", "variable-slices");
  bar.append(registry.el("span", "", t("Slice:")));
  let slices = normalizeSlices(tab.data.slices);
  for (let dimension = 2; dimension < sizes.length; dimension++) {
    let label = registry.el("label", "", t("Dimension {dimension}", { dimension: dimension + 1 }));
    let input = registry.el("input");
    input.type = "number";
    input.min = 1;
    input.max = sizes[dimension];
    input.value = slices[dimension - 2] || 1;
    input.setAttribute("aria-label", t("Dimension {dimension} slice", { dimension: dimension + 1 }));
    input.onchange = () => registry.safe(async () => {
      let selected = Number(input.value);
      if (!Number.isSafeInteger(selected) || selected < 1 || selected > sizes[dimension]) throw new Error(t("Slice index must be within the dimension bounds."));
      let next = [...slices];
      next[dimension - 2] = selected;
      tab.slices = next;
      tab.row = tab.column = 1;
      await readTab(tab);
    });
    input.onkeydown = (event) => {
      if (event.key === "Enter") input.blur();
    };
    label.append(input);
    bar.append(label);
  }
  wrap.append(bar);
}

function selectedText(tab) {
  let rect = selectionRect(tab.selection?.anchor, tab.selection?.focus);
  if (!rect) throw new Error(t("Select cells to copy."));
  let rows = normalizeRows(tab.data.rows, tab.data.row_count);
  let output = [];
  for (let row = rect.top; row <= rect.bottom; row++) {
    let values = [];
    for (let column = rect.left; column <= rect.right; column++) {
      let value = rows[row]?.[column];
      values.push(typeof value === "object" ? value.display : value ?? "");
    }
    output.push(values.join("\t"));
  }
  return output.join("\n");
}

async function copySelection(tab, event) {
  let text = selectedText(tab);
  if (event?.clipboardData) {
    event.clipboardData.setData("text/plain", text);
    event.preventDefault();
  } else {
    await navigator.clipboard.writeText(text);
    registry.toast(t("Selection copied to the clipboard."));
  }
}

function writeAllowed(tab, paste = false) {
  let data = tab.data;
  if (tab.path.length || !data.editable || !data.real) throw new Error(t("This view is read-only."));
  if (shared.engine.status !== "idle" || shared.busy || shared.engine.waiting_input) throw new Error(t("Finish the running operation or input request first."));
  if (paste && !["double", "single", "logical", ...integerClasses].includes(data.class)) throw new Error(t("Rectangular paste is limited to numeric and logical matrices."));
}

async function writeRange(tab, row, column, sourceRows) {
  writeAllowed(tab);
  let session = modalSession;
  let data = tab.data;
  let height = sourceRows.length;
  let width = sourceRows[0].length;
  if (height > 100 || width > 30 || height * width > 3000) throw new Error(t("Paste is limited to 100 rows × 30 columns and 3,000 cells."));
  if (data.kind !== 'char-text' && (data.row + row + height - 1 > data.row_total || data.column + column + width - 1 > data.column_total)) throw new Error(t("Paste exceeds the current array size; this version does not expand arrays."));
  let values = sourceRows.flatMap((items) => items.map((text) => typedCell(data.class, text, t)));
  let request = { action: "write", name: tab.name, path: tab.path, row: (data.row || 1) + row, column: (data.column || 1) + column, height, width, slices: normalizeSlices(data.slices), epoch: editorEpoch, read_job: tab.readJob, class: data.root_class, size: data.root_size, values };
  let result = await registry.api("variable", request);
  setJob(result, request, session);
}

async function pasteSelection(tab, text) {
  writeAllowed(tab, true);
  let grid = pasteGrid(text, t);
  let anchor = tab.selection?.anchor || { row: 0, column: 0 };
  await writeRange(tab, anchor.row, anchor.column, grid);
}

function beginCellEdit(tab, cell, row, column, initial) {
  registry.safe(async () => {
    writeAllowed(tab);
    let input = registry.el("input", "variable-cell-input");
    input.value = initial;
    input.setAttribute("aria-label", t("{name} cell at {row},{column}", { name: tab.name, row: tab.data.row + row, column: tab.data.column + column }));
    cell.replaceChildren(input);
    input.focus();
    input.select();
    let done = false;
    let submitting = false;
    let session = modalSession;
    let finish = async (save) => {
      if (done || submitting) return;
      if (!save || !sessionIsCurrent(session) || !registry.$('#modal').open) {
        done = true;
        cell.textContent = initial;
        return;
      }
      submitting = true;
      try {
        if (input.value !== initial) await writeRange(tab, row, column, [[input.value]]);
        else cell.textContent = initial;
        done = true;
      } catch (error) {
        if (sessionIsCurrent(session) && registry.$('#modal').open) input.focus();
        throw error;
      } finally {
        submitting = false;
      }
    };
    input.onkeydown = (event) => {
      event.stopPropagation();
      if (event.key === 'Enter' || event.key === 'Escape') event.preventDefault();
      if (event.key === "Enter") registry.safe(() => finish(true));
      if (event.key === "Escape") registry.safe(() => finish(false));
    };
    // Enter commits; leaving the cell cancels. Closing cannot submit a draft.
    input.onblur = () => registry.safe(() => finish(false));
    input.onclick = (event) => event.stopPropagation();
    input.ondblclick = (event) => event.stopPropagation();
  });
}

function selectCell(tab, table, row, column, extend) {
  let point = { row, column };
  if (!extend || !tab.selection) tab.selection = { anchor: point, focus: point };
  else tab.selection.focus = point;
  let rect = selectionRect(tab.selection.anchor, tab.selection.focus);
  table.querySelectorAll("td[data-row]").forEach((cell) => {
    let cellRow = Number(cell.dataset.row), cellColumn = Number(cell.dataset.column);
    cell.classList.toggle("selected", cellRow >= rect.top && cellRow <= rect.bottom && cellColumn >= rect.left && cellColumn <= rect.right);
  });
}

function openCollectionItem(tab, row, column) {
  let kind = tab.data.collection === "cell" ? "cell" : "element";
  tab.path = [...tab.path, { kind, indices: indicesFor(tab, row, column) }];
  tab.row = tab.column = 1;
  tab.slices = [];
  tab.selection = null;
  return readTab(tab);
}

function renderGrid(wrap, tab) {
  let data = tab.data;
  let rows = normalizeRows(data.rows, data.row_count);
  let tableWrap = registry.el("div", "variable-grid-wrap");
  let table = registry.el("table", "variable-grid");
  table.tabIndex = 0;
  let head = registry.el("tr");
  head.append(registry.el("th", "variable-corner", ""));
  for (let column = 0; column < data.column_count; column++) head.append(registry.el("th", "", data.column + column));
  table.append(head);
  for (let row = 0; row < data.row_count; row++) {
    let tr = registry.el("tr");
    tr.append(registry.el("th", "", data.row + row));
    for (let column = 0; column < data.column_count; column++) {
      let item = rows[row]?.[column];
      let display = typeof item === "object" ? item.display : item ?? "";
      let cell = registry.el("td", data.kind === "collection" ? "navigable" : "", display);
      cell.dataset.row = row;
      cell.dataset.column = column;
      cell.tabIndex = -1;
      cell.onclick = (event) => {
        selectCell(tab, table, row, column, event.shiftKey);
        cell.focus();
      };
      cell.ondblclick = () => data.kind === "collection" ? registry.safe(() => openCollectionItem(tab, row, column)) : beginCellEdit(tab, cell, row, column, String(display));
      cell.onkeydown = (event) => {
        if (event.key === "Enter") data.kind === "collection" ? registry.safe(() => openCollectionItem(tab, row, column)) : beginCellEdit(tab, cell, row, column, String(display));
      };
      tr.append(cell);
    }
    table.append(tr);
  }
  table.oncopy = (event) => registry.safe(() => copySelection(tab, event));
  table.onpaste = (event) => {
    event.preventDefault();
    registry.safe(() => pasteSelection(tab, event.clipboardData.getData("text/plain")));
  };
  tableWrap.append(table);
  wrap.append(tableWrap);
}

function renderStruct(wrap, tab) {
  let list = registry.el("table", "variable-fields");
  let head = registry.el("tr");
  head.append(registry.el("th", "", t("Field")), registry.el("th", "", t("Value")), registry.el("th", "", t("Class")));
  list.append(head);
  for (let field of tab.data.fields || []) {
    let row = registry.el("tr", "navigable");
    row.tabIndex = 0;
    row.append(registry.el("td", "", field.name), registry.el("td", "", field.display), registry.el("td", "", field.class));
    let open = () => {
      tab.path = [...tab.path, { kind: "field", name: field.name }];
      tab.row = tab.column = 1;
      tab.slices = [];
      return readTab(tab);
    };
    row.ondblclick = () => registry.safe(open);
    row.onkeydown = (event) => {
      if (event.key === "Enter") registry.safe(open);
    };
    list.append(row);
  }
  wrap.append(list);
}

function renderActions(wrap, tab) {
  if (tab.data.kind !== "matrix") return;
  let actions = registry.el("div", "variable-actions");
  let copy = registry.el("button", "", t("Copy Selection"));
  copy.onclick = () => registry.safe(() => copySelection(tab));
  let paste = registry.el("button", "", t("Paste from Clipboard"));
  paste.disabled = !tab.data.editable || tab.path.length > 0 || !["double", "single", "logical", ...integerClasses].includes(tab.data.class) || shared.engine.status !== "idle";
  paste.onclick = () => registry.safe(async () => pasteSelection(tab, await navigator.clipboard.readText()));
  actions.append(copy, paste);
  wrap.append(actions);
}

function renderVariableEditor(tab) {
  if (!tab?.data || !sessionIsCurrent(modalSession)) return;
  let data = tab.data;
  let wrap = registry.el("div", "variable-editor");
  renderTabs(wrap);
  renderBreadcrumb(wrap, tab);
  let size = (data.size || []).join(" × ");
  wrap.append(registry.el("p", "subtle variable-meta", `${data.class} · ${size}${data.real === false ? " · " + t("complex") : ""}`));
  if (data.kind !== 'char-text') renderSlices(wrap, tab);
  renderPageControls(wrap, tab);
  renderActions(wrap, tab);
  if (data.kind === 'char-text') renderText(wrap, tab);
  else if (["matrix", "collection"].includes(data.kind)) renderGrid(wrap, tab);
  else if (data.kind === "struct") renderStruct(wrap, tab);
  else wrap.append(registry.el("pre", "variable-detail", data.text || t("Empty value")));
  let note = data.note || t("Pages are limited to 100 rows × 30 columns; only the visible range is transferred.");
  if (data.truncated) note += " " + t("Value is shown in pages.");
  if (tab.path.length || !data.editable || data.real === false) note += " " + t("This view is read-only.");
  else if (data.kind !== 'char-text') note += " " + t("Double-click a cell to edit; Enter saves, Escape or leaving the cell cancels. Paste does not expand the array.");
  wrap.append(registry.el("p", "variable-limit", note));
  registry.modal(t("{name} — Variable View", { name: tab.name }), wrap);
  modalSession.shown = true;
  displayedSession = modalSession;
}

function renderText(wrap, tab) {
  let data = tab.data;
  if (!data.editable) {
    wrap.append(registry.el('pre', 'variable-detail', data.text || t('Empty text')));
    return;
  }
  let form = registry.el('form', 'variable-text-form');
  let input = registry.el('textarea', 'variable-text-input');
  input.setAttribute('aria-label', t('UTF-8 text'));
  input.value = data.text;
  let button = registry.el('button', '', t('Save Text'));
  button.type = 'submit';
  form.append(input, button);
  let session = modalSession;
  form.onsubmit = (event) => {
    event.preventDefault();
    registry.safe(async () => {
      if (!sessionIsCurrent(session) || !registry.$('#modal').open) return;
      button.disabled = true;
      try {
        await writeRange(tab, 0, 0, [[input.value]]);
      } finally {
        button.disabled = false;
      }
    });
  };
  wrap.append(form);
}

async function variableJobCompleted(state) {
  if (!pending || pending.job !== state.job) return;
  let completed = pending;
  pending = null;
  let tab = tabs.get(completed.request.name);
  if (!tab || !sessionIsCurrent(completed.session)) return;
  if (state.error) {
    if (state.kind === 'variable-write' && registry.$('#modal').open) await readTab(tab);
    return;
  }
  if (state.kind === "variable-read") {
    let data = state.variable_action;
    if (!data) return;
    data.rows = normalizeRows(data.rows, data.row_count);
    data.slices = normalizeSlices(data.slices);
    data.root_size = Array.isArray(data.root_size) ? data.root_size : [data.root_size];
    tab.data = data;
    tab.readJob = state.job;
    tab.slices = data.slices;
    tab.selection = null;
    activeName = tab.name;
    renderVariableEditor(tab);
  } else if (state.kind === "variable-write") {
    registry.toast(t("Variable cells updated."));
    await readTab(tab);
  }
}

Object.assign(registry, { canOpenVariable, openVariable, variableEpochChanged, variableJobCompleted });
onLanguageChange(() => {
  let tab = tabs.get(activeName);
  let modal = registry.$("#modal");
  if (tab?.data && modal?.open && modal.querySelector("#modal-body .variable-editor") && !modal.querySelector("input:focus, textarea:focus")) renderVariableEditor(tab);
});
