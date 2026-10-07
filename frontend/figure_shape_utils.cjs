"use strict";

// Pure helpers of the 2D viewer for filled shapes, text and ticks.
// No DOM, canvas, engine or globals: everything here runs in node tests.

const GREEK = { alpha: "α", beta: "β", gamma: "γ", delta: "δ", epsilon: "ε", zeta: "ζ", eta: "η", theta: "θ", vartheta: "ϑ",
  iota: "ι", kappa: "κ", lambda: "λ", mu: "μ", nu: "ν", xi: "ξ", o: "ο", pi: "π", varpi: "ϖ", rho: "ρ", sigma: "σ", varsigma: "ς",
  tau: "τ", upsilon: "υ", phi: "φ", chi: "χ", psi: "ψ", omega: "ω", Gamma: "Γ", Delta: "Δ", Theta: "Θ", Lambda: "Λ", Xi: "Ξ",
  Pi: "Π", Sigma: "Σ", Upsilon: "Υ", Phi: "Φ", Psi: "Ψ", Omega: "Ω" };
const SYMBOLS = { pm: "±", times: "×", cdot: "·", div: "÷", leq: "≤", geq: "≥", neq: "≠", approx: "≈", equiv: "≡", propto: "∝",
  infty: "∞", partial: "∂", nabla: "∇", int: "∫", sum: "∑", prod: "∏", surd: "√", circ: "∘", deg: "°", bullet: "•", ldots: "…",
  leftarrow: "←", rightarrow: "→", uparrow: "↑", downarrow: "↓", leftrightarrow: "↔", Leftarrow: "⇐", Rightarrow: "⇒",
  in: "∈", cap: "∩", cup: "∪", forall: "∀", exists: "∃", neg: "¬", angle: "∠", perp: "⊥", mid: "|", sim: "∼", copyright: "©" };
const FONT_COMMANDS = { bf: { bold: true }, it: { italic: true }, sl: { italic: true }, rm: { bold: false, italic: false } };
// Commands whose arguments select fonts or colours: dropped, the text stays.
const DROPPED = ["fontname", "fontsize", "color"];

// Octave's TeX subset as styled runs: Greek letters and common symbols,
// ^ and _ (one character or a braced group), \bf \it \sl \rm, braces.
// \fontname, \fontsize and \color are dropped with their arguments; an
// unknown command stays as typed, so the result is always readable text.
function parseTex(source, interpreter = "tex") {
  const text = String(source);
  if (interpreter === "none") return text ? [{ text, script: 0, bold: false, italic: false }] : [];
  const runs = [];
  const push = (value, style) => {
    if (!value) return;
    const last = runs[runs.length - 1];
    if (last && last.script === style.script && last.bold === style.bold && last.italic === style.italic) last.text += value;
    else runs.push({ text: value, script: style.script, bold: style.bold, italic: style.italic });
  };
  let i = 0;
  const command = () => {
    // text[i] is the backslash. Returns {name} or {literal}.
    const next = text[i + 1];
    if (next === undefined) {
      i++;
      return { literal: "\\" };
    }
    if (!/[A-Za-z]/.test(next)) {
      i += 2;
      return { literal: next };
    }
    let j = i + 1;
    while (j < text.length && /[A-Za-z]/.test(text[j])) j++;
    const name = text.slice(i + 1, j);
    i = j;
    return { name };
  };
  const skipArgument = () => {
    if (text[i] === "[") {
      const end = text.indexOf("]", i);
      if (end >= 0) i = end + 1;
    }
    if (text[i] === "{") {
      const end = text.indexOf("}", i);
      i = end >= 0 ? end + 1 : text.length;
    }
  };
  const symbol = (name, style) => {
    if (Object.prototype.hasOwnProperty.call(GREEK, name)) push(GREEK[name], style);
    else if (Object.prototype.hasOwnProperty.call(SYMBOLS, name)) push(SYMBOLS[name], style);
    else if (DROPPED.includes(name)) skipArgument();
    else if (Object.prototype.hasOwnProperty.call(FONT_COMMANDS, name)) return FONT_COMMANDS[name];
    else push("\\" + name, style);
    return null;
  };
  const group = (style, braced, depth) => {
    let current = style;
    while (i < text.length) {
      const ch = text[i];
      if (ch === "}") {
        i++;
        if (braced) return;
        continue;
      }
      if (ch === "{" && depth < 16) {
        i++;
        group(current, true, depth + 1);
      } else if (ch === "\\") {
        const token = command();
        if (token.literal !== undefined) push(token.literal, current);
        else {
          const change = symbol(token.name, current);
          if (change) current = { ...current, ...change };
        }
      } else if ((ch === "^" || ch === "_") && i + 1 < text.length) {
        const scripted = { ...current, script: ch === "^" ? 1 : -1 };
        i++;
        if (text[i] === "{" && depth < 16) {
          i++;
          group(scripted, true, depth + 1);
        } else if (text[i] === "\\") {
          const token = command();
          if (token.literal !== undefined) push(token.literal, scripted);
          else symbol(token.name, scripted);
        } else {
          // One whole code point, never half a surrogate pair.
          const point = String.fromCodePoint(text.codePointAt(i));
          push(point, scripted);
          i += point.length;
        }
      } else {
        push(ch, current);
        i++;
      }
    }
  };
  group({ script: 0, bold: false, italic: false }, false, 0);
  return runs;
}
function plainText(source, interpreter = "tex") {
  return parseTex(source, interpreter).map((run) => run.text).join("");
}

