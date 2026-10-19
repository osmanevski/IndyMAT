"use strict";
// Model core of the block diagram editor: strict file format, export equals
// schema 1, layout round trip, editing with undo/redo, port rules, panel number
// checks, wire geometry, generated Run code and compile-error targets.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const u = require("../frontend/block_model_utils.cjs");

const examplePath = path.join(__dirname, "..", "workspace", "examples", "pid_kapali_cevrim.imbd.json");
const exampleText = fs.readFileSync(examplePath, "utf8");
let checks = 0;
const ok = (value, message) => { assert.ok(value, message); checks++; };
const eq = (a, b, message) => { assert.deepStrictEqual(a, b, message); checks++; };
const rejects = (text, code, label) => {
  const read = u.readDiagram(typeof text === "string" ? text : JSON.stringify(text));
  ok(!read.ok, label + ": accepted");
  eq(read.error.code, code, label + ": " + JSON.stringify(read.error));
};
const clone = (v) => JSON.parse(JSON.stringify(v));

// ---- example file: valid, canonical, export equals its schema-1 model --------
const read = u.readDiagram(exampleText);
ok(read.ok, "example is valid");
const core = new u.DiagramCore(read.file);
eq(core.text(), exampleText, "example is stored in canonical form (round trip is byte-identical)");
eq(core.model(), JSON.parse(exampleText).model, "export equals the file's schema-1 model");
eq(core.fileObject().layout, JSON.parse(exampleText).layout, "layout round trip");
eq(Object.keys(core.model()), ["schemaVersion", "blocks", "connections", "simulation"], "model keys are exactly schema 1");
eq(core.blocks.length, 11, "11 blocks");
eq(core.connections.length, 14, "14 connections");

// ---- strict JSON and unknown fields -------------------------------------------
const base = JSON.parse(exampleText);
rejects('{"imbVersion":1,"imbVersion":1,"model":{},"layout":{}}', "duplicate-key", "duplicate root key");
rejects(exampleText.replace('"time": 0,', '"time": 0, "\\u0074ime": 1,'), "duplicate-key", "escaped-equivalent duplicate key");
rejects(exampleText + "x", "json", "trailing garbage");
rejects("{\"a\":1,}", "json", "trailing comma");
rejects("{\"a\":01}", "json", "leading zero");
rejects("{\"a\":NaN}", "json", "NaN literal");
rejects("{\"a\":\"x\ny\"}", "json", "raw control character in a string");
for (const [label, mutate] of [
  ["root", (f) => { f.extra = 1; }],
  ["model", (f) => { f.model.layout = {}; }],
  ["block", (f) => { f.model.blocks[0].reset = 1; }],
  ["params", (f) => { f.model.blocks[2].params.Kp = 1; }],
  ["simulation", (f) => { f.model.simulation.solverOptions = {}; }],
  ["connection", (f) => { f.model.connections[0].label = "a"; }],
  ["endpoint", (f) => { f.model.connections[0].from.signal = 1; }],
  ["layout", (f) => { f.layout.view = {}; }],
  ["layout entry", (f) => { f.layout.blocks.error.width = 3; }]
]) {
  const f = clone(base);
  mutate(f);
  rejects(f, "unknown-field", "unknown field in " + label);
}
let f = clone(base); delete f.layout.blocks.plant; rejects(f, "layout-missing", "missing layout entry");
f = clone(base); f.layout.blocks.ghost = { x: 0, y: 0 }; rejects(f, "layout-extra", "layout for unknown block");
f = clone(base); f.layout.blocks.plant.x = 2e6; rejects(f, "coordinate", "coordinate limit");
f = clone(base); f.imbVersion = 2; rejects(f, "imb-version", "imbVersion");
f = clone(base); f.model.schemaVersion = 2; rejects(f, "schema-version", "schemaVersion");
f = clone(base); f.model.blocks[0].type = "TriggeredSubsystem"; rejects(f, "type", "unsupported type");
f = clone(base); f.model.blocks[1].id = "1bad"; rejects(f, "block-id", "id pattern");
f = clone(base); f.model.blocks[1].id = "reference"; rejects(f, "duplicate-id", "duplicate id");
f = clone(base); f.model.blocks[2].params.gain = "2"; rejects(f, "number", "parameter text is not a number");
f = clone(base); delete f.model.blocks[3].initialState; rejects(f, "state-missing", "Integrator without initialState");
f = clone(base); f.model.blocks[5].initialState = 0; rejects(f, "state-forbidden", "TransferFcn with initialState");
f = clone(base); f.model.connections[0].to.port = 3; rejects(f, "endpoint-port", "port beyond Sum signs");
f = clone(base); f.model.connections[0].to.block = "nobody"; rejects(f, "endpoint-block", "unknown endpoint block");
f = clone(base); f.model.simulation.mode = "hybrid"; rejects(f, "mode", "unknown mode");
f = clone(base); f.model.simulation.Ts = 0.1; rejects(f, "unknown-field", "discrete field in continuous mode");
// Semantic rules are im_compile's: these files open and fail at Run.
f = clone(base); f.model.connections.splice(0, 1); ok(u.readDiagram(JSON.stringify(f)).ok, "an undriven input still opens");
f = clone(base); f.model.blocks[1].params.signs = [1, 2]; ok(u.readDiagram(JSON.stringify(f)).ok, "sign values are checked by im_compile");
f = clone(base); f.model.blocks = []; f.model.connections = []; f.layout.blocks = {}; ok(u.readDiagram(JSON.stringify(f)).ok, "an empty diagram opens");

