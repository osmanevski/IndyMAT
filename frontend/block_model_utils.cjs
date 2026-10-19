"use strict";
// Pure model core of the IndyMAT block diagram editor: file format, structural
// validation, editing with undo/redo, wire geometry and the generated Run code.
// No DOM, no storage, no network. The model part is schema 1 of
// octave/block-models.md unchanged; im_compile stays the semantic authority.

const FILE_SUFFIX = ".imbd.json";
const ID_PATTERN = "[A-Za-z][A-Za-z0-9_]{0,62}";
const ID_RE = /^[A-Za-z][A-Za-z0-9_]{0,62}$/;
const COORDINATE_LIMIT = 1e6;
const MAX_INPUTS = 256;
const HISTORY_LIMIT = 300;

// Exact parameter names, defaults and port counts per type (block-models.md table).
const CATALOG = {
  Constant: { scalars: ["value"], vectors: [], defaults: { value: 1 }, inputs: 0, outputs: 1, mode: "both" },
  Step: { scalars: ["time", "before", "after"], vectors: [], defaults: { time: 1, before: 0, after: 1 }, inputs: 0, outputs: 1, mode: "both" },
  Sine: { scalars: ["amplitude", "omega", "phase", "bias"], vectors: [], defaults: { amplitude: 1, omega: 1, phase: 0, bias: 0 }, inputs: 0, outputs: 1, mode: "both" },
  Gain: { scalars: ["gain"], vectors: [], defaults: { gain: 1 }, inputs: 1, outputs: 1, mode: "both" },
  Sum: { scalars: [], vectors: ["signs"], defaults: { signs: [1, -1] }, inputs: null, outputs: 1, mode: "both" },
  Saturation: { scalars: ["lower", "upper"], vectors: [], defaults: { lower: -1, upper: 1 }, inputs: 1, outputs: 1, mode: "both" },
  Integrator: { scalars: [], vectors: [], defaults: {}, inputs: 1, outputs: 1, mode: "continuous", state: true },
  TransferFcn: { scalars: [], vectors: ["num", "den"], defaults: { num: 1, den: [1, 1] }, inputs: 1, outputs: 1, mode: "continuous" },
  Mux: { scalars: ["inputs"], vectors: [], defaults: { inputs: 2 }, inputs: null, outputs: 1, mode: "both" },
  Scope: { scalars: [], vectors: [], defaults: {}, inputs: 1, outputs: 0, mode: "both" },
  UnitDelay: { scalars: [], vectors: [], defaults: {}, inputs: 1, outputs: 1, mode: "discrete", state: true },
  SampledInput: { scalars: [], vectors: ["values"], defaults: { values: [0] }, inputs: 0, outputs: 1, mode: "discrete" },
  ClampingGate: { scalars: ["lower", "upper"], vectors: [], defaults: { lower: -1, upper: 1 }, inputs: 2, outputs: 1, mode: "discrete" }
};
const BLOCK_TYPES = Object.keys(CATALOG);
const SIMULATION_FIELDS = {
  continuous: ["mode", "t0", "tf", "solver", "outputDt", "relTol", "absTol", "maxStep"],
  discrete: ["mode", "t0", "tf", "solver", "Ts"]
};
const SOLVERS = { continuous: "ode45", discrete: "discrete" };
const DEFAULT_SIMULATION = {
  continuous: { mode: "continuous", t0: 0, tf: 10, solver: "ode45", outputDt: 0.01, relTol: 1e-6, absTol: 1e-8, maxStep: 0.1 },
  discrete: { mode: "discrete", t0: 0, tf: 10, solver: "discrete", Ts: 0.1 }
};

class ModelError extends Error {
  constructor(code, params = {}) {
    super(code + " " + JSON.stringify(params));
    this.code = code;
    this.params = params;
  }
}
const fail = (code, params) => { throw new ModelError(code, params); };
const copy = (value) => JSON.parse(JSON.stringify(value));
const isPlainObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const isNumber = (value) => typeof value === "number" && Number.isFinite(value);
const vectorOf = (value) => Array.isArray(value) ? value : [value];

