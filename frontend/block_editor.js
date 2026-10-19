// IndyMAT block diagram editor: an own SVG+DOM view over block_model_utils.cjs.
// A diagram tab is an ordinary editor tab whose content is the diagram file text,
// so dirty state, drafts, close prompts and the atomic hash-checked save are the
// text editor's. Run goes through the ordinary execute path as visible code.
import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";
import blocks from "./block_model_utils.cjs";

const NS = "http://www.w3.org/2000/svg";
const W = blocks.BLOCK_WIDTH;
const runs = new Map();
let root = null, svg, world, wireLayer, blockLayer, ghost, panel, statusLine, zoomLabel, typeSelect, active = null, drag = null, lastRender = 0;

const svgEl = (name, attrs = {}, parent) => {
  const node = document.createElementNS(NS, name);
  for (const key of Object.keys(attrs)) node.setAttribute(key, attrs[key]);
  if (parent) parent.append(node);
  return node;
};

// ---------------------------------------------------------------------------
// Messages: every code from block_model_utils maps to one literal English source.
function describe(error) {
  const p = error?.params || {};
  switch (error?.code) {
    case "json": return t("Invalid JSON at character {position}.", p);
    case "duplicate-key": return t("Duplicate JSON key {key}.", p);
    case "object": return t("{context} must be an object.", p);
    case "missing-field": return t("{context} is missing field {field}.", p);
    case "unknown-field": return t("{context} has unsupported field {field}.", p);
    case "imb-version": return t("imbVersion must be 1.");
    case "schema-version": return t("schemaVersion must be 1.");
    case "array": return t("{context} must be an array.", p);
    case "block-id": return t("Block {index} id must match {pattern}.", p);
    case "duplicate-id": return t("Block id {id} is already used.", p);
    case "type": return t("Block {id}: unsupported type {type}.", p);
    case "number": return t("{context} must be a finite number.", p);
    case "vector": return t("{context} must be a finite number or a nonempty array of finite numbers.", p);
    case "mux-inputs": return t("Block {id} inputs must be a whole number from 1 to {limit}.", p);
    case "too-many-inputs": return t("Block {id} has more than {limit} inputs.", p);
    case "state-missing": return t("Block {id} requires initialState.", p);
    case "state-forbidden": return t("Block {id} does not accept initialState.", p);
    case "text": return t("{context} must be text.", p);
    case "mode": return t("simulation.mode must be continuous or discrete.");
    case "endpoint-block": return t("Connection {index} refers to unknown block {id}.", p);
    case "endpoint-port": return t("Connection {index} uses a port that block {id} does not have.", p);
    case "layout-extra": return t("The layout has a position for unknown block {id}.", p);
    case "layout-missing": return t("The layout has no position for block {id}.", p);
    case "coordinate": return t("{context} must be a finite number between -{limit} and {limit}.", p);
    case "no-block": return t("Block {id} does not exist.", p);
    case "no-output": return t("Block {id} has no output.", p);
    case "no-input": return t("Block {id} has no input {port}.", p);
    case "input-taken": return t("Input {port} of {id} is already connected.", p);
    case "no-connection": return t("The connection no longer exists.");
    case "block-name": return t("A block name must match {pattern}.", p);
    case "numbers-only": return t("{name}: enter numbers only, separated by spaces or commas.", p);
    case "scalar": return t("{name}: enter exactly one number.", p);
    case "integer-range": return t("{name}: enter a whole number from {min} to {max}.", p);
    case "lower-upper": return t("{name}: lower must not exceed upper.", p);
    case "unknown-parameter": return t("{name} is not a parameter of this block.", p);
    case "too-many": return t("{name}: at most {max} values.", p);
    case "signs": return t("{name}: every sign must be 1 or -1.", p);
    case "degree": return t("{name}: the denominator needs 2 to 65 coefficients.", p);
    case "proper": return t("{name}: num/den must be strictly proper with nonzero leading coefficients.", p);
    case "grid-values": return t("{name}: enter exactly {count} values, one per simulation time.", p);
    case "positive": return t("{name} must be positive.", p);
    case "t0-tf": return t("{name}: t0 must be less than tf.", p);
    default: return t("The block diagram could not be read.");
  }
}
function typeOption(type) {
  const mode = blocks.CATALOG[type].mode;
  if (mode === "continuous") return t("{type} (continuous only)", { type });
  if (mode === "discrete") return t("{type} (discrete only)", { type });
  return type;
}