// ---- editing, port rules, undo/redo -------------------------------------------
const c = new u.DiagramCore(read.file);
const initial = c.snapshot();
let r = c.connect("reference", 1, "error", 1);
eq(r.error.code, "input-taken", "occupied input refuses a second wire");
eq(c.snapshot(), initial, "refused edit leaves the model unchanged");
eq(c.history.length, 1, "refused edit records no history");
eq(c.connect("output", 1, "plant", 1).error.code, "no-output", "Scope has no output");
eq(c.connect("reference", 1, "plant", 2).error.code, "no-input", "input beyond the port count");
r = c.addBlock("Gain", 100, 400);
eq(r.result, "gain1", "fresh id");
eq(c.block("gain1").sampleTime, 0, "new block follows continuous sample time");
r = c.addBlock("UnitDelay", 300, 400);
eq(c.block("unitDelay1").initialState, 0, "state blocks get initialState");
for (let k = 0; k < 20; k++) c.moveBlock("gain1", 100 + k, 400 + k, false);
c.moveBlock("gain1", 140, 430, true);
eq(c.history.length, 4, "a whole drag is one history entry");
eq(c.layout.gain1, { x: 140, y: 430 }, "drag end position");
c.setParameter("control", "signs", [1, 1]);
ok(!c.connections.some((l) => l.to.block === "control" && l.to.port === 3), "Sum with fewer signs drops the wire on the removed input");
c.undo();
ok(c.connections.some((l) => l.to.block === "control" && l.to.port === 3), "undo brings the wire back");
c.redo();
ok(!c.connections.some((l) => l.to.block === "control" && l.to.port === 3), "redo removes it again");
c.setParameter("control", "signs", [1, 1, 1]);
eq(c.connect("derivative_gain", 1, "control", 3).ok, true, "reconnect to the new third input");
c.renameBlock("plant", "process");
ok(c.block("process") && !c.block("plant"), "rename");
ok(c.connections.filter((l) => l.from.block === "process").length === 2, "rename updates connections");
ok(c.layout.process && !c.layout.plant, "rename updates layout");
eq(c.renameBlock("process", "error").error.code, "duplicate-id", "rename refuses duplicates");
eq(c.renameBlock("process", "9x").error.code, "block-name", "rename refuses invalid ids");
c.deleteBlock("process");
ok(!c.connections.some((l) => l.from.block === "process" || l.to.block === "process"), "delete removes attached wires");
c.setMode("discrete");
ok(c.blocks.every((b) => b.sampleTime === 0.1), "discrete mode sets every sampleTime to Ts");
eq(Object.keys(c.simulation), u.SIMULATION_FIELDS.discrete, "discrete simulation fields");
c.setSimulation({ ...c.simulation, Ts: 0.05 });
ok(c.blocks.every((b) => b.sampleTime === 0.05), "Ts change follows to blocks");
c.setMode("continuous");
ok(c.blocks.every((b) => b.sampleTime === 0), "continuous again");
eq(c.simulation.relTol, 1e-9, "switching back restores the remembered continuous settings");
while (c.undo());
eq(c.snapshot(), initial, "undo all the way returns to the opened file");
while (c.redo());
eq(c.blocks.some((b) => b.id === "process"), false, "redo all the way");
const added = new u.DiagramCore(read.file);
added.setMode("discrete");
added.setSimulation({ ...added.simulation, tf: 1, Ts: 0.25 });
added.addBlock("SampledInput", 0, 0);
eq(added.block("sampledInput1").params.values, [0, 0, 0, 0, 0], "SampledInput gets one value per grid point");