// ---------------------------------------------------------------------------
// Strict JSON: JSON.parse silently keeps the last duplicate key; schema 1 rejects
// duplicates (im_compile check_json_keys), so the editor parses on its own.
function parseStrictJSON(text) {
  if (typeof text !== "string") fail("json", { position: 0 });
  let i = 0;
  const ws = () => { while (i < text.length && " \t\n\r".includes(text[i])) i++; };
  const bad = () => fail("json", { position: i + 1 });
  function string() {
    if (text[i] !== "\"") bad();
    i++;
    let out = "";
    while (true) {
      if (i >= text.length) bad();
      const c = text[i];
      if (c === "\"") { i++; return out; }
      if (c.charCodeAt(0) < 0x20) bad();
      if (c === "\\") {
        const e = text[i + 1];
        const simple = { "\"": "\"", "\\": "\\", "/": "/", b: "\b", f: "\f", n: "\n", r: "\r", t: "\t" };
        if (Object.hasOwn(simple, e)) { out += simple[e]; i += 2; continue; }
        if (e === "u" && /^[0-9a-fA-F]{4}$/.test(text.slice(i + 2, i + 6))) { out += String.fromCharCode(parseInt(text.slice(i + 2, i + 6), 16)); i += 6; continue; }
        bad();
      }
      out += c;
      i++;
    }
  }
  function value(depth) {
    if (depth > 64) bad();
    ws();
    const c = text[i];
    if (c === "{") {
      i++;
      const obj = {}, seen = new Set();
      ws();
      if (text[i] === "}") { i++; return obj; }
      while (true) {
        ws();
        const key = string();
        if (seen.has(key)) fail("duplicate-key", { key });
        seen.add(key);
        ws();
        if (text[i] !== ":") bad();
        i++;
        Object.defineProperty(obj, key, { value: value(depth + 1), enumerable: true, writable: true, configurable: true });
        ws();
        if (text[i] === ",") { i++; continue; }
        if (text[i] === "}") { i++; return obj; }
        bad();
      }
    }
    if (c === "[") {
      i++;
      const arr = [];
      ws();
      if (text[i] === "]") { i++; return arr; }
      while (true) {
        arr.push(value(depth + 1));
        ws();
        if (text[i] === ",") { i++; continue; }
        if (text[i] === "]") { i++; return arr; }
        bad();
      }
    }
    if (c === "\"") return string();
    for (const [word, result] of [["true", true], ["false", false], ["null", null]]) {
      if (text.startsWith(word, i)) { i += word.length; return result; }
    }
    const m = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/.exec(text.slice(i, i + 400));
    if (!m) bad();
    i += m[0].length;
    return Number(m[0]);
  }
  const result = value(0);
  ws();
  if (i !== text.length) bad();
  return result;
}

