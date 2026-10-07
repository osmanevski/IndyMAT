"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { MODES, MAX_TIPS, initialState, transition, formatNumber, tipText } = require("../frontend/figure_tool_utils.cjs");
const { validate } = require("../frontend/figure_data_utils.cjs");
const { translate } = require("../frontend/i18n_utils.cjs");
const localeSource = fs.readFileSync(path.join(__dirname, "../frontend/locales/tr_dosyalar.js"), "utf8");
const locale = new Function(localeSource.replace("export default", "return"))();
let count = 0;
function check(name, fn) {
  fn();
  count++;
}
const down = (extras = {}) => ({ type: "down", pointerId: 7, button: 0, x: 10, y: 20, ...extras });
const move = (extras = {}) => ({ type: "move", pointerId: 7, x: 30, y: 25, ...extras });
const up = (extras = {}) => ({ type: "up", pointerId: 7, x: 10, y: 20, ...extras });
const modeState = (mode) => transition(initialState(), { type: "mode", mode }).state;
check("default and exclusive modes", () => {
  assert.equal(initialState().mode, "rotate");
  for (const mode of MODES) assert.equal(modeState(mode).mode, mode);
  const state = initialState();
  assert.equal(transition(state, { type: "mode", mode: "invalid" }).state, state);
});
check("immutable input and pointer identity", () => {
  const state = Object.freeze(initialState());
  const result = transition(state, down());
  assert.equal(state.gesture, null);
  assert.deepEqual(result.actions, [{ type: "capture", pointerId: 7 }]);
  const dragging = result.state;
  assert.equal(transition(dragging, move({ pointerId: 8 })).state, dragging);
  assert.equal(transition(dragging, down({ pointerId: 8 })).state, dragging);
  assert.equal(transition(dragging, up({ pointerId: 8 })).state, dragging);
  assert.equal(transition(state, down({ button: 2 })).state, state);
});
check("drag actions", () => {
  for (const mode of ["rotate", "pan", "zoom"]) {
    const result = transition(transition(modeState(mode), down()).state, move());
    const action = result.actions[0];
    assert.equal(action.type, mode === "rotate" ? "orbit" : mode);
    if (mode === "zoom") {
      assert(action.factor > 0 && action.factor < 1);
      assert.deepEqual(action.anchor, { x: 10, y: 20 });
    } else assert.deepEqual([action.dx, action.dy], [20, 5]);
  }
});
check("pan overrides every mode", () => {
  for (const mode of MODES) {
    for (const extras of [{ shiftKey: true }, { button: 1 }]) {
      const state = transition(modeState(mode), down(extras)).state;
      assert.equal(state.mode, mode);
      assert.equal(state.gesture.mode, "pan");
      assert.equal(transition(state, move()).actions[0].type, "pan");
      assert(!transition(state, up()).actions.some((a) => a.type === "pick"));
    }
    const state = transition(modeState(mode), down()).state;
    const shifted = transition(state, move({ shiftKey: true }));
    assert.equal(shifted.actions[0].type, "pan");
    assert.equal(shifted.state.gesture.mode, "pan");
    assert(!transition(shifted.state, up()).actions.some((a) => a.type === "pick"));
  }
});
check("cancel, mode changes and capture release", () => {
  const state = transition(initialState(), down()).state;
  for (const event of [{ type: "cancel", pointerId: 7 }, { type: "key", key: "Escape" }, { type: "mode", mode: "tips" }]) {
    const result = transition(state, event);
    assert.equal(result.state.gesture, null);
    assert.deepEqual(result.actions, [{ type: "release", pointerId: 7 }]);
  }
  assert.equal(transition(state, { type: "cancel", pointerId: 8 }).state, state);
  assert.equal(transition(state, up()).state.gesture, null);
});
check("tips pick only clicks, never cancelled or dragged gestures", () => {
  let state = transition(modeState("tips"), down()).state;
  assert.deepEqual(transition(state, up()).actions, [{ type: "release", pointerId: 7 }, { type: "pick", x: 10, y: 20 }]);
  state = transition(state, move()).state;
  assert(!transition(state, up()).actions.some((a) => a.type === "pick"));
  state = transition(state, { type: "cancel", pointerId: 7 }).state;
  assert.deepEqual(transition(state, up()).actions, []);
  state = transition(modeState("tips"), down()).state;
  assert(!transition(state, up({ x: 60 })).actions.some((a) => a.type === "pick"));
});
check("pin miss, bounded pins and Escape", () => {
  let state = modeState("tips");
  assert.equal(transition(state, { type: "pin", hit: null }).state, state);
  for (let index = 0; index < MAX_TIPS + 1; index++) state = transition(state, { type: "pin", hit: { index }, anchor: { x: 1, y: 2 } }).state;
  assert.equal(state.pins.length, MAX_TIPS);
  assert.equal(state.pins[0].hit.index, 1);
  const dragging = transition(state, down()).state;
  const cancelled = transition(dragging, { type: "key", key: "Escape" }).state;
  assert.equal(cancelled.pins.length, MAX_TIPS, "first Escape must cancel the gesture");
  assert.deepEqual(transition(cancelled, { type: "key", key: "Escape" }).state.pins, []);
});
check("keyboard rotation, pan and zoom", () => {
  const action = (key, mode = "rotate", shiftKey = false) => transition(modeState(mode), { type: "key", key, shiftKey, anchor: { x: 50, y: 60 } }).actions[0];
  assert.deepEqual(action("ArrowLeft"), { type: "rotateAzimuth", deg: -5 });
  assert.deepEqual(action("ArrowRight"), { type: "rotateAzimuth", deg: 5 });
  assert.deepEqual(action("ArrowUp"), { type: "orbit", dx: 0, dy: -20 });
  assert.deepEqual(action("ArrowDown"), { type: "orbit", dx: 0, dy: 20 });
  assert.deepEqual(action("ArrowLeft", "pan"), { type: "pan", dx: -20, dy: 0 });
  for (const mode of MODES) assert.deepEqual(action("ArrowUp", mode, true), { type: "pan", dx: 0, dy: -20 });
  assert.equal(action("+").factor, 1.2);
  assert.equal(action("=").factor, 1.2);
  assert.equal(action("-").factor, 1 / 1.2);
  assert.deepEqual(action("+").anchor, { x: 50, y: 60 });
  assert.deepEqual(transition(initialState(), { type: "key", key: "ArrowLeft", ctrlKey: true }).actions, []);
  assert.deepEqual(transition(initialState(), { type: "key", key: "a" }).actions, []);
});
check("Home and external reset restore tool state and release gestures", () => {
  let state = transition(modeState("tips"), { type: "pin", hit: { index: 1 }, anchor: { x: 1, y: 1 } }).state;
  state = transition(state, down()).state;
  for (const event of [{ type: "key", key: "Home" }, { type: "reset" }]) {
    const result = transition(state, event);
    assert.deepEqual(result.state, initialState());
    assert.deepEqual(result.actions, [{ type: "release", pointerId: 7 }, { type: "reset" }]);
  }
});
check("wheel zoom in every mode is anchored, finite and bounded", () => {
  for (const mode of MODES) {
    const result = transition(modeState(mode), { type: "wheel", delta: -120, anchor: { x: 9, y: 10 } });
    assert.equal(result.actions[0].type, "zoom");
    assert(result.actions[0].factor > 1);
    assert.deepEqual(result.actions[0].anchor, { x: 9, y: 10 });
    assert.equal(result.state.mode, mode);
  }
  assert.deepEqual(transition(initialState(), { type: "wheel", delta: Infinity }).actions, []);
  assert(Number.isFinite(transition(initialState(), { type: "wheel", delta: 1e10 }).actions[0].factor));
});
check("number formatting matches existing 2D tips", () => {
  assert.equal(formatNumber(123456), "1.2346e+5");
  assert.equal(formatNumber(0.00001), "1.0000e-5");
  assert.equal(formatNumber(1.23456789), "1.23457");
  assert.equal(formatNumber(0), "0");
});
function fixture(name) {
  const result = validate(JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json"), "utf8")));
  assert(result.ok && result.supported, name + " did not pass admission");
  return result.data.axes[0].series[0];
}
check("real line, point and surface tip values in English and Turkish", () => {
  const line = fixture("plot3_gap");
  const point = fixture("scatter3_sizes");
  const surface = fixture("surf_vector");
  const lineHit = { kind: "line", x: line.x[3], y: line.y[3], z: line.z[3], index: line.source_indices[3], seriesName: "Trace" };
  const pointHit = { kind: "point", x: point.x[1], y: point.y[1], z: point.z[1], index: point.source_indices[1], seriesName: "Samples", colorValue: point.color_data.data.values };
  const surfaceHit = { kind: "surface", x: surface.x.values[1], y: surface.y.values[1], z: surface.z.values[3], row: 1, column: 1, seriesName: "Grid", colorValue: surface.cdata.values[3] };
  const labels = Object.fromEntries([ ["series", "Series"], ["index", "Index"], ["row", "Row"], ["column", "Column"], ["color", "Color value"] ].map(([name, key]) => [name, translate(key, {}, "tr", locale)]));
  assert.equal(tipText(lineHit), "Series: Trace\nX: 4\nY: 6\nZ: 8\nIndex: 4");
  assert.equal(tipText(lineHit, labels), "Seri: Trace\nX: 4\nY: 6\nZ: 8\nİndeks: 4");
  assert.equal(tipText(pointHit), "Series: Samples\nX: 2\nY: 5\nZ: 8\nIndex: 2\nColor value: [0.2, 0.4, 0.6]");
  assert.equal(tipText(pointHit, labels), "Seri: Samples\nX: 2\nY: 5\nZ: 8\nİndeks: 2\nRenk değeri: [0.2, 0.4, 0.6]");
  assert.equal(tipText(surfaceHit), "Series: Grid\nX: 20\nY: 50\nZ: 4\nRow: 2, Column: 2\nColor value: 4");
  assert.equal(tipText(surfaceHit, labels), "Seri: Grid\nX: 20\nY: 50\nZ: 4\nSatır: 2, Sütun: 2\nRenk değeri: 4");
  assert(!tipText({ x: null, y: NaN, z: Infinity }).includes("0"), "nonfinite/missing coordinates became zero");
});
console.log(`${count} figure tool cases passed; real line/point/surface fixtures checked.`);