// Block layout around the anchor (0,0), y pointing down, before rotation.
// widths: measured width per line. Returns the text block, the box (block
// plus margin) and one {x, baseline} per line.
function layoutText({ widths, lineHeight, ascent, descent, halign = "left", valign = "middle", margin = 0 }) {
  const count = widths.length;
  const width = count ? Math.max(...widths) : 0;
  const height = count * lineHeight;
  const x0 = halign === "center" ? 0 - width / 2 : halign === "right" ? 0 - width : 0;
  const lead = (lineHeight - (ascent + descent)) / 2;
  let y0;
  if (valign === "top" || valign === "cap") y0 = 0;
  else if (valign === "bottom") y0 = 0 - height;
  else if (valign === "baseline") y0 = 0 - (height - lead - descent);
  else y0 = 0 - height / 2;
  const lines = widths.map((w, index) => ({
    x: x0 + (halign === "center" ? (width - w) / 2 : halign === "right" ? width - w : 0),
    baseline: y0 + index * lineHeight + lead + ascent
  }));
  return { width, height, x: x0, y: y0, lines, box: { x: x0 - margin, y: y0 - margin, w: width + 2 * margin, h: height + 2 * margin } };
}

// Faces of a validated patch2d record as point lists. A face that reads a
// missing (null) vertex is left out; null face entries are trailing padding.
function facePolygons(record) {
  const [rows, cols] = record.faces.shape;
  const n = record.vertices.shape[0];
  const out = [];
  for (let r = 0; r < rows; r++) {
    const points = [];
    let missing = false;
    for (let c = 0; c < cols; c++) {
      const index = record.faces.values[r + c * rows];
      if (index === null) break;
      const x = record.vertices.values[index], y = record.vertices.values[index + n];
      if (x === null || y === null) {
        missing = true;
        break;
      }
      points.push([x, y]);
    }
    if (!missing && points.length >= 2) out.push({ index: r, points });
  }
  return out;
}
// Even-odd rule, the same rule the canvas fill uses.
function pointInPolygon(points, x, y) {
  let inside = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
    const [xi, yi] = points[i], [xj, yj] = points[j];
    if ((yi > y) !== (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}
function faceRectangle(points) {
  if (points.length !== 4) return null;
  const xs = points.map((p) => p[0]), ys = points.map((p) => p[1]);
  const x0 = Math.min(...xs), x1 = Math.max(...xs), y0 = Math.min(...ys), y1 = Math.max(...ys);
  for (const [x, y] of points) if ((x !== x0 && x !== x1) || (y !== y0 && y !== y1)) return null;
  for (let i = 0; i < 4; i++) {
    const a = points[i], b = points[(i + 1) % 4];
    if (a[0] !== b[0] && a[1] !== b[1]) return null;
  }
  return { x0, x1, y0, y1 };
}
// What a data tip says about one face: numbers, or [low, high] edges.
// Bars report the group's own position and value; touching bars (width >= 1,
// histogram style) and plain rectangles report their edges instead.
function faceTip(record, face) {
  const rectangle = faceRectangle(face.points);
  const far = (lo, hi, base) => Math.abs(hi - base) >= Math.abs(lo - base) ? hi : lo;
  if (record.bar) {
    const bar = record.bar;
    let position = bar.positions[face.index];
    const value = bar.values[face.index];
    if (position === null || value === null) return null;
    if (rectangle && bar.width >= 1) position = bar.horizontal ? [rectangle.y0, rectangle.y1] : [rectangle.x0, rectangle.x1];
    return bar.horizontal ? { x: value, y: position } : { x: position, y: value };
  }
  if (!rectangle) return null;
  return { x: [rectangle.x0, rectangle.x1], y: far(rectangle.y0, rectangle.y1, 0) };
}

// "Nice" linear ticks (steps 1, 2, 5 times a power of ten) inside [lo, hi].
function niceTicks(lo, hi, target = 5) {
  if (!Number.isFinite(lo) || !Number.isFinite(hi) || lo === hi) return [];
  if (lo > hi) [lo, hi] = [hi, lo];
  const raw = (hi - lo) / Math.max(1, target);
  const power = 10 ** Math.floor(Math.log10(raw));
  const fraction = raw / power;
  const step = (fraction < 1.5 ? 1 : fraction < 3.5 ? 2 : fraction < 7.5 ? 5 : 10) * power;
  const out = [];
  const first = Math.ceil(lo / step - 1e-9);
  for (let k = first; k * step <= hi + step * 1e-9 && out.length < 50; k++) out.push(Number((k * step).toPrecision(12)));
  return out;
}
// Ticks of one axis for the current view: [{value, label|null}], or null
// when the caller should keep its own evenly spaced ticks (log scale, or an
// older artifact without tick data). label null = format the number.
// At the exported limits these are Octave's own ticks and labels. After a
// zoom or pan, manual tick labels (categories) stay on their ticks; manual
// ticks stay while at least two are in view; otherwise nice ticks are used.
function axisTicks(axis, which, range, initial) {
  const ticks = axis[which + "tick"], labels = axis[which + "ticklabel"];
  const scale = axis[which + "scale"];
  const lo = Math.min(range[0], range[1]), hi = Math.max(range[0], range[1]);
  const slack = (hi - lo) * 1e-9;
  const exported = Array.isArray(ticks) ? ticks.map((value, index) => ({ value, label: Array.isArray(labels) && labels.length === ticks.length ? labels[index] : labels && labels.length === 0 ? "" : null })).filter((tick) => tick.value >= lo - slack && tick.value <= hi + slack) : null;
  const same = initial && range[0] === initial[0] && range[1] === initial[1];
  if (exported && same) return exported;
  if (exported && axis[which + "ticklabelmode"] === "manual") return exported;
  if (exported && axis[which + "tickmode"] === "manual" && exported.length >= 2) return exported;
  if (scale === "log" || !Array.isArray(ticks)) return null;
  return niceTicks(lo, hi).map((value) => ({ value, label: null }));
}

module.exports = { parseTex, plainText, layoutText, facePolygons, pointInPolygon, faceRectangle, faceTip, niceTicks, axisTicks, GREEK, SYMBOLS };