// ---------------------------------------------------------------------------
// Structural validation. Everything the editor needs to draw and edit is checked
// here; value rules (signs of +/-1, strict properness, grid lengths, drivers,
// algebraic loops, time grids) remain im_compile's and are reported at Run.
function exactFields(obj, required, optional, context) {
  if (!isPlainObject(obj)) fail("object", { context });
  for (const key of required) if (!Object.hasOwn(obj, key)) fail("missing-field", { context, field: key });
  for (const key of Object.keys(obj)) if (!required.includes(key) && !optional.includes(key)) fail("unknown-field", { context, field: key });
}
function inputCount(block) {
  const kind = CATALOG[block.type];
  if (block.type === "Sum") return vectorOf(block.params.signs).length;
  if (block.type === "Mux") return block.params.inputs;
  return kind.inputs;
}
function ports(block) {
  return { inputs: inputCount(block), outputs: CATALOG[block.type].outputs };
}
function checkBlock(block, index, ids) {
  const context = `model.blocks[${index}]`;
  exactFields(block, ["id", "type", "params", "sampleTime"], ["initialState"], context);
  if (typeof block.id !== "string" || !ID_RE.test(block.id)) fail("block-id", { index: index + 1, pattern: ID_PATTERN });
  if (ids.has(block.id)) fail("duplicate-id", { id: block.id });
  ids.add(block.id);
  if (typeof block.type !== "string" || !Object.hasOwn(CATALOG, block.type)) fail("type", { id: block.id, type: String(block.type) });
  const kind = CATALOG[block.type];
  exactFields(block.params, [...kind.scalars, ...kind.vectors], [], `${context}.params`);
  for (const name of kind.scalars) if (!isNumber(block.params[name])) fail("number", { context: `${context}.params.${name}` });
  for (const name of kind.vectors) {
    const v = block.params[name];
    if (!(isNumber(v) || Array.isArray(v) && v.length > 0 && v.every(isNumber))) fail("vector", { context: `${context}.params.${name}` });
  }
  if (block.type === "Mux" && !(Number.isInteger(block.params.inputs) && block.params.inputs >= 1 && block.params.inputs <= MAX_INPUTS)) fail("mux-inputs", { id: block.id, limit: MAX_INPUTS });
  if (block.type === "Sum" && vectorOf(block.params.signs).length > MAX_INPUTS) fail("too-many-inputs", { id: block.id, limit: MAX_INPUTS });
  if (!isNumber(block.sampleTime)) fail("number", { context: `${context}.sampleTime` });
  if (kind.state && !Object.hasOwn(block, "initialState")) fail("state-missing", { id: block.id });
  if (!kind.state && Object.hasOwn(block, "initialState")) fail("state-forbidden", { id: block.id });
  if (kind.state && !isNumber(block.initialState)) fail("number", { context: `${context}.initialState` });
}
function checkSimulation(sim) {
  if (!isPlainObject(sim)) fail("object", { context: "model.simulation" });
  if (!Object.hasOwn(SIMULATION_FIELDS, sim.mode)) fail("mode", {});
  exactFields(sim, SIMULATION_FIELDS[sim.mode], [], "model.simulation");
  for (const key of SIMULATION_FIELDS[sim.mode]) {
    if (key === "mode") continue;
    if (key === "solver") { if (typeof sim.solver !== "string") fail("text", { context: "model.simulation.solver" }); continue; }
    if (!isNumber(sim[key])) fail("number", { context: `model.simulation.${key}` });
  }
}
function validateFile(file) {
  exactFields(file, ["imbVersion", "model", "layout"], [], "file");
  if (file.imbVersion !== 1) fail("imb-version", {});
  const model = file.model;
  exactFields(model, ["schemaVersion", "blocks", "connections", "simulation"], [], "model");
  if (model.schemaVersion !== 1) fail("schema-version", {});
  if (!Array.isArray(model.blocks)) fail("array", { context: "model.blocks" });
  if (!Array.isArray(model.connections)) fail("array", { context: "model.connections" });
  checkSimulation(model.simulation);
  const ids = new Set();
  model.blocks.forEach((block, index) => checkBlock(block, index, ids));
  const byId = new Map(model.blocks.map((block) => [block.id, block]));
  model.connections.forEach((link, index) => {
    const context = `model.connections[${index}]`;
    exactFields(link, ["from", "to"], [], context);
    for (const side of ["from", "to"]) {
      exactFields(link[side], ["block", "port"], [], `${context}.${side}`);
      const id = link[side].block, port = link[side].port;
      if (typeof id !== "string" || !byId.has(id)) fail("endpoint-block", { index: index + 1, id: String(id) });
      const available = side === "from" ? ports(byId.get(id)).outputs : ports(byId.get(id)).inputs;
      if (!Number.isInteger(port) || port < 1 || port > available) fail("endpoint-port", { index: index + 1, id });
    }
  });
  exactFields(file.layout, ["blocks"], [], "layout");
  const positions = file.layout.blocks;
  if (!isPlainObject(positions)) fail("object", { context: "layout.blocks" });
  for (const id of Object.keys(positions)) if (!byId.has(id)) fail("layout-extra", { id });
  for (const block of model.blocks) {
    if (!Object.hasOwn(positions, block.id)) fail("layout-missing", { id: block.id });
    const p = positions[block.id], context = `layout.blocks.${block.id}`;
    exactFields(p, ["x", "y"], [], context);
    for (const axis of ["x", "y"]) if (!isNumber(p[axis]) || Math.abs(p[axis]) > COORDINATE_LIMIT) fail("coordinate", { context: `${context}.${axis}`, limit: COORDINATE_LIMIT });
  }
  return file;
}
// Returns {ok:true, file} or {ok:false, error:{code, params}}.
function readDiagram(text) {
  try {
    return { ok: true, file: validateFile(parseStrictJSON(text)) };
  } catch (error) {
    if (error instanceof ModelError) return { ok: false, error: { code: error.code, params: error.params } };
    throw error;
  }
}

