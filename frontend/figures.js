import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";
import figureData from "./figure_data_utils.cjs";
import shapes from "./figure_shape_utils.cjs";
import images from "./figure_image_utils.cjs";
import { Figure3D, drawColorbars } from "./figure3d.js";
import { figureReasonText, figureReductionText, unmountFigureTools } from "./figure_tools.js";
import viewports from "./figure_viewport_utils.cjs";
import { mountFigureWindowControls } from "./figure_window_controls.js";

let manifestRevision = 0;
let manifestPending = false;
let requestRevision = 0;
let activeRequestKey = "";
let figureEpoch = null;
let enlarged = null;
const cameraStates = new Map();

async function updateFigures(s) {
  const generation = shared.uiGeneration;
  const figures = (s.figures || []).map((figure) => ({ ...figure, job: figure.job || s.job }));
  const key = s.epoch + ":" + JSON.stringify(figures);
  if (key === shared.lastFiguresKey) return;
  shared.lastFiguresKey = key;
  const revision = ++manifestRevision;
  manifestPending = true;
  ++requestRevision;
  activeRequestKey = "";
  closeEnlarged(false);
  shared.interactivePlot?.destroy();
  shared.interactivePlot = null;
  // PNGs remain available for downloads and fallback. JSON is active-only.
  const results = await Promise.allSettled(figures.map((figure) => registry.blobAPI(`figure?job=${figure.job}&file=${encodeURIComponent(figure.file)}`)));
  const urls = results.map((result) => result.status === "fulfilled" ? result.value : "");
  if (generation !== shared.uiGeneration || revision !== manifestRevision) {
    urls.filter(Boolean).forEach(URL.revokeObjectURL);
    if (revision === manifestRevision) manifestPending = false;
    return;
  }
  shared.figureURLs.filter(Boolean).forEach(URL.revokeObjectURL);
  shared.figureURLs = urls;
  shared.figures = figures;
  shared.figureData = [];
  figureEpoch = s.epoch;
  manifestPending = false;
  const identities = new Set(figures.map((figure, i) => stateKey(i)));
  for (const identity of cameraStates.keys()) if (!identities.has(identity)) cameraStates.delete(identity);
  const persisted = shared.settings?.activeTabs?.figureName;
  if (persisted && figures.some((figure) => figure.name === persisted)) shared.figureIndex = figures.findIndex((figure) => figure.name === persisted);
  shared.figureIndex = Math.min(shared.figureIndex, Math.max(0, figures.length - 1));
  renderFigures();
}
function identityAt(index) {
  const figure = shared.figures[index];
  const ordinal = /^figure-(\d+)\.(?:json|png)$/.exec(figure?.data_file || figure?.file || "");
  return { epoch: figureEpoch, job: figure?.job, figure: ordinal ? Number(ordinal[1]) : index + 1 };
}
function stateKey(index) {
  return JSON.stringify(identityAt(index));
}
function loadActiveData() {
  const index = shared.figureIndex;
  const figure = shared.figures[index];
  const key = stateKey(index);
  if (activeRequestKey !== key) {
    ++requestRevision;
    activeRequestKey = key;
  }
  if (!figure) return;
  const cached = shared.figureData[index];
  if (cached && (!cached.loading || cached.revision === requestRevision)) return;
  const revision = ++requestRevision;
  const generation = shared.uiGeneration;
  shared.figureData[index] = { loading: true, supported: false, revision };
  if (!figure.data_file) {
    shared.figureData[index] = { supported: false, reason: figure.fallback_reason };
    return;
  }
  registry.api(`figure?job=${figure.job}&file=${encodeURIComponent(figure.data_file)}`).then((payload) => {
    if (generation !== shared.uiGeneration || revision !== requestRevision || key !== stateKey(shared.figureIndex)) return null;
    // Validate before any geometry, renderer, or legacy normalization reads it.
    const result = figureData.validate(payload);
    if (!result.ok || !result.supported) return { supported: false, reason_code: result.reason_code, reason_args: result.reason_args };
    if (result.version === 3 && (result.data.source.job !== figure.job || result.data.source.figure !== identityAt(index).figure)) return { supported: false, reason_code: "invalid_data" };
    return result.data;
  }).catch((error) => ({ supported: false, reason_code: error.details?.reason_code || "invalid_data", reason_args: error.details?.reason_args || {} })).then((data) => {
    if (generation !== shared.uiGeneration || revision !== requestRevision || key !== stateKey(shared.figureIndex)) {
      // A cancelled tab request must be retryable when the tab is revisited.
      if (key === stateKey(index) && shared.figureData[index]?.revision === revision) shared.figureData[index] = null;
      return;
    }
    shared.figureData[index] = data;
    renderFigures();
  });
}
const emptyPlot = { value: null };
const plotNumber = (n) => n !== null && n !== "" && Number.isFinite(Number(n)) ? Number(n) : null;
function cssColor(name) {
  return getComputedStyle(document.body).getPropertyValue(name).trim();
}
function plotTheme() {
  return { background: cssColor("--plot-bg"), text: cssColor("--plot-text"), grid: cssColor("--plot-grid"), axis: cssColor("--plot-axis"), selection: cssColor("--plot-selection") };
}
function plotLuminance(rgb) {
  return rgb.map((v) => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4).reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
}
function plotColor(value, palette = plotTheme()) {
  if (value === "none") return "transparent";
  if (!Array.isArray(value) || value.length !== 3 || !value.every(Number.isFinite)) return palette.text;
  const rgb = value.map((v) => Math.max(0, Math.min(1, v))), foreground = plotLuminance(rgb), background = plotLuminance(palette.background.slice(1).match(/../g).map((v) => parseInt(v, 16) / 255));
  const contrast = (Math.max(foreground, background) + 0.05) / (Math.min(foreground, background) + 0.05);
  // Keep Octave RGB unchanged unless contrast is below 3:1 on the themed canvas; then use plot ink. PNGs are untouched.
  return contrast < 3 ? palette.text : `rgb(${rgb.map((v) => v * 255).join(",")})`;
}
function rawColor(value) {
  return `rgb(${value.map((v) => Math.max(0, Math.min(1, v)) * 255).join(",")})`;
}
const plotDash = (style) => ({ "--": [7, 4], ":": [2, 3], "-.": [7, 3, 2, 3] }[style] || []);
const sansFont = (size, bold = false, italic = false) => `${italic ? "italic " : ""}${bold ? "600 " : ""}${size}px -apple-system, sans-serif`;
const monoFont = (size) => `${size}px "JetBrains Mono", Menlo, "SF Mono", ui-monospace, monospace`;
function plotFormat(v) {
  if (!Number.isFinite(v)) return String(v);
  let a = Math.abs(v);
  return a && (a >= 1e5 || a < 1e-4) ? v.toExponential(4) : Number(v.toPrecision(6)).toString();
}
class InteractiveFigure {
  constructor(root, data) {
    this.root = root;
    this.data = data;
    this.initial = data.axes.map((a) => ({ x: [...a.xlim], y: [...a.ylim] }));
    this.views = this.initial.map((v) => ({ x: [...v.x], y: [...v.y] }));
    this.wrap = registry.el("div", "interactive-plot");
    this.canvas = registry.el("canvas");
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("role", "img");
    this.canvas.setAttribute("aria-label", t("Interactive plot. Zoom with the wheel or dragging, pan by dragging with Shift, and reset by double-clicking."));
    this.tip = registry.el("div", "plot-data-tip");
    this.tip.hidden = true;
    this.wrap.append(this.canvas, this.tip);
    root.append(this.wrap);
    this.unmountWindowControls = mountFigureWindowControls(this.wrap, () => this.reset());
    this.ctx = this.canvas.getContext("2d");
    this.drag = null;
    this.hits = [];
    this.faces = [];
    this.texts = [];
    this.imageCanvases = new WeakMap();
    // Read-only hook for the browser tests: what was drawn, and where.
    this.wrap.figureTest = {
      sourceSize: () => [...(this.data.source?.figure_size || [])],
      texts: () => this.texts.map((item) => ({ ...item })),
      faces: () => this.faces.length,
      axes: () => this.data.axes.map((axes) => axes.visible !== false),
      box: (axis) => this.box(axis),
      viewport: () => ({ ...this.figureViewport }),
      project: (axis, x, y) => ({ x: this.dataPixel(x, axis, "x"), y: this.dataPixel(y, axis, "y") }),
      pixel: (x, y) => {
        const d = devicePixelRatio || 1;
        return [...this.ctx.getImageData(Math.round(x * d), Math.round(y * d), 1, 1).data];
      }
    };
    this.bind();
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(this.wrap);
    this.resize();
  }
  destroy() {
    this.cancelDrag();
    this.observer?.disconnect();
    this.unmountWindowControls?.();
  }
  bind() {
    let c = this.canvas;
    c.onwheel = (e) => {
      e.preventDefault();
      let p = this.point(e), i = this.axisAt(p);
      if (i < 0) return;
      let a = this.data.axes[i], v = this.views[i], factor = e.deltaY > 0 ? 1.2 : 1 / 1.2, x = this.pixelData(p.x, i, "x"), y = this.pixelData(p.y, i, "y");
      v.x = this.zoomRange(v.x, x, factor, a.xscale);
      v.y = this.zoomRange(v.y, y, factor, a.yscale);
      this.draw();
    };
    c.onpointerdown = (e) => {
      if (e.button !== 0 && e.button !== 1) return;
      let p = this.point(e), axis = this.axisAt(p);
      if (axis < 0) return;
      c.setPointerCapture(e.pointerId);
      let pan = e.shiftKey || e.button === 1;
      this.drag = { pointerId: e.pointerId, axis, start: p, current: p, pan, view: { x: [...this.views[axis].x], y: [...this.views[axis].y] } };
      c.classList.toggle("panning", pan);
      this.tip.hidden = true;
    };
    c.onpointermove = (e) => {
      let p = this.point(e);
      if (this.drag) {
        this.drag.current = p;
        if (this.drag.pan) this.pan(p);
        this.draw();
        return;
      }
      this.showHit(p, false);
    };
    c.onpointerleave = () => {
      if (!this.drag && !this.pinned) this.tip.hidden = true;
    };
    c.onpointerup = (e) => {
      if (!this.drag) return;
      let d = this.drag, p = this.point(e);
      if (!d.pan && Math.abs(p.x - d.start.x) > 8 && Math.abs(p.y - d.start.y) > 8) {
        let v = this.views[d.axis];
        v.x = [this.pixelData(Math.min(p.x, d.start.x), d.axis, "x"), this.pixelData(Math.max(p.x, d.start.x), d.axis, "x")].sort((a, b) => a - b);
        v.y = [this.pixelData(Math.max(p.y, d.start.y), d.axis, "y"), this.pixelData(Math.min(p.y, d.start.y), d.axis, "y")].sort((a, b) => a - b);
      } else if (!d.pan) this.showHit(p, true);
      this.cancelDrag();
      this.draw();
    };
    const cancel = (e) => {
      if (this.drag?.pointerId !== e.pointerId) return;
      this.cancelDrag();
      this.draw();
    };
    c.onpointercancel = cancel;
    c.onlostpointercapture = cancel;
    c.ondblclick = (e) => {
      e.preventDefault();
      this.reset();
    };
    c.onkeydown = (e) => {
      if (["Home", "Enter", "r", "R"].includes(e.key)) {
        e.preventDefault();
        this.reset();
      }
    };
  }
  resize() {
    let r = this.wrap.getBoundingClientRect(), d = devicePixelRatio || 1, w = Math.max(1, Math.round(r.width)), h = Math.max(1, Math.round(r.height));
    this.width = w;
    this.height = h;
    this.figureViewport = viewports.fitFigureViewport(this.data.source?.figure_size, w, h);
    this.canvas.width = Math.round(w * d);
    this.canvas.height = Math.round(h * d);
    this.ctx.setTransform(d, 0, 0, d, 0, 0);
    this.draw();
  }
  cancelDrag() {
    const pointerId = this.drag?.pointerId;
    this.drag = null;
    this.canvas.classList.remove("panning");
    if (pointerId !== undefined && this.canvas.hasPointerCapture?.(pointerId)) this.canvas.releasePointerCapture(pointerId);
  }
  reset() {
    this.cancelDrag();
    this.pinnedHit = null;
    this.views = this.initial.map((v) => ({ x: [...v.x], y: [...v.y] }));
    this.pinned = false;
    this.tip.hidden = true;
    this.draw();
  }
  point(e) {
    let r = this.canvas.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }
  box(i) {
    const fitted = this.axesViewport(i);
    const box = { x: fitted.x, y: fitted.y, w: fitted.width, h: fitted.height };
    return this.data.axes[i].series.some((s) => s.kind === "image") ? images.aspectBox(box, this.data.axes[i], this.views[i]) : box;
  }
  axesViewport(i) {
    return viewports.fitFigureAxes(this.data.axes[i], this.figureViewport, this.data.figure_axes || this.data.axes);
  }