// ---- panel number rules: nothing evaluated --------------------------------------
const gain = c.block("proportional") || added.block("proportional");
for (const text of ["pi", "1/2", "Inf", "NaN", "", "2 3x", "1e", "--1", "eval('1')", "0x10"]) eq(u.parseNumberList(text), null, "rejects " + JSON.stringify(text));
eq(u.parseNumberList("[1, -1 ; +2e-3 .5]"), [1, -1, 0.002, 0.5], "number list");
eq(u.checkParameter(gain, "gain", "2 3", c.simulation).error.code, "scalar", "scalar parameter");
eq(u.checkParameter(gain, "gain", "-0.5", c.simulation).value, -0.5, "scalar value");
const sum = { type: "Sum", params: { signs: [1, -1] } };
eq(u.checkParameter(sum, "signs", "1 0", c.simulation).error.code, "signs", "signs are +/-1");
eq(u.checkParameter(sum, "signs", "1 -1 1", c.simulation).value, [1, -1, 1], "three signs");
const tf = { type: "TransferFcn", params: { num: 1, den: [1, 1] } };
eq(u.checkParameter(tf, "den", "1", c.simulation).error.code, "degree", "denominator degree");
eq(u.checkParameter(tf, "num", "1 2", c.simulation).error.code, "proper", "strictly proper");
eq(u.checkParameter(tf, "den", "0 1 1", c.simulation).error.code, "proper", "leading zero");
eq(u.checkParameter({ type: "Mux", params: { inputs: 2 } }, "inputs", "2.5", c.simulation).error.code, "integer-range", "Mux inputs integer");
eq(u.checkParameter({ type: "Saturation", params: { lower: -1, upper: 1 } }, "lower", "2", c.simulation).error.code, "lower-upper", "lower <= upper");
eq(u.checkParameter({ type: "SampledInput", params: { values: [0] } }, "values", "1 2", { mode: "discrete", t0: 0, tf: 1, solver: "discrete", Ts: 0.25 }).error.code, "grid-values", "values match the grid");
eq(u.checkSimulationField(c.simulation, "tf", "-1").error.code, "t0-tf", "tf after t0");
eq(u.checkSimulationField(c.simulation, "Ts", "0").error.code, "positive", "Ts positive");

// ---- wire geometry: feedback goes below every block in its span -----------------
const g = new u.DiagramCore(read.file);
const feedback = g.connections.findIndex((l) => l.from.block === "plant" && l.to.block === "error");
const geo = u.wireGeometry(g, g.connections[feedback], 0);
ok(geo.feedback, "plant -> error is a feedback wire");
const lowest = Math.max(...g.blocks.map((b) => g.layout[b.id].y + u.blockHeight(b)));
ok(geo.routeY > lowest, `feedback corridor ${geo.routeY} lies below the lowest block ${lowest}`);
ok(!u.wireGeometry(g, g.connections[0], 0).feedback, "forward wire is a curve");
eq(u.wireLanes(g).size, 1, "one feedback lane");

// ---- generated Run code ----------------------------------------------------------
eq(u.resultName("/x/pid_kapali_cevrim.imbd.json"), "pid_kapali_cevrim_result", "result variable from the stem");
eq(u.resultName("/x/it's.imbd.json"), "diagram_result", "fallback for a non-identifier stem");
eq(u.resultName("/x/" + "a".repeat(57) + ".imbd.json"), "diagram_result", "fallback for a long stem");
eq(u.resultName("/x/ç.imbd.json"), "diagram_result", "fallback for a non-ASCII stem");
const code = u.runCode(core.model(), "pid_kapali_cevrim.imbd.json", { time: "Zaman (s)" });
const lines = code.trimEnd().split("\n");
ok(lines[1].startsWith("pid_kapali_cevrim_result = (@im_simulate)('{"), "assignment through a handle literal");
eq(JSON.parse(lines[1].slice(lines[1].indexOf("'") + 1, -3)), core.model(), "the literal is the compact model JSON");
ok(lines.includes("(@plot)(pid_kapali_cevrim_result.t, pid_kapali_cevrim_result.signals.output);"), "continuous Scope uses plot");
ok(lines.includes("(@close)((@findobj)(0, 'type', 'figure', '-regexp', 'name', '^pid_kapali_cevrim\\.imbd\\.json: output$'));"), "closes only the exact figure name");
ok(lines.includes("(@xlabel)('Zaman (s)');"), "axis label in the UI language");
const weird = clone(core.model());
weird.simulation.solver = "it's ç\n\t\u0001😀";
const literal = u.compactModelJSON(weird);
ok(/^[\x20-\x7e]+$/.test(literal), "compact JSON is printable ASCII on one line");
eq(JSON.parse(literal).simulation.solver, weird.simulation.solver, "escapes keep every character");
const weirdCode = u.runCode(weird, "it's ç.imbd.json");
ok(weirdCode.includes("'{\"schemaVersion\":1,") && weirdCode.includes("it''s"), "single quotes are doubled");
ok(weirdCode.includes("(@figure)('Name', 'it''s ç.imbd.json: output');"), "figure name keeps quotes and Unicode");
eq(u.regexpLiteral("a.b(c)[d]{e}^$|?*+\\"), "a\\.b\\(c\\)\\[d\\]\\{e\\}\\^\\$\\|\\?\\*\\+\\\\", "regexp metacharacters escaped");
const muxFile = clone(base);
muxFile.model.blocks.push({ id: "both", type: "Mux", params: { inputs: 2 }, sampleTime: 0 }, { id: "view", type: "Scope", params: {}, sampleTime: 0 });
muxFile.model.connections.push({ from: { block: "reference", port: 1 }, to: { block: "both", port: 1 } }, { from: { block: "plant", port: 1 }, to: { block: "both", port: 2 } }, { from: { block: "both", port: 1 }, to: { block: "view", port: 1 } });
muxFile.layout.blocks.both = { x: 0, y: 500 };
muxFile.layout.blocks.view = { x: 200, y: 500 };
const muxCore = new u.DiagramCore(u.readDiagram(JSON.stringify(muxFile)).file);
ok(u.runCode(muxCore.model(), "m.imbd.json").includes("(@legend)({'reference', 'plant'});"), "Mux scope legend lists the Mux sources");
const dCore = new u.DiagramCore(read.file);
dCore.setMode("discrete");
ok(u.runCode(dCore.model(), "d.imbd.json").includes("(@stairs)(d_result.t, d_result.signals.output);"), "discrete Scope uses stairs");