// ---------------------------------------------------------------------------
// Canonical forms and serialization.
function canonicalBlock(block) {
  const kind = CATALOG[block.type], params = {};
  for (const name of [...kind.scalars, ...kind.vectors]) params[name] = copy(block.params[name]);
  const out = { id: block.id, type: block.type, params, sampleTime: block.sampleTime };
  if (kind.state) out.initialState = block.initialState;
  return out;
}
function canonicalSimulation(sim) {
  const out = {};
  for (const key of SIMULATION_FIELDS[sim.mode]) out[key] = sim[key];
  return out;
}
function canonicalConnection(link) {
  return { from: { block: link.from.block, port: link.from.port }, to: { block: link.to.block, port: link.to.port } };
}
function inline(value) {
  if (Array.isArray(value)) return "[" + value.map(inline).join(", ") + "]";
  if (isPlainObject(value)) return "{" + Object.keys(value).map((key) => JSON.stringify(key) + ": " + inline(value[key])).join(", ") + "}";
  return JSON.stringify(value);
}
function list(items, indent) {
  if (!items.length) return "[]";
  return "[\n" + items.map((item) => indent + "  " + inline(item)).join(",\n") + "\n" + indent + "]";
}
// One block, connection or position per line: readable as text and diff-friendly.
function serializeFile(file) {
  const model = file.model, positions = file.layout.blocks;
  const layoutLines = Object.keys(positions).map((id) => "      " + JSON.stringify(id) + ": " + inline(positions[id]));
  return "{\n" +
    "  \"imbVersion\": 1,\n" +
    "  \"model\": {\n" +
    "    \"schemaVersion\": 1,\n" +
    "    \"blocks\": " + list(model.blocks, "    ") + ",\n" +
    "    \"connections\": " + list(model.connections, "    ") + ",\n" +
    "    \"simulation\": " + inline(model.simulation) + "\n" +
    "  },\n" +
    "  \"layout\": {\n" +
    "    \"blocks\": " + (layoutLines.length ? "{\n" + layoutLines.join(",\n") + "\n    }" : "{}") + "\n" +
    "  }\n" +
    "}\n";
}
// Compact single-line JSON, ASCII only: every non-ASCII or control character is a
// \uXXXX escape, so the Octave text literal carries bytes that cannot be altered.
function compactModelJSON(model) {
  return JSON.stringify(model).replace(/[\u0000-\u001f\u007f-￿]/g, (c) => "\\u" + c.charCodeAt(0).toString(16).padStart(4, "0"));
}

// ---------------------------------------------------------------------------
// Numbers typed in the property panel: plain decimal literals only. Nothing is
// evaluated (no pi, Inf, NaN, expressions or variables).
const NUMBER_RE = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
function parseNumberList(text) {
  if (typeof text !== "string") return null;
  let body = text.trim();
  if (body.startsWith("[") && body.endsWith("]")) body = body.slice(1, -1).trim();
  if (!body) return null;
  const parts = body.split(/[\s,;]+/).filter(Boolean);
  if (!parts.length || parts.some((part) => !NUMBER_RE.test(part))) return null;
  const values = parts.map(Number);
  return values.every(Number.isFinite) ? values : null;
}
function formatValue(value) {
  return Array.isArray(value) ? value.map(String).join(", ") : String(value);
}
function gridLength(sim) {
  const step = sim.mode === "discrete" ? sim.Ts : sim.outputDt;
  if (![sim.t0, sim.tf, step].every(isNumber) || step <= 0 || sim.tf <= sim.t0) return null;
  const ratio = (sim.tf - sim.t0) / step, count = Math.round(ratio);
  if (count < 1 || count + 1 > 100001 || Math.abs(ratio - count) > 32 * Number.EPSILON * Math.max(1, Math.abs(ratio))) return null;
  return count + 1;
}
// Panel check for one parameter. Returns {value} or {error:{code, params}}.
function checkParameter(block, name, text, sim) {
  const values = parseNumberList(text);
  const kind = CATALOG[block.type];
  const err = (code, params = {}) => ({ error: { code, params: { name, ...params } } });
  if (!values) return err("numbers-only");
  if (name === "initialState") return values.length === 1 ? { value: values[0] } : err("scalar");
  if (kind.scalars.includes(name)) {
    if (values.length !== 1) return err("scalar");
    const v = values[0];
    if (name === "inputs" && !(Number.isInteger(v) && v >= 1 && v <= MAX_INPUTS)) return err("integer-range", { min: 1, max: MAX_INPUTS });
    if (name === "lower" && v > block.params.upper) return err("lower-upper");
    if (name === "upper" && v < block.params.lower) return err("lower-upper");
    return { value: v };
  }
  if (!kind.vectors.includes(name)) return err("unknown-parameter");
  if (name === "signs") {
    if (values.length > MAX_INPUTS) return err("too-many", { max: MAX_INPUTS });
    if (values.some((v) => v !== 1 && v !== -1)) return err("signs");
    return { value: values };
  }
  if (name === "num" || name === "den") {
    const num = name === "num" ? values : vectorOf(block.params.num), den = name === "den" ? values : vectorOf(block.params.den);
    if (den.length < 2 || den.length > 65) return err("degree");
    if (den[0] === 0 || num.length >= den.length || num[0] === 0 && num.length > 1) return err("proper");
    return { value: values.length === 1 ? values[0] : values };
  }
  if (name === "values") {
    const n = gridLength(sim);
    if (n !== null && values.length !== n) return err("grid-values", { count: n });
    return { value: values.length === 1 ? values[0] : values };
  }
  return { value: values };
}
const SIM_POSITIVE = new Set(["outputDt", "relTol", "absTol", "maxStep", "Ts"]);
function checkSimulationField(sim, name, text) {
  const values = parseNumberList(text);
  const err = (code, params = {}) => ({ error: { code, params: { name, ...params } } });
  if (!values) return err("numbers-only");
  if (values.length !== 1) return err("scalar");
  const v = values[0];
  if (SIM_POSITIVE.has(name) && v <= 0) return err("positive");
  if (name === "t0" && v >= sim.tf || name === "tf" && v <= sim.t0) return err("t0-tf");
  return { value: v };
}