  axisAt(p) {
    for (let i = this.data.axes.length - 1; i >= 0; i--) {
      // An invisible axes (figure title, annotation overlay) is not a zoom target.
      if (this.data.axes[i].visible === false) continue;
      let b = this.box(i);
      if (p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h) return i;
    }
    return -1;
  }
  trans(v, scale) {
    v = plotNumber(v);
    return scale === "log" ? v > 0 ? Math.log10(v) : null : v;
  }
  inverse(v, scale) {
    return scale === "log" ? 10 ** v : v;
  }
  pixelData(pixel, i, which) {
    let a = this.data.axes[i], v = this.views[i], b = this.box(i), scale = a[which + "scale"], range = v[which], lo = this.trans(range[0], scale), hi = this.trans(range[1], scale), ratio = which === "x" ? (pixel - b.x) / b.w : 1 - (pixel - b.y) / b.h;
    if (a[which + "dir"] === "reverse") ratio = 1 - ratio;
    return this.inverse(lo + ratio * (hi - lo), scale);
  }
  dataPixel(value, i, which) {
    let a = this.data.axes[i], v = this.views[i], b = this.box(i), scale = a[which + "scale"], n = this.trans(value, scale), lo = this.trans(v[which][0], scale), hi = this.trans(v[which][1], scale);
    if (n === null || !Number.isFinite(n) || lo === hi) return null;
    let ratio = (n - lo) / (hi - lo);
    if (a[which + "dir"] === "reverse") ratio = 1 - ratio;
    return which === "x" ? b.x + ratio * b.w : b.y + (1 - ratio) * b.h;
  }
  zoomRange(range, center, factor, scale) {
    let lo = this.trans(range[0], scale), hi = this.trans(range[1], scale), c = this.trans(center, scale);
    return [this.inverse(c + (lo - c) * factor, scale), this.inverse(c + (hi - c) * factor, scale)];
  }
  pan(p) {
    let d = this.drag, a = this.data.axes[d.axis], b = this.box(d.axis), v = this.views[d.axis], x0 = this.trans(d.view.x[0], a.xscale), x1 = this.trans(d.view.x[1], a.xscale), y0 = this.trans(d.view.y[0], a.yscale), y1 = this.trans(d.view.y[1], a.yscale), dx = (p.x - d.start.x) / b.w * (x1 - x0) * (a.xdir === "reverse" ? 1 : -1), dy = (p.y - d.start.y) / b.h * (y1 - y0) * (a.ydir === "reverse" ? -1 : 1);
    v.x = [this.inverse(x0 + dx, a.xscale), this.inverse(x1 + dx, a.xscale)];
    v.y = [this.inverse(y0 + dy, a.yscale), this.inverse(y1 + dy, a.yscale)];
  }
  ticks(range, scale) {
    let lo = this.trans(range[0], scale), hi = this.trans(range[1], scale), out = [];
    for (let i = 0; i < 5; i++) out.push(this.inverse(lo + (hi - lo) * i / 4, scale));
    return out;
  }
  draw() {
    if (!this.width || !this.height) return;
    let c = this.ctx, p = this.palette = plotTheme();
    c.clearRect(0, 0, this.width, this.height);
    c.fillStyle = p.background;
    c.fillRect(0, 0, this.width, this.height);
    this.hits = [];
    this.faces = [];
    this.texts = [];
    this.data.axes.forEach((a, i) => this.drawAxis(a, i));
    if (this.drag && !this.drag.pan) {
      let d = this.drag;
      c.save();
      c.strokeStyle = p.selection;
      c.setLineDash([4, 3]);
      c.strokeRect(d.start.x, d.start.y, d.current.x - d.start.x, d.current.y - d.start.y);
      c.restore();
    }
    if (this.pinned && this.pinnedHit) this.positionTip(this.pinnedHit);
    this.canvas.dataset.view = JSON.stringify(this.views);
  }
  runFont(run, size, bold, italic, mono) {
    const scaled = run.script ? size * 0.7 : size;
    return mono ? monoFont(scaled) : sansFont(scaled, bold || run.bold, italic || run.italic);
  }
  measureRuns(runs, size, bold = false, italic = false, mono = false) {
    const c = this.ctx;
    let width = 0;
    for (const run of runs) {
      c.font = this.runFont(run, size, bold, italic, mono);
      width += c.measureText(run.text).width;
    }
    return width;
  }
  // Draws styled runs from the left edge x on the given baseline.
  drawRuns(runs, x, baseline, size, bold = false, italic = false, mono = false) {
    const c = this.ctx;
    c.textAlign = "left";
    for (const run of runs) {
      c.font = this.runFont(run, size, bold, italic, mono);
      c.fillText(run.text, x, baseline + (run.script > 0 ? -0.38 * size : run.script < 0 ? 0.18 * size : 0));
      x += c.measureText(run.text).width;
    }
  }
  label(text, interpreter, x, y, size, align, bold = false, mono = false) {
    const runs = shapes.parseTex(text, interpreter === "none" ? "none" : "tex");
    const width = this.measureRuns(runs, size, bold, false, mono);
    this.drawRuns(runs, align === "center" ? x - width / 2 : align === "right" ? x - width : x, y, size, bold, false, mono);
    return width;
  }
  axisTicks(a, i, which) {
    const range = this.views[i][which], ticks = shapes.axisTicks(a, which, range, this.initial[i]?.[which]);
    return ticks || this.ticks(range, a[which + "scale"]).map((value) => ({ value, label: null }));
  }
  drawAxis(a, i) {
    let c = this.ctx, p = this.palette, b = this.box(i), visible = a.visible !== false;
    const interpreters = a.interpreters || {};
    c.save();
    if (visible) {
      c.fillStyle = p.background;
      c.fillRect(b.x, b.y, b.w, b.h);
      c.strokeStyle = p.grid;
      c.lineWidth = 1;
      c.fillStyle = p.text;
      let edge = -Infinity;
      for (let tick of this.axisTicks(a, i, "x")) {
        let px = this.dataPixel(tick.value, i, "x");
        if (px === null) continue;
        if (a.grid.x) {
          c.beginPath();
          c.moveTo(px, b.y);
          c.lineTo(px, b.y + b.h);
          c.stroke();
        }
        // Skip a label that would run into the previous one.
        const text = tick.label ?? plotFormat(tick.value), width = this.measureRuns(shapes.parseTex(text, interpreters.ticks), 10, false, false, true);
        if (px - width / 2 < edge + 4) continue;
        this.label(text, interpreters.ticks, px, b.y + b.h + 13, 10, "center", false, true);
        edge = px + width / 2;
      }
      edge = null;
      for (let tick of this.axisTicks(a, i, "y")) {
        let py = this.dataPixel(tick.value, i, "y");
        if (py === null) continue;
        if (a.grid.y) {
          c.beginPath();
          c.moveTo(b.x, py);
          c.lineTo(b.x + b.w, py);
          c.stroke();
        }
        if (edge !== null && Math.abs(py - edge) < 11) continue;
        this.label(tick.label ?? plotFormat(tick.value), interpreters.ticks, b.x - 4, py + 3, 10, "right", false, true);
        edge = py;
      }
      c.strokeStyle = p.axis;
      c.strokeRect(b.x, b.y, b.w, b.h);
    }
    // Octave's order: children in stacking order, then text in non-data units.
    const late = (s) => s.kind === "text" && s.units !== "data";
    for (const pass of [false, true]) {
      a.series.forEach((s, j) => {
        if (late(s) !== pass) return;
        c.save();
        // Text is not clipped to the axes unless its own clipping is on.
        if (s.kind !== "text" || s.clipping) {
          c.beginPath();
          c.rect(b.x, b.y, b.w, b.h);
          c.clip();
        }
        c.lineWidth = 1;
        if (s.kind === "image") this.drawImage(s, a, i);
        else if (s.kind === "patch2d") this.drawPatch(s, i, j);
        else if (s.kind === "text") this.drawText(s, i, j);
        else this.drawSeries(s, a, i, j);
        c.restore();
      });
    }
    c.fillStyle = p.text;
    if (a.title) this.label(a.title, interpreters.title, b.x + b.w / 2, Math.max(12, b.y - 8), 12, "center", true);
    if (a.xlabel) this.label(a.xlabel, interpreters.xlabel, b.x + b.w / 2, Math.min(this.height - 2, b.y + b.h + 27), 11, "center");
    if (a.ylabel) {
      c.save();
      c.translate(Math.max(9, b.x - 34), b.y + b.h / 2);
      c.rotate(-Math.PI / 2);
      this.label(a.ylabel, interpreters.ylabel, 0, 0, 11, "center");
      c.restore();
    }
    this.drawLegend(a, i);
    if (this.data.version === 3) {
      const decode = (hex) => hex.slice(1).match(/../g).map((value) => parseInt(value, 16) / 255);
      drawColorbars(c, a, this.width, this.height, { background: decode(p.background), text: decode(p.text), axis: decode(p.axis) }, this.data.source?.figure_size);
    }
    c.restore();
  }
  // Filled faces keep their serialised colour; only the edge follows the
  // 3:1 contrast rule. The geometry is Octave's own patch, never recomputed.
  drawImage(s, axes, i) {
    this.imageCanvases ||= new WeakMap();
    let bitmap = this.imageCanvases.get(s);
    if (!bitmap) {
      bitmap = document.createElement("canvas");
      bitmap.width = s.shape[1];
      bitmap.height = s.shape[0];
      bitmap.getContext("2d").putImageData(new ImageData(images.rgba(s, axes), bitmap.width, bitmap.height), 0, 0);
      this.imageCanvases.set(s, bitmap);
    }
    const bx = images.bounds(s.x, s.shape[1]), by = images.bounds(s.y, s.shape[0]);
    const x0 = this.dataPixel(bx[0], i, "x"), x1 = this.dataPixel(bx[1], i, "x"), y0 = this.dataPixel(by[0], i, "y"), y1 = this.dataPixel(by[1], i, "y");
    const c = this.ctx;
    c.imageSmoothingEnabled = false;
    c.translate(x0, y0);
    c.scale(x1 >= x0 ? 1 : -1, y1 >= y0 ? 1 : -1);
    c.drawImage(bitmap, 0, 0, Math.abs(x1 - x0), Math.abs(y1 - y0));
  }
  drawPatch(s, i, j) {
    const c = this.ctx, face = s.face_color === "none" ? null : rawColor(s.face_color);
    const edge = s.edge_color === "none" || s.line_style === "none" ? null : plotColor(s.edge_color, this.palette);
    c.lineWidth = Math.max(1, s.line_width);
    c.lineJoin = "miter";
    for (const polygon of shapes.facePolygons(s)) {
      const points = [];
      for (const [x, y] of polygon.points) {
        const px = this.dataPixel(x, i, "x"), py = this.dataPixel(y, i, "y");
        if (px === null || py === null) break;
        points.push([px, py]);
      }
      if (points.length !== polygon.points.length) continue;
      c.beginPath();
      points.forEach(([px, py], n) => n ? c.lineTo(px, py) : c.moveTo(px, py));
      c.closePath();
      if (face && points.length > 2) {
        c.fillStyle = face;
        c.fill("evenodd");
      }
      if (edge) {
        c.strokeStyle = edge;
        c.setLineDash(plotDash(s.line_style));
        c.stroke();
        c.setLineDash([]);
      }
      const tip = s.role === "textbox" ? null : shapes.faceTip(s, polygon);
      // Every opaque polygon participates in occlusion, even when it has no
      // rectangular/bar tip. General shapes retain their original vertex tips.
      if (tip || face && points.length > 2) this.faces.push({ axis: i, seriesIndex: j, series: s, points, tip });
      if (s.role === "textbox") continue;
      if (!tip && !s.bar) polygon.points.forEach(([x, y], n) => this.hits.push({ x: points[n][0], y: points[n][1], vx: x, vy: y, series: s, axis: i, seriesIndex: j }));
    }
  }
  drawText(s, i, j) {
    const c = this.ctx, a = this.data.axes[i], b = this.box(i);
    let x, y;
    if (s.units === "data") {
      x = this.dataPixel(s.position[0], i, "x");
      y = this.dataPixel(s.position[1], i, "y");
      if (x === null || y === null) return;
      // Unclipped text follows its anchor: it is shown while the anchor is in
      // view, and always when it was placed outside the exported limits.
      const inside = x >= b.x - 0.5 && x <= b.x + b.w + 0.5 && y >= b.y - 0.5 && y <= b.y + b.h + 0.5;
      const within = (value, range) => value >= Math.min(...range) && value <= Math.max(...range);
      const margin = !within(s.position[0], this.initial[i].x) || !within(s.position[1], this.initial[i].y);
      if (!inside && !(margin && !s.clipping)) return;
    } else {
      x = b.x + s.position[0] * b.w;
      y = b.y + (1 - s.position[1]) * b.h;
      if (s.figure_title) y = Math.max(6, y);
    }
    const size = s.font_size, bold = s.font_weight === "bold", italic = s.font_angle === "italic";
    const lines = s.lines.map((line) => shapes.parseTex(line, s.interpreter));
    if (!lines.some((runs) => runs.length)) return;
    const layout = shapes.layoutText({ widths: lines.map((runs) => this.measureRuns(runs, size, bold, italic)), lineHeight: size * 1.2, ascent: size * 0.8, descent: size * 0.2, halign: s.horizontal_alignment, valign: s.vertical_alignment, margin: s.margin });
    c.save();
    c.translate(x, y);
    c.rotate(-s.rotation * Math.PI / 180);
    // An annotation textbox draws its background as the patch right before its text.
    const frame = s.role === "textbox" ? a.series[j - 1] : null;
    const boxed = s.background_color !== "none" || frame?.kind === "patch2d" && frame.role === "textbox" && frame.face_color !== "none";
    if (s.background_color !== "none") {
      c.fillStyle = rawColor(s.background_color);
      c.fillRect(layout.box.x, layout.box.y, layout.box.w, layout.box.h);
    }
    if (s.edge_color !== "none" && s.line_style !== "none") {
      c.strokeStyle = plotColor(s.edge_color, this.palette);
      c.lineWidth = Math.max(1, s.line_width);
      c.setLineDash(plotDash(s.line_style));
      c.strokeRect(layout.box.x, layout.box.y, layout.box.w, layout.box.h);
      c.setLineDash([]);
    }
    // On its own box the text keeps its colour; on the themed plot background
    // it follows the same 3:1 contrast rule as lines.
    c.fillStyle = boxed ? rawColor(s.color) : plotColor(s.color, this.palette);
    const color = c.fillStyle;
    lines.forEach((runs, n) => this.drawRuns(runs, layout.lines[n].x, layout.lines[n].baseline, size, bold, italic));
    c.restore();
    this.texts.push({ axis: i, id: j, text: s.lines.map((line) => shapes.plainText(line, s.interpreter)).join("\n"), x, y, rotation: s.rotation, box: { x: x + layout.box.x, y: y + layout.box.y, w: layout.box.w, h: layout.box.h }, boxed, color });
  }
  seriesStyle(s) {
    if (s.kind === "patch2d") return { line: null, edge: s.edge_color === "none" || s.line_style === "none" ? null : plotColor(s.edge_color, this.palette), face: s.face_color === "none" ? null : rawColor(s.face_color) };
    // Keep the exported source mode: an explicit RGB equal to Octave's axes
    // colour is not necessarily 'auto'. Only auto tracks the displayed canvas.
    return { line: s.kind !== "scatter" && s.line_style !== "none" && s.line_color !== "none" ? plotColor(s.line_color, this.palette) : null, edge: s.marker_edge_color === "none" ? null : plotColor(s.marker_edge_color, this.palette), face: s.marker_face_auto ? this.palette.background : s.marker_face_color === "none" ? null : plotColor(s.marker_face_color, this.palette) };
  }
  drawSeries(s, a, i, j) {
    const c = this.ctx, style = this.seriesStyle(s), points = [];
    for (let n = 0; n < Math.min(s.x.length, s.y.length); n++) {
      let x = this.dataPixel(s.x[n], i, "x"), y = this.dataPixel(s.y[n], i, "y");
      points.push(x === null || y === null ? null : { x, y, vx: s.x[n], vy: s.y[n], series: s });
    }
    c.lineWidth = 1.5;
    c.setLineDash(plotDash(s.line_style));
    if (style.line && s.span) {
      // A baseline or constant line crosses the whole current view.
      const b = this.box(i), across = s.span === "horizontal", at = across ? points[0]?.y : points[0]?.x;
      if (at !== undefined && at !== null) {
        c.strokeStyle = style.line;
        c.lineWidth = s.role === "bar" ? 1 : 1.5;
        c.beginPath();
        c.moveTo(across ? b.x : at, across ? at : b.y);
        c.lineTo(across ? b.x + b.w : at, across ? at : b.y + b.h);
        c.stroke();
      }
      c.setLineDash([]);
      return;
    }
    if (style.line) {
      c.strokeStyle = style.line;
      if (s.kind === "stem") {
        let base = this.dataPixel(s.base_value, i, "y");
        if (base !== null) {
          for (let p of points) if (p) {
            c.beginPath();
            c.moveTo(p.x, base);
            c.lineTo(p.x, p.y);
            c.stroke();
          }
        }
      } else {
        c.beginPath();
        let previous = null;
        for (let p of points) {
          if (!p) {
            previous = null;
            continue;
          }
          if (!previous) c.moveTo(p.x, p.y);
          else if (s.kind === "stairs") {
            c.lineTo(p.x, previous.y);
            c.lineTo(p.x, p.y);
          } else c.lineTo(p.x, p.y);
          previous = p;
        }
        c.stroke();
      }
    }
    c.setLineDash([]);
    for (let p of points) if (p) {
      const marked = this.marker(p.x, p.y, s.marker, style.edge, style.face);
      if (style.line || marked) this.hits.push({ ...p, axis: i, seriesIndex: j });
    }
  }
  marker(x, y, kind, edge, face) {
    if (!kind || kind === "none" || !edge && !face) return false;
    const c = this.ctx, r = kind === "." ? 2 : 3.5, open = ["+", "x", "*", "."].includes(kind);
    if (open && !edge) return false;
    c.save();
    c.setLineDash([]);
    c.beginPath();
    if (["+", "x", "*"].includes(kind)) {
      if (kind !== "x") {
        c.moveTo(x - r, y);
        c.lineTo(x + r, y);
        c.moveTo(x, y - r);
        c.lineTo(x, y + r);
      }
      if (kind !== "+") {
        c.moveTo(x - r, y - r);
        c.lineTo(x + r, y + r);
        c.moveTo(x - r, y + r);
        c.lineTo(x + r, y - r);
      }
    } else if (["s", "square"].includes(kind)) c.rect(x - r, y - r, r * 2, r * 2);
    else if (["^", "v", ">", "<"].includes(kind)) {
      let angle = { "^": -Math.PI / 2, "v": Math.PI / 2, ">": 0, "<": Math.PI }[kind];
      for (let k = 0; k < 3; k++) {
        let a = angle + k * Math.PI * 2 / 3, px = x + Math.cos(a) * r * 1.3, py = y + Math.sin(a) * r * 1.3;
        k ? c.lineTo(px, py) : c.moveTo(px, py);
      }
      c.closePath();
    } else c.arc(x, y, r, 0, Math.PI * 2);
    if (kind === ".") {
      c.fillStyle = edge;
      c.fill();
    } else {
      if (!open && face) {
        c.fillStyle = face;
        c.fill();
      }
      if (edge) {
        c.strokeStyle = edge;
        c.stroke();
      }
    }
    c.restore();
    return true;
  }
  drawLegend(a, i) {
    if (!a.legend?.visible) return;
    // Text and group baselines are never legend entries.
    const p = this.palette, labels = a.legend.labels || [], items = a.series.filter((s) => s.kind !== "text" && s.kind !== "image" && !(s.span && s.role === "bar")).map((s, j) => ({ label: s.display_name || labels[j], series: s, style: this.seriesStyle(s) })).filter((x2) => x2.label);
    if (!items.length) return;
    const c = this.ctx, b = this.box(i);
    c.font = sansFont(10);
    const width = Math.min(b.w * 0.55, Math.max(...items.map((x2) => c.measureText(shapes.plainText(x2.label)).width)) + 35), height = items.length * 16 + 8, x = b.x + b.w - width - 6, y = b.y + 6;
    c.save();
    c.fillStyle = p.background;
    c.fillRect(x, y, width, height);
    c.strokeStyle = p.axis;
    c.strokeRect(x, y, width, height);
    c.font = "10px -apple-system, sans-serif";
    c.textAlign = "left";
    items.forEach((item, n) => {
      let yy = y + 13 + n * 16;
      if (item.series.kind === "patch2d") {
        if (item.style.face) {
          c.fillStyle = item.style.face;
          c.fillRect(x + 6, yy - 8, 17, 9);
        }
        if (item.style.edge) {
          c.strokeStyle = item.style.edge;
          c.strokeRect(x + 6, yy - 8, 17, 9);
        }
        c.fillStyle = p.text;
        this.drawRuns(shapes.parseTex(item.label), x + 27, yy, 10);
        return;
      }
      if (item.style.line) {
        c.strokeStyle = item.style.line;
        c.setLineDash(plotDash(item.series.line_style));
        c.beginPath();
        c.moveTo(x + 6, yy - 3);
        c.lineTo(x + 23, yy - 3);
        c.stroke();
      }
      c.setLineDash([]);
      this.marker(x + 14.5, yy - 3, item.series.marker, item.style.edge, item.style.face);
      c.fillStyle = p.text;
      this.drawRuns(shapes.parseTex(item.label), x + 27, yy, 10);
    });
    c.restore();
  }
  positionTip(hit) {
    const x = hit.anchor ? this.dataPixel(hit.anchor.x, hit.axis, "x") : hit.x;
    const y = hit.anchor ? this.dataPixel(hit.anchor.y, hit.axis, "y") : hit.y;
    if (x === null || y === null) { this.tip.hidden = true; return; }
    this.tip.style.left = Math.max(2, Math.min(this.width - 190, x + 9)) + "px";
    this.tip.style.top = Math.max(2, Math.min(this.height - this.tip.offsetHeight - 2, y - 30)) + "px";
  }
  hitAt(p) {
    // Walk the same axes/series stacking order as the painter. An opaque image
    // or filled face masks samples below it, while later overlays stay pickable.
    const group = (items) => {
      const groups = new Map();
      for (const item of items) {
        const key = item.axis + ":" + item.seriesIndex;
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(item);
      }
      return groups;
    };
    const hitGroups = group(this.hits), faceGroups = group(this.faces);
    for (let axis = this.data.axes.length - 1; axis >= 0; axis--) {
      const axes = this.data.axes[axis], b = this.box(axis);
      if (axes.visible === false || p.x < b.x || p.x > b.x + b.w || p.y < b.y || p.y > b.y + b.h) continue;
      const covering = (j, point) => {
        const series = axes.series[j];
        if (series.kind === "image") return images.pick(series, this.pixelData(point.x, axis, "x"), this.pixelData(point.y, axis, "y"));
        if (series.kind === "patch2d" && series.face_color !== "none") return (faceGroups.get(axis + ":" + j) || []).some((face) => shapes.pointInPolygon(face.points, point.x, point.y));
        return false;
      };
      for (let j = axes.series.length - 1; j >= 0; j--) {
        let nearest = null, distance = 14;
        for (const hit of hitGroups.get(axis + ":" + j) || []) {
          if (hit.x < b.x || hit.x > b.x + b.w || hit.y < b.y || hit.y > b.y + b.h) continue;
          const d = Math.hypot(hit.x - p.x, hit.y - p.y);
          if (d >= distance) continue;
          let hidden = false;
          for (let above = j + 1; above < axes.series.length && !hidden; above++) hidden = !!covering(above, hit);
          if (!hidden) { nearest = hit; distance = d; }
        }
        if (nearest) return nearest;
        const series = axes.series[j];
        if (series.kind === "image") {
          const x = this.pixelData(p.x, axis, "x"), y = this.pixelData(p.y, axis, "y"), pixel = images.pick(series, x, y);
          if (pixel) return { x: p.x, y: p.y, vx: x, vy: y, axis, series, seriesIndex: j, pixel };
        }
        const faces = faceGroups.get(axis + ":" + j) || [];
        for (let n = faces.length - 1; n >= 0; n--) {
          const face = faces[n];
          if (series.face_color !== "none" && shapes.pointInPolygon(face.points, p.x, p.y)) {
            if (!face.tip) return null;
            return { ...face, x: p.x, y: p.y, vx: face.tip.x, vy: face.tip.y };
          }
        }
      }
      // This axes' painted background obscures earlier overlapping axes.
      return null;
    }
    return null;
  }
  showHit(p, pin) {
    const nearest = this.hitAt(p);
    if (!nearest) {
      if (!this.pinned) this.tip.hidden = true;
      return;
    }
    this.pinned = pin;
    this.pinnedHit = pin ? { ...nearest, anchor: { x: this.pixelData(nearest.x, nearest.axis, "x"), y: this.pixelData(nearest.y, nearest.axis, "y") } } : null;
    const value = (v) => Array.isArray(v) ? `[${plotFormat(Number(v[0]))}, ${plotFormat(Number(v[1]))}]` : plotFormat(Number(v));
    let label = shapes.plainText(nearest.series.display_name || this.data.axes[nearest.axis].legend?.labels?.[nearest.seriesIndex] || ""), name = label ? label + ": " : "";
    this.tip.textContent = nearest.pixel ? `${t("Row")} = ${nearest.pixel.row + 1}, ${t("Column")} = ${nearest.pixel.column + 1}, ${t("Value")} = ${Array.isArray(nearest.pixel.value) ? "[" + nearest.pixel.value.map(plotFormat).join(", ") + "]" : plotFormat(nearest.pixel.value)}` : `${name}x = ${value(nearest.vx)}, y = ${value(nearest.vy)}`;
    this.tip.hidden = false;
    this.positionTip(this.pinnedHit || nearest);
  }
}
function figureDetail(data, supported) {
  if (data?.decimated) {
    if (data.version === 3) {
      const series = data.axes.flatMap((axes) => axes.series);
      return " · " + figureReductionText(series.reduce((sum, item) => sum + item.original_points, 0), series.reduce((sum, item) => sum + item.rendered_points, 0));
    }
    return " " + t("· Interactive data limited to 2000 points; endpoints were preserved.");
  }
  if (!supported && !data?.loading) {
    const reason = data?.reason_code ? figureReasonText(data.reason_code, data.reason_args) : data?.reason || shared.figures[shared.figureIndex]?.fallback_reason || t("Only the PNG view is available.");
    return ` · ${reason}`;
  }
  return "";
}
function supportedData(data) {
  return !!data?.supported && [2, 3].includes(data.version);
}
function paint2D(ctx, axes, width, height, figureSize, figureAxes) {
  const plot = Object.create(InteractiveFigure.prototype);
  plot.ctx = ctx;
  plot.width = width;
  plot.height = height;
  plot.palette = plotTheme();
  plot.hits = [];
  plot.faces = [];
  plot.texts = [];
  plot.data = { version: 3, axes: [axes], figure_axes: figureAxes, source: { figure_size: figureSize } };
  plot.figureViewport = viewports.fitFigureViewport(figureSize, width, height);
  plot.views = [{ x: [...axes.xlim], y: [...axes.ylim] }];
  plot.initial = [{ x: [...axes.xlim], y: [...axes.ylim] }];
  plot.drawAxis(axes, 0);
}
function renderFigures() {
  if (manifestPending) return;
  closeEnlarged(false);
  shared.interactivePlot?.destroy();
  shared.interactivePlot = null;
  unmountFigureTools();
  loadActiveData();
  const list = registry.$("#figure-tabs");
  list.replaceChildren();
  shared.figures.forEach((figure, i) => {
    const button = registry.el("button", i === shared.figureIndex ? "active" : "", figure.name);
    button.onclick = () => {
      shared.figureIndex = i;
      registry.persistActiveTab("figureName", figure.name);
      renderFigures();
    };
    list.append(button);
  });
  registry.$("#figure-count").textContent = shared.figures.length;
  const area = registry.$("#plot-area");
  area.classList.remove("zoomed");
  area.replaceChildren();
  const index = shared.figureIndex;
  let data = shared.figureData[index];
  let supported = supportedData(data);
  let showInteractive = !!shared.figures.length && supported && shared.plotMode === "interactive";
  const is3D = showInteractive && data.version === 3 && data.axes.some((axes) => axes.dimension === 3);
  const fallback = (code) => {
    shared.figureData[index] = { supported: false, reason_code: code };
    renderFigures();
  };
  if (shared.figures.length) {
    if (is3D) {
      try {
        const key = stateKey(index);
        shared.interactivePlot = new Figure3D(area, data, { identity: identityAt(index), saved: cameraStates.get(key), save: (state) => cameraStates.set(key, state), failed: fallback, paint2D });
      } catch (error) {
        data = shared.figureData[index] = { supported: false, reason_code: error.reason_code || "shader_failure" };
        supported = false;
        showInteractive = false;
      }
    } else if (showInteractive) shared.interactivePlot = new InteractiveFigure(area, data);
    if (!showInteractive) {
      const img = registry.el("img");
      img.src = shared.figureURLs[index];
      img.alt = shared.figures[index].name;
      img.onclick = () => area.classList.toggle("zoomed");
      area.append(img);
    }
    registry.$("#figure-caption").textContent = shared.figures[index].name + figureDetail(data, supported);
  } else {
    area.innerHTML = emptyPlot.value;
    registry.$("#figure-caption").textContent = "";
  }
  registry.$("#plot-interactive").disabled = !shared.figures.length || !supported;
  registry.$("#plot-interactive").classList.toggle("active", showInteractive);
  registry.$("#plot-png").disabled = !shared.figures.length;
  registry.$("#plot-png").classList.toggle("active", !!shared.figures.length && !showInteractive);
  registry.$("#plot-download").disabled = !shared.figures.length;
  registry.$("#plot-expand").disabled = !shared.figures.length;
  registry.$("#plot-fit").disabled = !shared.figures.length;
  registry.$("#rotate-left").disabled = registry.$("#rotate-right").disabled = !shared.figures.length;
}
function resetFigure() {
  if (shared.interactivePlot instanceof Figure3D) shared.interactivePlot.tools.reset();
  else if (shared.interactivePlot) shared.interactivePlot.reset();
  else registry.$("#plot-area").classList.remove("zoomed");
}
function closeEnlarged(redraw = true) {
  if (!enlarged) return;
  const current = enlarged;
  enlarged = null;
  current.observer.disconnect();
  registry.$("#modal").classList.remove("figure-expanded-dialog");
  registry.$("#modal").style.width = "";
  registry.$("#modal").style.height = "";
  registry.$("#modal").removeEventListener("close", current.close);
  if (current.controls) registry.$("#plot-area").before(current.controls);
  // A 3D camera is kept by its identity-scoped state; the 2D viewer's limits
  // are carried over to the viewer rebuilt in the panel for the same data.
  const plot = shared.interactivePlot;
  const kept = plot instanceof InteractiveFigure ? { data: plot.data, views: plot.views } : null;
  plot?.destroy();
  shared.interactivePlot = null;
  if (current.host.isConnected && registry.$("#modal").open) registry.$("#modal").close();
  current.host.remove();
  if (!redraw) return;
  renderFigures();
  const restored = shared.interactivePlot;
  if (kept && restored instanceof InteractiveFigure && restored.data === kept.data) {
    restored.views = kept.views.map((view) => ({ x: [...view.x], y: [...view.y] }));
    restored.draw();
  }
}
function expandFigure() {
  if (!shared.figures.length) return;
  const viewer = shared.interactivePlot;
  if (!viewer) {
    // PNG mode, or no interactive data for this figure.
    const img = registry.el("img");
    img.src = shared.figureURLs[shared.figureIndex];
    img.alt = shared.figures[shared.figureIndex].name;
    registry.modal(shared.figures[shared.figureIndex].name, img);
    return;
  }
  // The live viewer itself moves into the modal (2D and 3D): same state, no second canvas.
  const is3D = viewer instanceof Figure3D;
  const host = registry.el("div", "figure-expanded");
  const controls = is3D ? registry.$("#figure-tools") : null;
  const stage = registry.el("div", "figure-expanded-stage");
  stage.append(viewer.wrap);
  if (controls) host.append(controls);
  host.append(stage);
  const close = () => closeEnlarged();
  const observer = new MutationObserver(() => {
    if (!host.isConnected) closeEnlarged();
  });
  enlarged = { host, controls, close, observer };
  registry.$("#modal").classList.add("figure-expanded-dialog");
  registry.modal(shared.figures[shared.figureIndex].name, host);
  registry.$("#modal").addEventListener("close", close);
  observer.observe(registry.$("#modal-body"), { childList: true });
  viewer.resize();
}
async function rotate(angle) {
  if (shared.interactivePlot instanceof Figure3D) {
    shared.interactivePlot.rotateAzimuth(angle);
    return;
  }
  registry.requireIdle();
  if (!shared.figures.length) return;
  let result = await registry.api("execute", { mode: "rotate", argument: JSON.stringify({ figure: shared.figures[shared.figureIndex].number, angle }) });
  shared.lastJob = result.job;
  shared.activeOutput = null;
  shared.busy = true;
  registry.setStatus({ status: "running" });
}

registry.captureEmptyPlot = () => {
  emptyPlot.value = registry.$("#plot-area").innerHTML;
};
Object.assign(registry, { updateFigures, emptyPlot, plotNumber, cssColor, plotTheme, plotLuminance, plotColor, plotFormat, InteractiveFigure, figureDetail, renderFigures, rotate, resetFigure, expandFigure });

onLanguageChange(() => {
  if (shared.interactivePlot instanceof Figure3D) shared.interactivePlot.requestRender();
  else {
    const canvas = registry.$("#plot-area canvas");
    if (canvas) canvas.setAttribute("aria-label", t("Interactive plot. Zoom with the wheel or dragging, pan by dragging with Shift, and reset by double-clicking."));
    shared.interactivePlot?.draw();
  }
  const figure = shared.figures[shared.figureIndex];
  if (!figure) return;
  const data = shared.figureData[shared.figureIndex];
  registry.$("#figure-caption").textContent = figure.name + figureDetail(data, supportedData(data));
});