// ---- compile errors name a block or connection -----------------------------------
const m = core.model();
for (const [message, target] of [
  ["Block plant input 1 has no driver.", { kind: "block", id: "plant" }],
  ["error: Block derivative_filter requires strictly proper num/den vectors without leading zeros (scalar zero numerator allowed).", { kind: "block", id: "derivative_filter" }],
  ["Algebraic loop: error.out1 -> proportional.in1; proportional.out1 -> error.in2. Add no implicit delay; this model is unsupported.", { kind: "block", id: "error" }],
  ["Connection 3: port out of range (error.out1 -> proportional.in2).", { kind: "connection", index: 2 }],
  ["Mux output may connect only to a Scope input.", { kind: "block", id: "output" }],
  ["block 4 missing field initialState.", { kind: "block", id: "integral" }],
  ["Simulation duration must be an integer multiple of outputDt or Ts (32-ulp ratio tolerance).", null]
]) eq(u.errorTarget(message, m), target, "target of: " + message);
eq(u.firstErrorLine("error: Block x input 1 has no driver.\nerror: called from\n  im_compile at line 3"), "Block x input 1 has no driver.", "first error line");

// ---- 200 blocks: valid, serialised and parsed quickly ----------------------------
const big = { imbVersion: 1, model: { schemaVersion: 1, blocks: [], connections: [], simulation: clone(base.model.simulation) }, layout: { blocks: {} } };
big.model.blocks.push({ id: "b0", type: "Constant", params: { value: 1 }, sampleTime: 0 });
big.layout.blocks.b0 = { x: 0, y: 0 };
for (let k = 1; k < 199; k++) {
  big.model.blocks.push({ id: "b" + k, type: "Gain", params: { gain: 1.001 }, sampleTime: 0 });
  big.model.connections.push({ from: { block: "b" + (k - 1), port: 1 }, to: { block: "b" + k, port: 1 } });
  big.layout.blocks["b" + k] = { x: (k % 20) * 160, y: Math.floor(k / 20) * 90 };
}
big.model.blocks.push({ id: "out", type: "Scope", params: {}, sampleTime: 0 });
big.model.connections.push({ from: { block: "b198", port: 1 }, to: { block: "out", port: 1 } });
big.layout.blocks.out = { x: 0, y: 1000 };
const t0 = process.hrtime.bigint();
const bigCore = new u.DiagramCore(u.readDiagram(JSON.stringify(big)).file);
const bigText = bigCore.text();
const again = new u.DiagramCore(u.readDiagram(bigText).file);
const ms = Number(process.hrtime.bigint() - t0) / 1e6;
eq(again.text(), bigText, "200-block round trip");
eq(again.blocks.length, 200, "200 blocks");
ok(ms < 500, `200-block parse/serialise ${ms.toFixed(1)} ms`);

console.log(`BLOCK EDITOR UNIT PASS: ${checks} checks (200-block round trip ${ms.toFixed(1)} ms).`);