// ---------------------------------------------------------------------------
// Editing core. One history entry per user action; a drag commits once at the end.
class DiagramCore {
  constructor(file) {
    this.load(file);
    this.history = [this.snapshot()];
    this.future = [];
    this.listeners = [];
  }
  load(file) {
    this.blocks = file.model.blocks.map(canonicalBlock);
    this.connections = file.model.connections.map(canonicalConnection);
    this.simulation = canonicalSimulation(file.model.simulation);
    this.layout = {};
    for (const block of this.blocks) this.layout[block.id] = { x: file.layout.blocks[block.id].x, y: file.layout.blocks[block.id].y };
  }
  snapshot() {
    return JSON.stringify(this.fileObject());
  }
  restore(text) {
    this.load(JSON.parse(text));
  }
  emit(kind) {
    for (const fn of this.listeners) fn(kind);
  }
  // Runs one edit; an error restores the previous state and records nothing.
  apply(edit, kind = "structure") {
    const before = this.history[this.history.length - 1];
    let result;
    try {
      result = edit();
    } catch (error) {
      this.restore(before);
      if (error instanceof ModelError) return { error: { code: error.code, params: error.params } };
      throw error;
    }
    this.commit();
    this.emit(kind);
    return { ok: true, result };
  }
  commit() {
    const now = this.snapshot();
    if (now !== this.history[this.history.length - 1]) {
      this.history.push(now);
      if (this.history.length > HISTORY_LIMIT) this.history.shift();
      this.future = [];
      return true;
    }
    return false;
  }
  canUndo() { return this.history.length > 1; }
  canRedo() { return this.future.length > 0; }
  undo() {
    if (!this.canUndo()) return false;
    this.future.push(this.history.pop());
    this.restore(this.history[this.history.length - 1]);
    this.emit("structure");
    return true;
  }
  redo() {
    if (!this.canRedo()) return false;
    const next = this.future.pop();
    this.history.push(next);
    this.restore(next);
    this.emit("structure");
    return true;
  }
  block(id) { return this.blocks.find((block) => block.id === id); }
  ports(id) { return ports(this.block(id)); }
  sampleTime() { return this.simulation.mode === "discrete" ? this.simulation.Ts : 0; }
  freshId(type) {
    let n = 1;
    const base = type.charAt(0).toLowerCase() + type.slice(1);
    while (this.block(base + n)) n++;
    return base + n;
  }
  addBlock(type, x, y) {
    return this.apply(() => {
      if (!Object.hasOwn(CATALOG, type)) fail("type", { id: "", type });
      const kind = CATALOG[type], id = this.freshId(type);
      const block = { id, type, params: copy(kind.defaults), sampleTime: this.sampleTime() };
      if (type === "SampledInput") {
        const n = gridLength(this.simulation);
        block.params.values = n && this.simulation.mode === "discrete" ? new Array(n).fill(0) : 0;
      }
      if (kind.state) block.initialState = 0;
      this.blocks.push(canonicalBlock(block));
      this.layout[id] = { x: clampCoordinate(x), y: clampCoordinate(y) };
      return id;
    });
  }
  // Live position during a drag; commit=true records one history entry.
  moveBlock(id, x, y, commit) {
    if (!this.block(id)) return false;
    this.layout[id] = { x: clampCoordinate(x), y: clampCoordinate(y) };
    if (commit && this.commit()) this.emit("move");
    return true;
  }
  deleteBlock(id) {
    return this.apply(() => {
      if (!this.block(id)) fail("no-block", { id });
      this.blocks = this.blocks.filter((block) => block.id !== id);
      this.connections = this.connections.filter((link) => link.from.block !== id && link.to.block !== id);
      delete this.layout[id];
    });
  }
  inputDriver(id, port) {
    return this.connections.findIndex((link) => link.to.block === id && link.to.port === port);
  }
  connect(fromId, fromPort, toId, toPort) {
    return this.apply(() => {
      const from = this.block(fromId), to = this.block(toId);
      if (!from || !to) fail("no-block", { id: !from ? fromId : toId });
      if (!(fromPort >= 1 && fromPort <= ports(from).outputs)) fail("no-output", { id: fromId });
      if (!(toPort >= 1 && toPort <= ports(to).inputs)) fail("no-input", { id: toId, port: toPort });
      if (this.inputDriver(toId, toPort) >= 0) fail("input-taken", { id: toId, port: toPort });
      this.connections.push({ from: { block: fromId, port: fromPort }, to: { block: toId, port: toPort } });
      return this.connections.length - 1;
    });
  }
  disconnect(index) {
    return this.apply(() => {
      if (!(index >= 0 && index < this.connections.length)) fail("no-connection", { index });
      this.connections.splice(index, 1);
    });
  }
  // Changing Sum signs or Mux inputs can remove input ports; their wires go too.
  setParameter(id, name, value) {
    return this.apply(() => {
      const block = this.block(id);
      if (!block) fail("no-block", { id });
      if (name === "initialState") block.initialState = value;
      else block.params[name] = value;
      const count = ports(block).inputs;
      this.connections = this.connections.filter((link) => !(link.to.block === id && link.to.port > count));
    }, "parameter");
  }
  renameBlock(id, next) {
    return this.apply(() => {
      if (!this.block(id)) fail("no-block", { id });
      if (typeof next !== "string" || !ID_RE.test(next)) fail("block-name", { pattern: ID_PATTERN });
      if (next === id) return id;
      if (this.block(next)) fail("duplicate-id", { id: next });
      this.block(id).id = next;
      for (const link of this.connections) {
        if (link.from.block === id) link.from.block = next;
        if (link.to.block === id) link.to.block = next;
      }
      const positions = {};
      for (const key of Object.keys(this.layout)) positions[key === id ? next : key] = this.layout[key];
      this.layout = positions;
      return next;
    });
  }
  // sampleTime follows the simulation: 0 in continuous mode, Ts in discrete mode.
  setSimulation(next) {
    return this.apply(() => {
      checkSimulation(next);
      this.simulation = canonicalSimulation(next);
      for (const block of this.blocks) block.sampleTime = this.sampleTime();
    }, "simulation");
  }
  setMode(mode) {
    if (mode === this.simulation.mode) return { ok: true };
    const next = copy(DEFAULT_SIMULATION[mode]);
    next.t0 = this.simulation.t0;
    next.tf = this.simulation.tf;
    const remembered = this.otherMode && this.otherMode.mode === mode ? this.otherMode : null;
    if (remembered) for (const key of SIMULATION_FIELDS[mode]) if (!["mode", "t0", "tf"].includes(key)) next[key] = remembered[key];
    const previous = copy(this.simulation);
    const result = this.setSimulation(next);
    if (result.ok) this.otherMode = previous;
    return result;
  }
  model() {
    return {
      schemaVersion: 1,
      blocks: this.blocks.map(canonicalBlock),
      connections: this.connections.map(canonicalConnection),
      simulation: canonicalSimulation(this.simulation)
    };
  }
  fileObject() {
    const blocks = {};
    for (const block of this.blocks) blocks[block.id] = { x: this.layout[block.id].x, y: this.layout[block.id].y };
    return { imbVersion: 1, model: this.model(), layout: { blocks } };
  }
  text() {
    return serializeFile(this.fileObject());
  }
}
function clampCoordinate(v) {
  const n = Math.round(Number(v) || 0);
  return Math.max(-COORDINATE_LIMIT, Math.min(COORDINATE_LIMIT, n));
}