// ---------------------------------------------------------------------------
// Tabs.
const isDiagramTab = (tab) => !!tab && tab.view === "diagram" && !!tab.diagram;
function canonicalText(text) {
  if (typeof text !== "string") return null;
  const read = blocks.readDiagram(text);
  return read.ok ? new blocks.DiagramCore(read.file).text() : null;
}
function attach(tab, file) {
  const core = new blocks.DiagramCore(file);
  tab.diagram = { core, view: null, selection: null, message: null, errorTarget: null, wire: null, baselineFor: undefined, baseline: null, lastText: tab.content };
  core.listeners.push((kind) => changed(tab, kind));
  tab.view = "diagram";
  tab.diagramProblem = null;
}
// Decides the view the first time a diagram file is shown. A file that fails
// validation opens as text with the reason.
function prepareDiagramTab(tab) {
  if (!tab || !blocks.isDiagramPath(tab.path)) return false;
  if (tab.view === "text") return false;
  // Content replaced from outside the editor (for example a reload from disk) is read again.
  if (isDiagramTab(tab) && tab.content === tab.diagram.lastText) return true;
  const read = blocks.readDiagram(tab.content);
  if (!read.ok) {
    tab.view = "text";
    tab.diagramProblem = describe(read.error);
    registry.toast(t("Opened as text: {reason}", { reason: tab.diagramProblem }));
    return false;
  }
  attach(tab, read.file);
  return true;
}
function syncText(tab) {
  const d = tab.diagram;
  if (d.baselineFor !== tab.saved) {
    d.baselineFor = tab.saved;
    d.baseline = canonicalText(tab.saved);
  }
  const text = d.core.text();
  tab.content = text === d.baseline && typeof tab.saved === "string" ? tab.saved : text;
  tab.dirty = tab.content !== tab.saved;
  d.lastText = tab.content;
}
function changed(tab, kind) {
  syncText(tab);
  tab.diagram.errorTarget = null;
  if (tab.diagram.message?.kind === "error") tab.diagram.message = null;
  const d = tab.diagram;
  if (d.selection?.kind === "block" && !d.core.block(d.selection.id)) d.selection = null;
  if (d.selection?.kind === "connection" && d.selection.index >= d.core.connections.length) d.selection = null;
  registry.renderTabs();
  registry.persistDrafts();
  if (tab === active) render(kind === "parameter" || kind === "simulation" ? "keep-panel" : "full");
}
function openAsText(tab = shared.active) {
  if (!tab) return;
  tab.view = "text";
  tab.diagram = null;
  tab.diagramProblem = null;
  tab.state = null;
  shared.active = null;
  registry.switchTab(tab);
}
function openAsDiagram(tab = shared.active) {
  if (!tab) return;
  const read = blocks.readDiagram(tab.content);
  if (!read.ok) {
    tab.diagramProblem = describe(read.error);
    registry.renderTabs();
    throw new Error(t("Opened as text: {reason}", { reason: tab.diagramProblem }));
  }
  attach(tab, read.file);
  shared.active = null;
  registry.switchTab(tab);
}
function showDiagramTab(tab) {
  ensureRoot();
  active = tab;
  root.hidden = !tab;
  registry.$("#editor").hidden = !!tab;
  if (!tab) return;
  syncText(tab);
  if (!tab.diagram.view) requestAnimationFrame(() => {
    if (active === tab && !tab.diagram.view) fit();
  });
  render("full");
}
function renderDiagramCaption() {
  ensureRoot();
  const tab = shared.active, button = registry.$("#open-as-diagram"), note = registry.$("#diagram-note");
  const textDiagram = !!tab && blocks.isDiagramPath(tab.path) && !isDiagramTab(tab);
  button.hidden = !textDiagram;
  button.textContent = t("Open as Diagram");
  note.hidden = !(textDiagram && tab.diagramProblem);
  note.textContent = textDiagram && tab.diagramProblem ? t("Opened as text: {reason}", { reason: tab.diagramProblem }) : "";
  note.title = note.textContent;
}
function diagramFooter() {
  const core = active?.diagram?.core;
  if (!core) return false;
  const selection = active.diagram.selection;
  registry.$("#cursor").textContent = selection?.kind === "block" ? t("Selected: {id}", { id: selection.id }) : selection?.kind === "connection" ? t("Selected: connection {index}", { index: selection.index + 1 }) : t("Nothing selected");
  registry.$("#lines").textContent = t("{blocks} blocks · {connections} connections", { blocks: core.blocks.length, connections: core.connections.length });
  return true;
}

