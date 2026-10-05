import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";

async function updateFigures(s) {
  const generation = shared.uiGeneration, key = s.epoch + ":" + JSON.stringify(s.figures);
  if (key === shared.lastFiguresKey) return;
  shared.lastFiguresKey = key;
  shared.figures = s.figures || [];
  let urls = await Promise.all(shared.figures.map((f) => registry.blobAPI(`figure?job=${f.job || s.job}&file=${encodeURIComponent(f.file)}`))), data = await Promise.all(shared.figures.map((f) => f.data_file ? registry.api(`figure?job=${f.job || s.job}&file=${encodeURIComponent(f.data_file)}`).catch(() => ({ supported: false, reasonSource: "Interactive data could not be read." })) : Promise.resolve({ supported: false, reason: f.fallback_reason, reasonSource: f.fallback_reason ? null : "Interactive data is unavailable." })));
  if (generation !== shared.uiGeneration) {
    urls.forEach(URL.revokeObjectURL);
    return;
  }
  shared.figureURLs.forEach(URL.revokeObjectURL);
  shared.figureURLs = urls;
  shared.figureData = data;
  let persistedFigure = shared.settings?.activeTabs?.figureName;
  if (persistedFigure && shared.figures.some((figure) => figure.name === persistedFigure)) shared.figureIndex = shared.figures.findIndex((figure) => figure.name === persistedFigure);
  shared.figureIndex = Math.min(shared.figureIndex, Math.max(0, shared.figures.length - 1));
  renderFigures();
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
    this.ctx = this.canvas.getContext("2d");
    this.drag = null;
    this.hits = [];
    this.bind();
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(this.wrap);
    this.resize();
  }
  destroy() {
    this.observer?.disconnect();
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
      this.drag = { axis, start: p, current: p, pan, view: { x: [...this.views[axis].x], y: [...this.views[axis].y] } };
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
      this.drag = null;
      c.classList.remove("panning");
      this.draw();
    };
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
    this.canvas.width = Math.round(w * d);
    this.canvas.height = Math.round(h * d);
    this.ctx.setTransform(d, 0, 0, d, 0, 0);
    this.draw();
  }
  reset() {
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
    let p = this.data.axes[i].position;
    return { x: p[0] * this.width, y: (1 - p[1] - p[3]) * this.height, w: p[2] * this.width, h: p[3] * this.height };
  }
  axisAt(p) {
    for (let i = this.data.axes.length - 1; i >= 0; i--) {
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
    this.data.axes.forEach((a, i) => this.drawAxis(a, i));
    if (this.drag && !this.drag.pan) {
      let d = this.drag;
      c.save();
      c.strokeStyle = p.selection;
      c.setLineDash([4, 3]);
      c.strokeRect(d.start.x, d.start.y, d.current.x - d.start.x, d.current.y - d.start.y);
      c.restore();
    }
    this.canvas.dataset.view = JSON.stringify(this.views);
  }
  drawAxis(a, i) {
    let c = this.ctx, p = this.palette, b = this.box(i), v = this.views[i], xt = this.ticks(v.x, a.xscale), yt = this.ticks(v.y, a.yscale);
    c.save();
    c.fillStyle = p.background;
    c.fillRect(b.x, b.y, b.w, b.h);
    c.strokeStyle = p.grid;
    c.lineWidth = 1;
    c.font = '10px "JetBrains Mono", Menlo, "SF Mono", ui-monospace, monospace';
    c.fillStyle = p.text;
    for (let x of xt) {
      let px = this.dataPixel(x, i, "x");
      if (a.grid.x) {
        c.beginPath();
        c.moveTo(px, b.y);
        c.lineTo(px, b.y + b.h);
        c.stroke();
      }
      c.textAlign = "center";
      c.fillText(plotFormat(x), px, b.y + b.h + 13);
    }
    for (let y of yt) {
      let py = this.dataPixel(y, i, "y");
      if (a.grid.y) {
        c.beginPath();
        c.moveTo(b.x, py);
        c.lineTo(b.x + b.w, py);
        c.stroke();
      }
      c.textAlign = "right";
      c.fillText(plotFormat(y), b.x - 4, py + 3);
    }
    c.strokeStyle = p.axis;
    c.strokeRect(b.x, b.y, b.w, b.h);
    c.save();
    c.beginPath();
    c.rect(b.x, b.y, b.w, b.h);
    c.clip();
    a.series.forEach((s, j) => this.drawSeries(s, a, i, j));
    c.restore();
    c.fillStyle = p.text;
    c.textAlign = "center";
    c.font = "11px -apple-system, sans-serif";
    if (a.title) {
      c.font = "600 12px -apple-system, sans-serif";
      c.fillText(a.title, b.x + b.w / 2, Math.max(12, b.y - 8));
    }
    c.font = "11px -apple-system, sans-serif";
    if (a.xlabel) c.fillText(a.xlabel, b.x + b.w / 2, Math.min(this.height - 2, b.y + b.h + 27));
    if (a.ylabel) {
      c.save();
      c.translate(Math.max(9, b.x - 34), b.y + b.h / 2);
      c.rotate(-Math.PI / 2);
      c.fillText(a.ylabel, 0, 0);
      c.restore();
    }
    this.drawLegend(a, i);
    c.restore();
  }
  seriesStyle(s) {
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
    c.setLineDash({ "--": [7, 4], ":": [2, 3], "-.": [7, 3, 2, 3] }[s.line_style] || []);
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
    const p = this.palette, labels = a.legend.labels || [], items = a.series.map((s, j) => ({ label: s.display_name || labels[j], series: s, style: this.seriesStyle(s) })).filter((x2) => x2.label);
    if (!items.length) return;
    const c = this.ctx, b = this.box(i), width = Math.min(b.w * 0.55, Math.max(...items.map((x2) => c.measureText(x2.label).width)) + 35), height = items.length * 16 + 8, x = b.x + b.w - width - 6, y = b.y + 6;
    c.save();
    c.fillStyle = p.background;
    c.fillRect(x, y, width, height);
    c.strokeStyle = p.axis;
    c.strokeRect(x, y, width, height);
    c.font = "10px -apple-system, sans-serif";
    c.textAlign = "left";
    items.forEach((item, n) => {
      let yy = y + 13 + n * 16;
      if (item.style.line) {
        c.strokeStyle = item.style.line;
        c.setLineDash({ "--": [7, 4], ":": [2, 3], "-.": [7, 3, 2, 3] }[item.series.line_style] || []);
        c.beginPath();
        c.moveTo(x + 6, yy - 3);
        c.lineTo(x + 23, yy - 3);
        c.stroke();
      }
      c.setLineDash([]);
      this.marker(x + 14.5, yy - 3, item.series.marker, item.style.edge, item.style.face);
      c.fillStyle = p.text;
      c.fillText(item.label, x + 27, yy);
    });
    c.restore();
  }
  showHit(p, pin) {
    let nearest = null, distance = 14;
    for (let h of this.hits) {
      let d = Math.hypot(h.x - p.x, h.y - p.y);
      if (d < distance) {
        nearest = h;
        distance = d;
      }
    }
    if (!nearest) {
      if (!this.pinned) this.tip.hidden = true;
      return;
    }
    this.pinned = pin;
    let label = nearest.series.display_name || this.data.axes[nearest.axis].legend?.labels?.[nearest.seriesIndex] || "", name = label ? label + ": " : "";
    this.tip.textContent = `${name}x = ${plotFormat(Number(nearest.vx))}, y = ${plotFormat(Number(nearest.vy))}`;
    this.tip.hidden = false;
    this.tip.style.left = Math.min(this.width - 190, nearest.x + 9) + "px";
    this.tip.style.top = Math.max(2, nearest.y - 30) + "px";
  }
}
function figureDetail(data, supported) {
  if (data?.decimated) return " " + t("· Interactive data limited to 2000 points; endpoints were preserved.");
  if (!supported) {
    const reason = data?.reason || shared.figures[shared.figureIndex]?.fallback_reason || (data?.reasonSource ? t(data.reasonSource) : t("Only the PNG view is available."));
    return ` · ${reason}`;
  }
  return "";
}
function renderFigures() {
  shared.interactivePlot?.destroy();
  shared.interactivePlot = null;
  let list = registry.$("#figure-tabs");
  list.replaceChildren();
  shared.figures.forEach((f, i) => {
    let b = registry.el("button", i === shared.figureIndex ? "active" : "", f.name);
    b.onclick = () => {
      shared.figureIndex = i;
      registry.persistActiveTab("figureName", f.name);
      renderFigures();
    };
    list.append(b);
  });
  registry.$("#figure-count").textContent = shared.figures.length;
  let area = registry.$("#plot-area");
  area.classList.remove("zoomed");
  area.replaceChildren();
  let data = shared.figureData[shared.figureIndex], supported = !!data?.supported && data.version === 2, showInteractive = !!shared.figures.length && supported && shared.plotMode === "interactive";
  if (shared.figures.length) {
    if (showInteractive) shared.interactivePlot = new InteractiveFigure(area, data);
    else {
      let img = registry.el("img");
      img.src = shared.figureURLs[shared.figureIndex];
      img.alt = shared.figures[shared.figureIndex].name;
      img.onclick = () => area.classList.toggle("zoomed");
      area.append(img);
    }
    registry.$("#figure-caption").textContent = shared.figures[shared.figureIndex].name + figureDetail(data, supported);
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
  registry.$("#rotate-left").disabled = registry.$("#rotate-right").disabled = !shared.figures.length || showInteractive || supported;
}
async function rotate(angle) {
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
Object.assign(registry, { updateFigures, emptyPlot, plotNumber, cssColor, plotTheme, plotLuminance, plotColor, plotFormat, InteractiveFigure, figureDetail, renderFigures, rotate });

onLanguageChange(() => {
  const canvas = registry.$("#plot-area canvas");
  if (canvas) canvas.setAttribute("aria-label", t("Interactive plot. Zoom with the wheel or dragging, pan by dragging with Shift, and reset by double-clicking."));
  const figure = shared.figures[shared.figureIndex];
  if (!figure) return;
  const data = shared.figureData[shared.figureIndex], supported = !!data?.supported && data.version === 2;
  registry.$("#figure-caption").textContent = figure.name + figureDetail(data, supported);
});