// ---------------------------------------------------------------------------
// Geometry. Blocks are drawn at layout (x, y) with a fixed width; inputs on the
// left edge, the output on the right edge.
const BLOCK_WIDTH = 124;
function blockHeight(block) {
  const p = ports(block);
  return Math.max(52, 20 * Math.max(p.inputs, p.outputs, 1) + 14);
}
function portY(block, side, port) {
  const p = ports(block), n = side === "in" ? p.inputs : p.outputs;
  return blockHeight(block) * port / (n + 1);
}
function rounded(points, radius = 8) {
  let d = `M${points[0][0]} ${points[0][1]}`;
  for (let k = 1; k < points.length - 1; k++) {
    const [px, py] = points[k - 1], [x, y] = points[k], [nx, ny] = points[k + 1];
    const a = Math.min(radius, Math.hypot(x - px, y - py) / 2), b = Math.min(radius, Math.hypot(nx - x, ny - y) / 2);
    const ux = Math.sign(x - px), uy = Math.sign(y - py), vx = Math.sign(nx - x), vy = Math.sign(ny - y);
    d += ` L${x - ux * a} ${y - uy * a} Q${x} ${y} ${x + vx * b} ${y + vy * b}`;
  }
  const last = points[points.length - 1];
  return d + ` L${last[0]} ${last[1]}`;
}
// A wire whose target lies left of its source (feedback) is routed below every
// block in the horizontal span it crosses, so it never runs behind a block.
// lane separates several feedback wires sharing the same corridor.
function wireGeometry(core, link, lane = 0) {
  const a = core.block(link.from.block), b = core.block(link.to.block);
  const pa = core.layout[a.id], pb = core.layout[b.id];
  const x1 = pa.x + BLOCK_WIDTH, y1 = pa.y + portY(a, "out", link.from.port);
  const x2 = pb.x, y2 = pb.y + portY(b, "in", link.to.port);
  if (x2 >= x1 + 24) {
    const d = Math.max(30, (x2 - x1) / 2);
    return { feedback: false, d: `M${x1} ${y1} C${x1 + d} ${y1} ${x2 - d} ${y2} ${x2} ${y2}` };
  }
  const left = x2 - 18 - lane * 6, right = x1 + 18 + lane * 6;
  let bottom = Math.max(pa.y + blockHeight(a), pb.y + blockHeight(b));
  for (const block of core.blocks) {
    const p = core.layout[block.id];
    if (p.x <= right && p.x + BLOCK_WIDTH >= left) bottom = Math.max(bottom, p.y + blockHeight(block));
  }
  const y = bottom + 22 + lane * 10;
  return { feedback: true, routeY: y, d: rounded([[x1, y1], [right, y1], [right, y], [left, y], [left, y2], [x2, y2]]) };
}
function wireLanes(core) {
  const lanes = new Map();
  let n = 0;
  core.connections.forEach((link, index) => {
    const a = core.layout[link.from.block], b = core.layout[link.to.block];
    if (b.x < a.x + BLOCK_WIDTH + 24) lanes.set(index, n++ % 6);
  });
  return lanes;
}
function bounds(core) {
  if (!core.blocks.length) return { x: 0, y: 0, width: 400, height: 240 };
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const block of core.blocks) {
    const p = core.layout[block.id];
    x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y);
    x1 = Math.max(x1, p.x + BLOCK_WIDTH); y1 = Math.max(y1, p.y + blockHeight(block));
  }
  // Room for feedback corridors below the lowest block.
  return { x: x0 - 30, y: y0 - 30, width: x1 - x0 + 60, height: y1 - y0 + 110 };
}