// ---------------------------------------------------------------------------
// DOM.
function ensureRoot() {
  if (root) return;
  root = registry.el("div", "block-editor");
  root.id = "block-editor";
  root.hidden = true;
  const bar = registry.el("div", "block-toolbar");
  bar.setAttribute("role", "toolbar");
  typeSelect = registry.el("select");
  typeSelect.id = "block-type";
  const add = registry.el("button");
  add.id = "block-add";
  add.onclick = () => registry.safe(() => addBlock(typeSelect.value));
  const zoomOut = registry.el("button", "block-icon-button", "−");
  zoomOut.id = "block-zoom-out";
  zoomOut.onclick = () => zoomBy(1 / 1.2);
  zoomLabel = registry.el("span", "block-zoom");
  zoomLabel.id = "block-zoom";
  const zoomIn = registry.el("button", "block-icon-button", "+");
  zoomIn.id = "block-zoom-in";
  zoomIn.onclick = () => zoomBy(1.2);
  const fitButton = registry.el("button");
  fitButton.id = "block-fit";
  fitButton.onclick = () => fit();
  const text = registry.el("button");
  text.id = "block-open-text";
  text.onclick = () => registry.safe(() => openAsText(active));
  statusLine = registry.el("span", "block-status");
  statusLine.id = "block-status";
  statusLine.setAttribute("role", "status");
  statusLine.setAttribute("aria-live", "polite");
  bar.append(typeSelect, add, registry.el("span", "block-separator"), zoomOut, zoomLabel, zoomIn, fitButton, registry.el("span", "block-separator"), text, statusLine);
  const main = registry.el("div", "block-main");
  const canvas = registry.el("div", "block-canvas");
  svg = svgEl("svg", { class: "block-svg", role: "application" });
  world = svgEl("g", {}, svg);
  wireLayer = svgEl("g", { class: "block-wires" }, world);
  blockLayer = svgEl("g", { class: "block-nodes" }, world);
  ghost = svgEl("path", { class: "block-wire-ghost", display: "none" }, world);
  canvas.append(svg);
  panel = registry.el("aside", "block-panel");
  panel.id = "block-panel";
  main.append(canvas, panel);
  root.append(bar, main);
  registry.$("#editor").after(root);
  const caption = registry.$(".editor-caption-actions");
  const note = registry.el("span", "diagram-note");
  note.id = "diagram-note";
  note.hidden = true;
  const openDiagram = registry.el("button", "diagram-switch");
  openDiagram.id = "open-as-diagram";
  openDiagram.hidden = true;
  openDiagram.onclick = () => registry.safe(() => openAsDiagram(shared.active));
  caption.prepend(note, openDiagram);
  installEvents();
  labels();
  root.blockEditorTest = {
    core: () => active?.diagram?.core || null,
    view: () => active?.diagram?.view || null,
    render: () => { const start = performance.now(); render("full"); return performance.now() - start; },
    lastRender: () => lastRender,
    setView: (view) => { active.diagram.view = { ...view }; applyView(); },
    // Centre a block on the canvas at zoom k (drift measurements).
    centre: (id, k) => {
      const r = svg.getBoundingClientRect(), p = active.diagram.core.layout[id];
      active.diagram.view = { k, x: r.width / 2 - (p.x + W / 2) * k, y: r.height / 2 - (p.y + 26) * k };
      applyView();
    },
    wirePoint: (index) => {
      const path = wireLayer.querySelector(`[data-index="${index}"] .block-wire-line`);
      if (!path) return null;
      const length = path.getTotalLength(), point = path.getPointAtLength(length / 2), m = path.getScreenCTM();
      return { x: m.a * point.x + m.c * point.y + m.e, y: m.b * point.x + m.d * point.y + m.f };
    },
    screenOf: (id) => {
      const g = blockLayer.querySelector(`[data-id="${id}"] rect`);
      const r = g?.getBoundingClientRect();
      return r ? { x: r.x, y: r.y, width: r.width, height: r.height } : null;
    },
    portScreen: (id, side, port) => {
      const c = blockLayer.querySelector(`[data-id="${id}"] .block-port[data-side="${side}"][data-port="${port}"]`);
      const r = c?.getBoundingClientRect();
      return r ? { x: r.x + r.width / 2, y: r.y + r.height / 2 } : null;
    }
  };
}
function labels() {
  if (!root) return;
  root.setAttribute("aria-label", t("Block diagram"));
  root.querySelector(".block-toolbar").setAttribute("aria-label", t("Block diagram"));
  const current = typeSelect.value;
  typeSelect.replaceChildren(...blocks.BLOCK_TYPES.map((type) => {
    const option = registry.el("option", "", typeOption(type));
    option.value = type;
    return option;
  }));
  typeSelect.value = current || "Gain";
  typeSelect.setAttribute("aria-label", t("Block type"));
  registry.$("#block-add").textContent = t("Add Block");
  registry.$("#block-zoom-out").setAttribute("aria-label", t("Zoom out"));
  registry.$("#block-zoom-out").title = t("Zoom out");
  registry.$("#block-zoom-in").setAttribute("aria-label", t("Zoom in"));
  registry.$("#block-zoom-in").title = t("Zoom in");
  registry.$("#block-fit").textContent = t("Fit");
  registry.$("#block-fit").title = t("Fit the diagram to the view");
  registry.$("#block-open-text").textContent = t("Open as Text");
  svg.setAttribute("aria-label", t("Block diagram canvas. Arrow keys move a block, W starts a connection, Delete removes."));
  panel.setAttribute("aria-label", t("Properties"));
}

