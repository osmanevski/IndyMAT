"use strict";

const MODES = Object.freeze(["rotate", "zoom", "pan", "tips"]);
const MAX_TIPS = 16;

function initialState() {
  return { mode: "rotate", gesture: null, pins: [] };
}

function zoomFactor(delta) {
  return Math.exp(-Math.max(-600, Math.min(600, delta)) * 0.002);
}

// Returns target actions; never touches a DOM node, camera, engine or network.
function transition(state, event) {
  let next = state;
  const actions = [];
  const cancel = () => {
    if (next.gesture) actions.push({ type: "release", pointerId: next.gesture.pointerId });
    next = { ...next, gesture: null };
  };
  const reset = () => {
    cancel();
    next = initialState();
    actions.push({ type: "reset" });
  };
  switch (event.type) {
    case "mode":
      if (MODES.includes(event.mode) && event.mode !== state.mode) {
        cancel();
        next = { ...next, mode: event.mode };
      }
      break;
    case "down":
      if (state.gesture || ![0, 1].includes(event.button)) break;
      next = { ...state, gesture: { pointerId: event.pointerId, x: event.x, y: event.y,
        startX: event.x, startY: event.y, moved: false,
        mode: event.shiftKey || event.button === 1 ? "pan" : state.mode } };
      actions.push({ type: "capture", pointerId: event.pointerId });
      break;
    case "move": {
      const g = state.gesture;
      if (!g || g.pointerId !== event.pointerId) break;
      const dx = event.x - g.x;
      const dy = event.y - g.y;
      const mode = event.shiftKey ? "pan" : g.mode;
      const moved = g.moved || Math.hypot(event.x - g.startX, event.y - g.startY) > 4;
      next = { ...state, gesture: { ...g, x: event.x, y: event.y, moved, mode } };
      if (dx || dy) {
        if (mode === "rotate") actions.push({ type: "orbit", dx, dy });
        if (mode === "pan") actions.push({ type: "pan", dx, dy });
        if (mode === "zoom") actions.push({ type: "zoom", factor: zoomFactor(dy), anchor: { x: g.startX, y: g.startY } });
      }
      break;
    }
    case "up": {
      const g = state.gesture;
      if (!g || g.pointerId !== event.pointerId) break;
      cancel();
      if (g.mode === "tips" && !g.moved && !event.shiftKey && Math.hypot(event.x - g.startX, event.y - g.startY) <= 4) {
        actions.push({ type: "pick", x: event.x, y: event.y });
      }
      break;
    }
    case "cancel":
      if (state.gesture && (event.pointerId === undefined || event.pointerId === state.gesture.pointerId)) cancel();
      break;
    case "pin":
      if (event.hit) next = { ...state, pins: [...state.pins, { hit: { ...event.hit }, anchor: { ...event.anchor } }].slice(-MAX_TIPS) };
      break;
    case "wheel":
      if (Number.isFinite(event.delta) && event.delta) actions.push({ type: "zoom", factor: zoomFactor(event.delta), anchor: event.anchor });
      break;
    case "reset":
      reset();
      break;
    case "key": {
      if (event.ctrlKey || event.metaKey || event.altKey) break;
      if (event.key === "Escape") {
        if (state.gesture) cancel();
        else next = { ...state, pins: [] };
      } else if (event.key === "Home") reset();
      else if (["+", "=", "-", "_"].includes(event.key)) actions.push({ type: "zoom", factor: ["+", "="].includes(event.key) ? 1.2 : 1 / 1.2, anchor: event.anchor });
      else if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
        const dx = event.key === "ArrowLeft" ? -20 : event.key === "ArrowRight" ? 20 : 0;
        const dy = event.key === "ArrowUp" ? -20 : event.key === "ArrowDown" ? 20 : 0;
        if (state.mode === "pan" || event.shiftKey) actions.push({ type: "pan", dx, dy });
        else if (dx) actions.push({ type: "rotateAzimuth", deg: dx < 0 ? -5 : 5 });
        else actions.push({ type: "orbit", dx, dy });
      }
      break;
    }
  }
  return { state: next, actions };
}

function formatNumber(value) {
  if (!Number.isFinite(value)) return String(value);
  const magnitude = Math.abs(value);
  return magnitude && (magnitude >= 1e5 || magnitude < 1e-4) ? value.toExponential(4) : Number(value.toPrecision(6)).toString();
}

// Hit indices stay zero-based internally; labels are one-based for Octave users.
// R3 supplies {x,y,z,seriesName,index} or {x,y,z,seriesName,row,column},
// optionally colorValue (raw scalar or RGB triple). No artifact is changed.
function tipText(hit, labels = {}) {
  const words = { series: "Series", index: "Index", row: "Row", column: "Column", color: "Color value", ...labels };
  const lines = [];
  if (typeof hit.seriesName === "string" && hit.seriesName) lines.push(`${words.series}: ${hit.seriesName}`);
  for (const key of ["x", "y", "z"]) {
    if (Number.isFinite(hit[key])) lines.push(`${key.toUpperCase()}: ${formatNumber(hit[key])}`);
  }
  if (Number.isSafeInteger(hit.row) && hit.row >= 0 && Number.isSafeInteger(hit.column) && hit.column >= 0) {
    lines.push(`${words.row}: ${hit.row + 1}, ${words.column}: ${hit.column + 1}`);
  } else if (Number.isSafeInteger(hit.index) && hit.index >= 0) lines.push(`${words.index}: ${hit.index + 1}`);
  if (Number.isFinite(hit.colorValue)) lines.push(`${words.color}: ${formatNumber(hit.colorValue)}`);
  else if (Array.isArray(hit.colorValue) && hit.colorValue.every(Number.isFinite)) lines.push(`${words.color}: [${hit.colorValue.map(formatNumber).join(", ")}]`);
  return lines.join("\n");
}

module.exports = { MODES, MAX_TIPS, initialState, transition, formatNumber, tipText };