// ---------------------------------------------------------------------------
// Run: ordinary visible Octave code for the existing execute path. Calls use
// function-handle literals so a user variable named like a function cannot
// capture them. The only base variable written is <stem>_result.
function octaveString(text) {
  return "'" + String(text).replace(/'/g, "''") + "'";
}
function diagramStem(fileName) {
  const name = String(fileName).split("/").pop();
  return name.endsWith(FILE_SUFFIX) ? name.slice(0, -FILE_SUFFIX.length) : name;
}
function resultName(fileName) {
  const stem = diagramStem(fileName);
  return /^[A-Za-z][A-Za-z0-9_]{0,55}$/.test(stem) && !stem.startsWith("__mf_") ? stem + "_result" : "diagram_result";
}
function regexpLiteral(text) {
  return String(text).replace(/[\\^$.|?*+()[\]{}]/g, "\\$&");
}
function scopeLegend(model, scope) {
  const driver = model.connections.find((link) => link.to.block === scope.id && link.to.port === 1);
  if (!driver) return null;
  const source = model.blocks.find((block) => block.id === driver.from.block);
  if (!source || source.type !== "Mux") return null;
  const names = [];
  for (let port = 1; port <= source.params.inputs; port++) {
    const link = model.connections.find((item) => item.to.block === source.id && item.to.port === port);
    if (!link) return null;
    names.push(link.from.block);
  }
  return names;
}
function runCode(model, fileName, labels = {}) {
  const name = String(fileName).split("/").pop(), result = resultName(fileName);
  const lines = [
    "% " + (labels.comment || "IndyMAT block diagram") + ": " + name.replace(/[\r\n]/g, " "),
    `${result} = (@im_simulate)(${octaveString(compactModelJSON(model))});`
  ];
  const plotter = model.simulation.mode === "discrete" ? "stairs" : "plot";
  for (const scope of model.blocks.filter((block) => block.type === "Scope")) {
    const figure = name + ": " + scope.id;
    lines.push(
      `(@close)((@findobj)(0, 'type', 'figure', '-regexp', 'name', ${octaveString("^" + regexpLiteral(figure) + "$")}));`,
      `(@figure)('Name', ${octaveString(figure)});`,
      `(@${plotter})(${result}.t, ${result}.signals.${scope.id});`,
      "(@grid)('on');",
      `(@title)(${octaveString(scope.id)});`,
      `(@xlabel)(${octaveString(labels.time || "Time (s)")});`
    );
    const legend = scopeLegend(model, scope);
    if (legend) lines.push(`(@legend)({${legend.map(octaveString).join(", ")}});`);
  }
  return lines.join("\n") + "\n";
}

// im_compile messages name the block (Block x, Mux x, x.outN -> y.inM) or the
// connection (Connection n). Returns the first target that exists in the model.
function errorTarget(message, model) {
  const text = String(message || "");
  const ids = new Set(model.blocks.map((block) => block.id));
  const connection = /Connection (\d+)\b/.exec(text);
  if (connection && /^\s*(?:error:\s*)?Connection \d+/.test(text)) {
    const index = Number(connection[1]) - 1;
    if (index >= 0 && index < model.connections.length) return { kind: "connection", index };
  }
  const patterns = [/\bBlock ([A-Za-z][A-Za-z0-9_]*)\b/g, /\bMux ([A-Za-z][A-Za-z0-9_]*)\b/g, /\bunknown block ([A-Za-z][A-Za-z0-9_]*)\b/g, /\bDuplicate block id: ([A-Za-z][A-Za-z0-9_]*)/g, /\b([A-Za-z][A-Za-z0-9_]*)\.(?:out|in)\d+\b/g];
  for (const re of patterns) {
    for (const match of text.matchAll(re)) if (ids.has(match[1])) return { kind: "block", id: match[1] };
  }
  const blockIndex = /\b[Bb]lock (\d+)\b/.exec(text);
  if (blockIndex) {
    const block = model.blocks[Number(blockIndex[1]) - 1];
    if (block) return { kind: "block", id: block.id };
  }
  if (connection) {
    const index = Number(connection[1]) - 1;
    if (index >= 0 && index < model.connections.length) return { kind: "connection", index };
  }
  return null;
}
function firstErrorLine(text) {
  const lines = String(text || "").split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const line = lines.find((item) => item.startsWith("error:")) || lines[0] || "";
  return line.replace(/^error:\s*/, "");
}

function isDiagramPath(path) {
  return typeof path === "string" && path.toLowerCase().endsWith(FILE_SUFFIX);
}

module.exports = {
  FILE_SUFFIX, ID_PATTERN, CATALOG, BLOCK_TYPES, SIMULATION_FIELDS, SOLVERS, DEFAULT_SIMULATION, BLOCK_WIDTH, MAX_INPUTS,
  ModelError, parseStrictJSON, validateFile, readDiagram, serializeFile, compactModelJSON,
  parseNumberList, formatValue, gridLength, checkParameter, checkSimulationField,
  DiagramCore, ports, blockHeight, portY, wireGeometry, wireLanes, bounds,
  octaveString, diagramStem, resultName, regexpLiteral, runCode, errorTarget, firstErrorLine, isDiagramPath, canonicalBlock
};