// ---------------------------------------------------------------------------
// View transform.
function applyView() {
  const v = active.diagram.view;
  world.setAttribute("transform", `translate(${v.x} ${v.y}) scale(${v.k})`);
  zoomLabel.textContent = Math.round(v.k * 100) + "%";
}
function fit() {
  if (!active) return;
  const b = blocks.bounds(active.diagram.core), r = svg.getBoundingClientRect();
  if (!r.width || !r.height) return;
  const k = Math.max(0.25, Math.min(1.5, Math.min(r.width / b.width, r.height / b.height)));
  active.diagram.view = { k, x: (r.width - b.width * k) / 2 - b.x * k, y: (r.height - b.height * k) / 2 - b.y * k };
  applyView();
}
function zoomAt(factor, px, py) {
  const v = active.diagram.view, k = Math.max(0.25, Math.min(3, v.k * factor));
  v.x = px - (px - v.x) / v.k * k;
  v.y = py - (py - v.y) / v.k * k;
  v.k = k;
  applyView();
}
function zoomBy(factor) {
  if (!active) return;
  const r = svg.getBoundingClientRect();
  zoomAt(factor, r.width / 2, r.height / 2);
}
function worldPoint(event) {
  const p = svg.createSVGPoint();
  p.x = event.clientX;
  p.y = event.clientY;
  return p.matrixTransform(world.getScreenCTM().inverse());
}
function reveal(id) {
  const d = active.diagram, p = d.core.layout[id], r = svg.getBoundingClientRect(), v = d.view;
  if (!p || !v) return;
  const x = v.x + p.x * v.k, y = v.y + p.y * v.k, w = W * v.k;
  if (x < 0 || y < 0 || x + w > r.width || y + 40 * v.k > r.height) {
    v.x = r.width / 2 - (p.x + W / 2) * v.k;
    v.y = r.height / 2 - (p.y + 26) * v.k;
    applyView();
  }
}

// ---------------------------------------------------------------------------
// Drawing.
function summary(block) {
  const p = block.params, list = (v) => [].concat(v).join(" ");
  switch (block.type) {
    case "Constant": return String(p.value);
    case "Step": return `${p.before} → ${p.after} @ ${p.time}`;
    case "Sine": return `${p.amplitude}·sin(${p.omega}t)`;
    case "Gain": return String(p.gain);
    case "Sum": return [].concat(p.signs).map((s) => s > 0 ? "+" : "−").join(" ");
    case "Saturation": case "ClampingGate": return `[${p.lower}, ${p.upper}]`;
    case "Integrator": case "UnitDelay": return "x0 = " + block.initialState;
    case "TransferFcn": return `[${list(p.num)}] / [${list(p.den)}]`;
    case "Mux": return String(p.inputs);
    case "SampledInput": return t("{count} values", { count: [].concat(p.values).length });
    default: return "";
  }
}
function portLabel(block, port) {
  if (block.type === "Sum") return [].concat(block.params.signs)[port - 1] > 0 ? "+" : "−";
  if (block.type === "ClampingGate") return port === 1 ? "e" : "v";
  return "";
}
function drawBlock(block) {
  const d = active.diagram, pos = d.core.layout[block.id], ports = blocks.ports(block), h = blocks.blockHeight(block);
  const selected = d.selection?.kind === "block" && d.selection.id === block.id;
  const failed = d.errorTarget?.kind === "block" && d.errorTarget.id === block.id;
  const g = svgEl("g", { class: "block-node" + (selected ? " selected" : "") + (failed ? " failed" : ""), transform: `translate(${pos.x} ${pos.y})`, "data-id": block.id, tabindex: 0, role: "button" }, blockLayer);
  g.setAttribute("aria-label", t("{id}, {type}, {inputs} inputs, {outputs} outputs", { id: block.id, type: block.type, inputs: ports.inputs, outputs: ports.outputs }));
  svgEl("rect", { width: W, height: h, rx: 7 }, g);
  const name = svgEl("text", { x: W / 2, y: h / 2 - 4, class: "block-name" }, g);
  name.textContent = block.id.length > 16 ? block.id.slice(0, 15) + "…" : block.id;
  const sub = svgEl("text", { x: W / 2, y: h / 2 + 12, class: "block-sub" }, g);
  const text = block.type + " " + summary(block);
  sub.textContent = text.length > 22 ? text.slice(0, 21) + "…" : text;
  const tip = svgEl("title", {}, g);
  tip.textContent = block.id + " · " + block.type + " " + summary(block);
  for (let port = 1; port <= ports.inputs; port++) {
    const y = blocks.portY(block, "in", port);
    svgEl("circle", { class: "block-port in", cx: 0, cy: y, r: 5, "data-side": "in", "data-port": port }, g);
    const label = portLabel(block, port);
    if (label) svgEl("text", { x: 9, y: y + 4, class: "block-port-label" }, g).textContent = label;
  }
  for (let port = 1; port <= ports.outputs; port++) svgEl("circle", { class: "block-port out", cx: W, cy: blocks.portY(block, "out", port), r: 5, "data-side": "out", "data-port": port }, g);
  return g;
}
function drawWires() {
  const d = active.diagram, lanes = blocks.wireLanes(d.core);
  wireLayer.replaceChildren();
  d.core.connections.forEach((link, index) => {
    const geometry = blocks.wireGeometry(d.core, link, lanes.get(index) || 0);
    const selected = d.selection?.kind === "connection" && d.selection.index === index;
    const failed = d.errorTarget?.kind === "connection" && d.errorTarget.index === index;
    const g = svgEl("g", { class: "block-wire" + (geometry.feedback ? " feedback" : "") + (selected ? " selected" : "") + (failed ? " failed" : ""), "data-index": index, tabindex: 0, role: "button" }, wireLayer);
    g.setAttribute("aria-label", t("Connection from {from} to {to} input {port}", { from: link.from.block, to: link.to.block, port: link.to.port }));
    svgEl("path", { class: "block-wire-line", d: geometry.d }, g);
    svgEl("path", { class: "block-wire-hit", d: geometry.d }, g);
  });
}
function render(mode = "full") {
  if (!active || !root) return;
  const start = performance.now();
  const d = active.diagram;
  if (mode === "full" || mode === "keep-panel") {
    const focused = document.activeElement?.closest?.(".block-node")?.dataset.id;
    const focusedWire = document.activeElement?.closest?.(".block-wire")?.dataset.index;
    const hadFocus = svg.contains(document.activeElement);
    blockLayer.replaceChildren();
    for (const block of d.core.blocks) drawBlock(block);
    drawWires();
    if (focused && d.core.block(focused)) blockLayer.querySelector(`[data-id="${focused}"]`)?.focus({ preventScroll: true });
    else if (focusedWire !== undefined && d.selection?.kind === "connection") wireLayer.querySelector(`[data-index="${d.selection.index}"]`)?.focus({ preventScroll: true });
    // A deleted block or wire keeps keyboard focus on the canvas (Undo, Tab).
    if (hadFocus && !svg.contains(document.activeElement)) svg.focus({ preventScroll: true });
  }
  if (d.view) applyView();
  if (mode !== "keep-panel" || !panel.contains(document.activeElement)) renderPanel();
  renderStatus();
  registry.updateCursor();
  lastRender = performance.now() - start;
}
function renderStatus() {
  const d = active.diagram;
  statusLine.classList.toggle("error", d.message?.kind === "error");
  if (d.wire) {
    const target = d.wire.targets[d.wire.index];
    statusLine.textContent = target ? t("Target: {id} input {port}. ↑/↓ choose, Enter connects, Esc cancels.", { id: target.id, port: target.port }) : t("No free input to connect.");
    return;
  }
  statusLine.textContent = d.message?.text || "";
  statusLine.title = statusLine.textContent;
}
function select(selection, focus = false) {
  const d = active.diagram;
  d.selection = selection;
  for (const node of blockLayer.querySelectorAll(".block-node")) node.classList.toggle("selected", selection?.kind === "block" && node.dataset.id === selection.id);
  for (const node of wireLayer.querySelectorAll(".block-wire")) node.classList.toggle("selected", selection?.kind === "connection" && Number(node.dataset.index) === selection.index);
  renderPanel();
  registry.updateCursor();
  if (focus && selection?.kind === "block") blockLayer.querySelector(`[data-id="${selection.id}"]`)?.focus({ preventScroll: true });
  if (focus && selection?.kind === "connection") wireLayer.querySelector(`[data-index="${selection.index}"]`)?.focus({ preventScroll: true });
}
function setMessage(text, kind = "info") {
  if (!active) return;
  active.diagram.message = text ? { text, kind } : null;
  renderStatus();
}
function report(result) {
  if (result?.error) setMessage(describe(result.error), "error");
  return result;
}

// ---------------------------------------------------------------------------
// Property panel.
function field(label, value, onCommit, options = {}) {
  const wrap = registry.el("label", "block-field");
  wrap.append(registry.el("span", "", label));
  const input = registry.el("input");
  input.value = value;
  input.spellcheck = false;
  input.autocomplete = "off";
  if (options.name) input.dataset.param = options.name;
  if (options.readOnly) input.readOnly = true;
  const error = registry.el("span", "block-field-error");
  error.setAttribute("aria-live", "polite");
  input.onchange = () => {
    const message = onCommit(input.value);
    input.setAttribute("aria-invalid", message ? "true" : "false");
    error.textContent = message || "";
  };
  input.onkeydown = (event) => {
    if (event.key === "Escape") {
      input.value = value;
      input.setAttribute("aria-invalid", "false");
      error.textContent = "";
    }
  };
  wrap.append(input, error);
  return wrap;
}
function renderPanel() {
  const d = active.diagram, core = d.core, s = d.selection;
  panel.replaceChildren();
  if (s?.kind === "block" && core.block(s.id)) {
    const block = core.block(s.id), ports = blocks.ports(block);
    panel.append(registry.el("h3", "", t("Block")));
    panel.append(field(t("Name"), block.id, (value) => {
      const result = core.renameBlock(block.id, value.trim());
      if (result.error) return describe(result.error);
      d.selection = { kind: "block", id: value.trim() };
      render("full");
      return "";
    }, { name: "id" }));
    panel.append(registry.el("p", "block-meta", t("{type} · {inputs} inputs · {outputs} outputs", { type: block.type, inputs: ports.inputs, outputs: ports.outputs })));
    const kind = blocks.CATALOG[block.type];
    for (const name of [...kind.scalars, ...kind.vectors]) {
      panel.append(field(name, blocks.formatValue(block.params[name]), (value) => {
        const checked = blocks.checkParameter(core.block(block.id), name, value, core.simulation);
        if (checked.error) return describe(checked.error);
        const result = core.setParameter(block.id, name, checked.value);
        return result.error ? describe(result.error) : "";
      }, { name }));
    }
    if (kind.state) {
      panel.append(field("initialState", String(block.initialState), (value) => {
        const checked = blocks.checkParameter(core.block(block.id), "initialState", value, core.simulation);
        if (checked.error) return describe(checked.error);
        const result = core.setParameter(block.id, "initialState", checked.value);
        return result.error ? describe(result.error) : "";
      }, { name: "initialState" }));
    }
    panel.append(registry.el("p", "block-meta", t("Sample time {value} (follows the simulation)", { value: block.sampleTime })));
    const remove = registry.el("button", "block-danger", t("Delete Block"));
    remove.id = "block-delete";
    remove.onclick = () => registry.safe(() => report(core.deleteBlock(block.id)));
    panel.append(remove);
    return;
  }
  if (s?.kind === "connection" && core.connections[s.index]) {
    const link = core.connections[s.index];
    panel.append(registry.el("h3", "", t("Connection")));
    panel.append(registry.el("p", "block-meta", t("{from} output → {to} input {port}", { from: link.from.block, to: link.to.block, port: link.to.port })));
    const remove = registry.el("button", "block-danger", t("Delete Connection"));
    remove.id = "block-delete";
    remove.onclick = () => registry.safe(() => report(core.disconnect(s.index)));
    panel.append(remove);
    return;
  }
  const sim = core.simulation;
  panel.append(registry.el("h3", "", t("Simulation")));
  const modeWrap = registry.el("label", "block-field");
  modeWrap.append(registry.el("span", "", t("Mode")));
  const mode = registry.el("select");
  mode.dataset.param = "mode";
  for (const [value, label] of [["continuous", t("Continuous")], ["discrete", t("Discrete")]]) {
    const option = registry.el("option", "", label);
    option.value = value;
    mode.append(option);
  }
  mode.value = sim.mode;
  mode.onchange = () => {
    report(core.setMode(mode.value));
    renderPanel();
  };
  modeWrap.append(mode);
  panel.append(modeWrap);
  for (const name of blocks.SIMULATION_FIELDS[sim.mode]) {
    if (name === "mode") continue;
    if (name === "solver") {
      panel.append(field(t("Solver"), sim.solver, () => "", { readOnly: true, name }));
      continue;
    }
    panel.append(field(name, String(sim[name]), (value) => {
      const checked = blocks.checkSimulationField(core.simulation, name, value);
      if (checked.error) return describe(checked.error);
      const result = core.setSimulation({ ...core.simulation, [name]: checked.value });
      return result.error ? describe(result.error) : "";
    }, { name }));
  }
  panel.append(registry.el("p", "block-hint", t("Select a block or connection to edit it. Arrow keys move a block, W starts a connection, Delete removes, ⌘Z undoes.")));
}

// ---------------------------------------------------------------------------
// Editing actions.
function addBlock(type) {
  if (!active) return;
  const d = active.diagram, r = svg.getBoundingClientRect(), v = d.view || { x: 0, y: 0, k: 1 };
  let x = (r.width / 2 - v.x) / v.k - W / 2, y = (r.height / 2 - v.y) / v.k - 26;
  // Do not stack new blocks exactly on top of each other.
  while (Object.values(d.core.layout).some((p) => Math.abs(p.x - x) < 8 && Math.abs(p.y - y) < 8)) {
    x += 24;
    y += 24;
  }
  const result = report(d.core.addBlock(type, x, y));
  if (result.ok) select({ kind: "block", id: result.result }, true);
}
function freeInputs(core) {
  const out = [];
  for (const block of core.blocks) {
    const n = blocks.ports(block).inputs;
    for (let port = 1; port <= n; port++) if (core.inputDriver(block.id, port) < 0) out.push({ id: block.id, port });
  }
  return out;
}
function markWireTarget() {
  for (const node of blockLayer.querySelectorAll(".block-port.candidate")) node.classList.remove("candidate");
  const w = active.diagram.wire;
  const target = w && w.targets[w.index];
  if (target) blockLayer.querySelector(`[data-id="${target.id}"] .block-port[data-side="in"][data-port="${target.port}"]`)?.classList.add("candidate");
  renderStatus();
}
function deleteSelection() {
  const d = active.diagram, s = d.selection;
  if (s?.kind === "block") report(d.core.deleteBlock(s.id));
  else if (s?.kind === "connection") report(d.core.disconnect(s.index));
}
function undo() {
  if (active?.diagram.core.undo()) setMessage("");
}
function redo() {
  if (active?.diagram.core.redo()) setMessage("");
}
function onKeydown(event) {
  if (!active) return;
  const typing = event.target.closest?.("input, select, textarea");
  const mod = event.metaKey || event.ctrlKey;
  if (mod && !event.altKey && event.key.toLowerCase() === "z" && !typing) {
    event.preventDefault();
    event.shiftKey ? redo() : undo();
    return;
  }
  if (mod && !event.altKey && event.key.toLowerCase() === "y" && !typing) {
    event.preventDefault();
    redo();
    return;
  }
  if (typing || !svg.contains(event.target)) return;
  const d = active.diagram;
  const node = event.target.closest?.(".block-node"), wireNode = event.target.closest?.(".block-wire");
  if (d.wire) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const n = d.wire.targets.length;
      if (n) d.wire.index = (d.wire.index + (event.key === "ArrowDown" ? 1 : n - 1)) % n;
      markWireTarget();
    } else if (event.key === "Enter") {
      event.preventDefault();
      const w = d.wire, target = w.targets[w.index];
      d.wire = null;
      if (target) report(d.core.connect(w.from, 1, target.id, target.port));
      markWireTarget();
    } else if (event.key === "Escape") {
      event.preventDefault();
      d.wire = null;
      markWireTarget();
    }
    return;
  }
  if (node) {
    const id = node.dataset.id, pos = d.core.layout[id], step = event.shiftKey ? 1 : 10;
    const dir = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[event.key];
    if (dir) {
      event.preventDefault();
      d.core.moveBlock(id, pos.x + dir[0] * step, pos.y + dir[1] * step, true);
      return;
    }
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      report(d.core.deleteBlock(id));
      return;
    }
    if (event.key.toLowerCase() === "w" && !mod && !event.altKey) {
      event.preventDefault();
      if (!blocks.ports(d.core.block(id)).outputs) {
        setMessage(describe({ code: "no-output", params: { id } }), "error");
        return;
      }
      d.wire = { from: id, targets: freeInputs(d.core), index: 0 };
      markWireTarget();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      panel.querySelector("input:not([readonly])")?.focus();
    }
    return;
  }
  if (wireNode && (event.key === "Delete" || event.key === "Backspace")) {
    event.preventDefault();
    report(d.core.disconnect(Number(wireNode.dataset.index)));
  }
}
function wirePreview(from, point) {
  const block = active.diagram.core.block(from.id), p = active.diagram.core.layout[from.id];
  const x1 = p.x + W, y1 = p.y + blocks.portY(block, "out", from.port), dx = Math.max(30, Math.abs(point.x - x1) / 2);
  ghost.setAttribute("d", `M${x1} ${y1} C${x1 + dx} ${y1} ${point.x - dx} ${point.y} ${point.x} ${point.y}`);
  ghost.removeAttribute("display");
}
// Wires of one block are redrawn while it is dragged; everything else stays.
function redrawWires() {
  drawWires();
}
function installEvents() {
  root.addEventListener("keydown", onKeydown);
  svg.addEventListener("focusin", (event) => {
    if (!active) return;
    const node = event.target.closest?.(".block-node"), wireNode = event.target.closest?.(".block-wire");
    const s = active.diagram.selection;
    if (active.diagram.wire && !(node && node.dataset.id === active.diagram.wire.from)) {
      active.diagram.wire = null;
      markWireTarget();
    }
    if (node && !(s?.kind === "block" && s.id === node.dataset.id)) select({ kind: "block", id: node.dataset.id });
    if (wireNode && !(s?.kind === "connection" && s.index === Number(wireNode.dataset.index))) select({ kind: "connection", index: Number(wireNode.dataset.index) });
  });
  svg.addEventListener("pointerdown", (event) => {
    if (!active || event.button !== 0) return;
    const d = active.diagram;
    if (d.wire) {
      d.wire = null;
      markWireTarget();
    }
    const port = event.target.closest?.(".block-port.out"), node = event.target.closest?.(".block-node"), wireNode = event.target.closest?.(".block-wire");
    svg.setPointerCapture(event.pointerId);
    if (port && node) {
      event.preventDefault();
      drag = { kind: "wire", from: { id: node.dataset.id, port: Number(port.dataset.port) } };
      return;
    }
    if (node) {
      event.preventDefault();
      const id = node.dataset.id, p = worldPoint(event), pos = d.core.layout[id];
      select({ kind: "block", id }, true);
      drag = { kind: "block", id, dx: p.x - pos.x, dy: p.y - pos.y, node, moved: false };
      return;
    }
    if (wireNode) {
      event.preventDefault();
      select({ kind: "connection", index: Number(wireNode.dataset.index) }, true);
      drag = null;
      return;
    }
    select(null);
    svg.focus({ preventScroll: true });
    drag = { kind: "pan", x: event.clientX - d.view.x, y: event.clientY - d.view.y };
  });
  svg.addEventListener("pointermove", (event) => {
    if (!drag || !active) return;
    const d = active.diagram;
    if (drag.kind === "pan") {
      d.view.x = event.clientX - drag.x;
      d.view.y = event.clientY - drag.y;
      applyView();
    } else if (drag.kind === "block") {
      // Absolute mapping from the pointer each time: no accumulated rounding.
      const p = worldPoint(event);
      drag.moved = true;
      d.core.moveBlock(drag.id, p.x - drag.dx, p.y - drag.dy, false);
      const pos = d.core.layout[drag.id];
      drag.node.setAttribute("transform", `translate(${pos.x} ${pos.y})`);
      redrawWires();
    } else if (drag.kind === "wire") {
      wirePreview(drag.from, worldPoint(event));
    }
  });
  const finish = (event) => {
    const current = drag;
    drag = null;
    ghost.setAttribute("display", "none");
    if (!current || !active) return;
    const d = active.diagram;
    if (current.kind === "block" && current.moved) {
      const pos = d.core.layout[current.id];
      d.core.moveBlock(current.id, pos.x, pos.y, true);
    }
    if (current.kind === "wire" && event.type === "pointerup") {
      const hit = document.elementFromPoint(event.clientX, event.clientY);
      const port = hit?.closest?.(".block-port.in"), node = hit?.closest?.(".block-node");
      if (node && svg.contains(node)) {
        let target = port ? Number(port.dataset.port) : 0;
        // Dropped on a block body: its first free input.
        if (!target) {
          const n = blocks.ports(d.core.block(node.dataset.id)).inputs;
          for (let k = 1; k <= n && !target; k++) if (d.core.inputDriver(node.dataset.id, k) < 0) target = k;
        }
        if (target) report(d.core.connect(current.from.id, current.from.port, node.dataset.id, target));
        else setMessage(t("{id} has no free input.", { id: node.dataset.id }), "error");
      }
    }
  };
  svg.addEventListener("pointerup", finish);
  svg.addEventListener("pointercancel", finish);
  svg.addEventListener("wheel", (event) => {
    if (!active?.diagram.view) return;
    event.preventDefault();
    if (event.ctrlKey || event.metaKey) {
      const r = svg.getBoundingClientRect();
      zoomAt(Math.exp(-event.deltaY * 0.0025), event.clientX - r.left, event.clientY - r.top);
    } else {
      active.diagram.view.x -= event.deltaX;
      active.diagram.view.y -= event.deltaY;
      applyView();
    }
  }, { passive: false });
  svg.setAttribute("tabindex", "-1");
}

// ---------------------------------------------------------------------------
// Run: save, then the ordinary execute path with visible generated code.
async function runDiagram(tab = shared.active) {
  registry.requireIdle();
  if (!isDiagramTab(tab)) return;
  if (tab.dirty || !tab.hash) {
    if (!await registry.saveActive()) return;
  }
  const name = tab.path.split("/").pop(), model = tab.diagram.core.model();
  const code = blocks.runCode(model, name, { time: t("Time (s)") });
  tab.diagram.errorTarget = null;
  setMessage(t("Running the diagram…"));
  render("full");
  await registry.execute(code, "code", "", t("Run block diagram {name}", { name }), false, code, (accepted) => {
    runs.set(accepted.job, { tab, model });
    const entry = shared.activeOutput;
    if (entry?.wrap) {
      const details = registry.el("details", "console-generated");
      details.append(registry.el("summary", "", t("Generated code")), registry.el("pre", "console-source", code));
      entry.wrap.querySelector(".console-command")?.after(details);
    }
  });
}
// Called by poll when a job reaches its terminal state.
function blockDiagramJobCompleted(state) {
  const run = runs.get(state.job);
  if (!run) return;
  runs.delete(state.job);
  const tab = run.tab;
  if (!shared.tabs.includes(tab) || !isDiagramTab(tab)) return;
  const d = tab.diagram;
  if (state.error) {
    const text = blocks.firstErrorLine(state.error);
    d.message = { kind: "error", text: t("Run failed: {message}", { message: text }) };
    const target = blocks.errorTarget(text, run.model);
    if (target?.kind === "block" && d.core.block(target.id)) {
      d.errorTarget = target;
      d.selection = { kind: "block", id: target.id };
    } else if (target?.kind === "connection" && target.index < d.core.connections.length) {
      d.errorTarget = target;
      d.selection = { kind: "connection", index: target.index };
    }
    if (tab === active) {
      render("full");
      if (d.selection?.kind === "block") reveal(d.selection.id);
    }
    return;
  }
  d.message = { kind: "info", text: t("Simulation finished.") };
  if (tab === active) renderStatus();
}

onLanguageChange(() => {
  labels();
  if (active) render("full");
  if (root) renderDiagramCaption();
});

registry.setupBlockEditor = () => ensureRoot();
Object.assign(registry, { isDiagramTab, prepareDiagramTab, showDiagramTab, renderDiagramCaption, diagramFooter, runDiagram, blockDiagramJobCompleted, diagramUndo: undo, diagramRedo: redo, openAsText, openAsDiagram });
