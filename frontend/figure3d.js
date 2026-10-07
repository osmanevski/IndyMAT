import cameraMath from "./figure_camera_utils.cjs";
import geometry from "./figure_geometry_utils.cjs";
import colorMath from "./figure_color_utils.cjs";
import picking from "./figure_pick_utils.cjs";
import { FigureWebGL } from "./figure_webgl.js";
import { figureCanvasDescription, mountFigureTools, unmountFigureTools } from "./figure_tools.js";
import { onLanguageChange } from "./i18n.js";

const rgbText = (rgb) => `rgb(${rgb.map((v) => Math.round(v * 255)).join(",")})`;
function theme() {
  const style = getComputedStyle(document.body);
  const read = (name) => style.getPropertyValue(name).trim();
  const parse = (name) => read(name).slice(1).match(/../g).map((value) => parseInt(value, 16) / 255);
  return { background: parse("--plot-bg"), text: parse("--plot-text"), axis: parse("--plot-axis"), grid: parse("--plot-grid") };
}
function viewport(position, width, height) {
  return { x: position[0] * width, y: (1 - position[1] - position[3]) * height, width: position[2] * width, height: position[3] * height };
}

// Shared with the v3 top-view 2D path; swatches encode values, never contrast.
export function drawColorbars(ctx, axes, width, height, palette) {
  ctx.save();
  ctx.font = '10px "JetBrains Mono", Menlo, monospace';
  for (const bar of axes.colorbars || []) {
    const box = viewport(bar.position, width, height);
    const vertical = bar.orientation === "vertical";
    const stops = colorMath.colorbarGradient(bar);
    for (let i = 0; i < stops.length; i += 2) {
      const low = stops[i].offset;
      const high = stops[i + 1].offset;
      ctx.fillStyle = rgbText(stops[i].rgb);
      if (vertical) ctx.fillRect(box.x, box.y + (1 - high) * box.height, box.width, (high - low) * box.height + 0.5);
      else ctx.fillRect(box.x + low * box.width, box.y, (high - low) * box.width + 0.5, box.height);
    }
    ctx.strokeStyle = rgbText(colorMath.adjustForContrast(palette.axis, palette.background, palette.text));
    ctx.strokeRect(box.x, box.y, box.width, box.height);
    ctx.fillStyle = rgbText(palette.text);
    const opposite = bar.axis_location === "left" || bar.axis_location === "top";
    for (let i = 0; i < bar.ticks.length; i++) {
      let fraction = (bar.ticks[i] - bar.limits[0]) / (bar.limits[1] - bar.limits[0]);
      if (fraction < 0 || fraction > 1) continue;
      if (bar.direction === "reverse") fraction = 1 - fraction;
      const label = bar.tick_labels[i] ?? String(bar.ticks[i]);
      ctx.beginPath();
      if (vertical) {
        const x = opposite ? box.x : box.x + box.width;
        const y = box.y + (1 - fraction) * box.height;
        ctx.moveTo(x, y);
        ctx.lineTo(x + (opposite ? -4 : 4), y);
        ctx.textAlign = opposite ? "right" : "left";
        ctx.fillText(label, x + (opposite ? -6 : 6), y + 3);
      } else {
        const x = box.x + fraction * box.width;
        const y = opposite ? box.y : box.y + box.height;
        ctx.moveTo(x, y);
        ctx.lineTo(x, y + (opposite ? -4 : 4));
        ctx.textAlign = "center";
        ctx.fillText(label, x, y + (opposite ? -7 : 14));
      }
      ctx.stroke();
    }
    ctx.save();
    ctx.textAlign = "center";
    if (vertical) {
      ctx.translate(opposite ? box.x - 42 : box.x + box.width + 42, box.y + box.height / 2);
      ctx.rotate(-Math.PI / 2);
      ctx.fillText(bar.label, 0, 0);
    } else ctx.fillText(bar.label, box.x + box.width / 2, opposite ? box.y - 24 : box.y + box.height + 28);
    ctx.restore();
  }
  ctx.restore();
}

export class Figure3D {
  constructor(root, data, { identity, saved, save, failed, paint2D }) {
    this.data = data;
    this.identity = identity;
    this.saveState = save;
    this.paint2D = paint2D;
    this.disposed = false;
    this.root = root;
    this.wrap = document.createElement("div");
    this.wrap.className = "interactive-plot figure-3d";
    this.canvas = document.createElement("canvas");
    this.canvas.className = "figure-webgl";
    this.overlay = document.createElement("canvas");
    this.overlay.className = "figure-labels";
    this.overlay.setAttribute("aria-hidden", "true");
    this.canvas.setAttribute("role", "img");
    this.canvas.setAttribute("aria-label", figureCanvasDescription());
    this.wrap.append(this.canvas, this.overlay);
    root.append(this.wrap);
    this.ctx = this.overlay.getContext("2d");
    this.scenes = data.axes.map((axes) => geometry.buildScene(axes.dimension === 3 ? axes : undefined));
    this.cameras = data.axes.map((axes) => cameraMath.createCamera(axes));
    this.cameras.forEach((camera, i) => cameraMath.restore(camera, saved?.[i], identity));
    this.axis = Math.max(0, data.axes.findIndex((axes) => axes.dimension === 3));
    try {
      this.renderer = new FigureWebGL(this.canvas, this.scenes, failed);
    } catch (error) {
      this.wrap.remove();
      throw error;
    }
    this.tools = mountFigureTools(this.wrap, this);
    this.pointer = (event) => {
      const bounds = this.wrap.getBoundingClientRect();
      this.axisAt(event.clientX - bounds.left, event.clientY - bounds.top);
    };
    this.wrap.addEventListener("pointerdown", this.pointer, true);
    this.wrap.addEventListener("wheel", this.pointer, { capture: true, passive: true });
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(this.wrap);
    this.windowResize = () => this.resize();
    window.addEventListener("resize", this.windowResize);
    this.dprQuery = null;
    this.watchDpr();
    this.unsubscribe = onLanguageChange(() => {
      this.canvas.setAttribute("aria-label", figureCanvasDescription());
      this.requestRender();
    });
    // Deliberately small, local test hook. No numerical result or session access.
    this.wrap.figureTest = {
      camera: () => this.states(),
      pixels: (points) => {
        this.render();
        return points.map(({ x, y }) => this.renderer.readPixels(x, y, this.dpr));
      },
      project: (axis, point) => cameraMath.project(this.cameras[axis], this.frames()[axis].viewport, cameraMath.dataToNormalised(this.data.axes[axis], point)),
      loseContext: () => {
        const extension = this.renderer.gl.getExtension("WEBGL_lose_context");
        if (!extension) return false;
        extension.loseContext();
        return true;
      }
    };
    this.resize();
  }
  watchDpr() {
    this.dprQuery?.removeEventListener("change", this.windowResize);
    this.dprQuery = matchMedia(`(resolution: ${devicePixelRatio || 1}dppx)`);
    this.dprQuery.addEventListener("change", this.windowResize, { once: true });
  }
  resize() {
    if (this.disposed) return;
    const bounds = this.wrap.getBoundingClientRect();
    this.width = Math.max(1, bounds.width);
    this.height = Math.max(1, bounds.height);
    this.dpr = devicePixelRatio || 1;
    for (const canvas of [this.canvas, this.overlay]) {
      canvas.width = Math.round(this.width * this.dpr);
      canvas.height = Math.round(this.height * this.dpr);
    }
    this.watchDpr();
    this.requestRender();
  }
  frames() {
    return this.data.axes.map((axes, i) => ({ scene: this.scenes[i], camera: this.cameras[i], viewport: viewport(axes.position, this.width, this.height) }));
  }
  axisAt(x, y) {
    const frames = this.frames();
    for (let i = frames.length - 1; i >= 0; i--) {
      const vp = frames[i].viewport;
      if (this.data.axes[i].dimension === 3 && x >= vp.x && x <= vp.x + vp.width && y >= vp.y && y <= vp.y + vp.height) {
        this.axis = i;
        return i;
      }
    }
    return -1;
  }
  states() {
    return this.cameras.map((camera) => cameraMath.serialize(camera, this.identity));
  }
  orbit(dx, dy) {
    cameraMath.orbit(this.cameras[this.axis], dx, dy);
  }
  rotateAzimuth(degrees) {
    cameraMath.rotateAzimuth(this.cameras[this.axis], degrees);
    this.requestRender();
  }
  pan(dx, dy) {
    cameraMath.pan(this.cameras[this.axis], dx, dy, this.frames()[this.axis].viewport);
  }
  zoom(factor, anchor) {
    const vp = this.frames()[this.axis].viewport;
    cameraMath.matrices(this.cameras[this.axis], vp.width, vp.height);
    cameraMath.zoom(this.cameras[this.axis], factor, anchor ? [(anchor.x - vp.x) * 2 / vp.width - 1, 1 - (anchor.y - vp.y) * 2 / vp.height] : undefined);
  }
  reset() {
    this.cameras.forEach((camera) => cameraMath.reset(camera));
    this.requestRender();
  }
  pick(x, y) {
    const axis = this.axisAt(x, y);
    if (axis < 0) return null;
    const hit = picking.pick(this.scenes[axis], this.cameras[axis], this.frames()[axis].viewport, x, y);
    if (!hit) return null;
    const series = this.data.axes[axis].series[hit.series];
    return { ...hit.data, seriesName: series.display_name || this.data.axes[axis].legend.labels[hit.series] || "", ...(hit.kind === "surface" ? { row: hit.row, column: hit.column } : { index: hit.index }), ...(hit.colorValue === undefined ? {} : { colorValue: hit.colorValue }) };
  }
  requestRender() {
    if (this.disposed) return;
    this.saveState(this.states());
    this.renderer.requestRender(() => this.render());
  }
  draw() {
    this.requestRender();
  }
  render() {
    if (this.disposed || !this.width) return;
    if (this.dpr !== (devicePixelRatio || 1)) {
      this.resize();
      return;
    }
    const palette = theme();
    const frames = this.frames();
    this.renderer.render(frames, palette, this.dpr);
    const ctx = this.ctx;
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, this.width, this.height);
    this.data.axes.forEach((axes, i) => {
      if (axes.dimension === 3) this.drawAxes(axes, frames[i], palette);
      else this.paint2D?.(ctx, axes, this.width, this.height);
      drawColorbars(ctx, axes, this.width, this.height, palette);
    });
  }
  drawAxes(axes, { camera, viewport: vp }, palette) {
    const ctx = this.ctx;
    const box = cameraMath.axisBox(camera, axes);
    const project = (point) => cameraMath.project(camera, vp, point);
    const stroke = (a, b) => {
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
    };
    // The box outline and grid are drawn depth-tested by the renderer; this
    // overlay only carries tick marks and text.
    ctx.save();
    ctx.lineWidth = 1;
    ctx.strokeStyle = rgbText(palette.axis);
    ctx.fillStyle = rgbText(palette.text);
    const tickFont = '10px "JetBrains Mono", Menlo, monospace';
    const center = project([0, 0, 0]);
    for (const [key, axis] of Object.entries(box.axes)) {
      const from = project(axis.edge.start);
      const to = project(axis.edge.end);
      // An axis seen end-on (Z in a top view) has no room for ticks or a label.
      if (Math.hypot(to.x - from.x, to.y - from.y) < 24) continue;
      const anchor = project(axis.labelAnchor);
      const dx = anchor.x - center.x;
      const dy = anchor.y - center.y;
      const distance = Math.hypot(dx, dy) || 1;
      const ox = dx / distance;
      const oy = dy / distance;
      ctx.font = tickFont;
      ctx.textBaseline = "middle";
      ctx.textAlign = ox > 0.35 ? "left" : ox < -0.35 ? "right" : "center";
      let widest = 0;
      let last = null;
      for (const tick of axis.ticks) {
        const p = project(tick.position);
        stroke(p, { x: p.x + ox * 4, y: p.y + oy * 4 });
        const width = ctx.measureText(tick.label).width;
        const x = p.x + ox * 8;
        const y = p.y + oy * 12;
        const left = ctx.textAlign === "left" ? x : ctx.textAlign === "right" ? x - width : x - width / 2;
        // Every tick keeps its mark; a label that would run into the previous one is left out.
        if (last && left < last.right + 3 && left + width > last.left - 3 && Math.abs(y - last.y) < 11) continue;
        ctx.fillText(tick.label, x, y);
        last = { left, right: left + width, y };
        widest = Math.max(widest, width);
      }
      if (axis.label) {
        const reach = 12 + Math.abs(ox) * (widest + 6) + Math.abs(oy) * 16;
        const middle = project(axis.edge.start.map((v, j) => (v + axis.edge.end[j]) / 2));
        ctx.font = "11px -apple-system, sans-serif";
        ctx.textAlign = "center";
        ctx.save();
        ctx.translate(middle.x + ox * reach, middle.y + oy * reach);
        if (key === "z" && Math.abs(ox) > 0.5) ctx.rotate(-Math.PI / 2);
        else if (Math.abs(ox) > 0.35) ctx.textAlign = ox > 0 ? "left" : "right";
        ctx.fillText(axis.label, 0, 0);
        ctx.restore();
      }
    }
    ctx.textAlign = "center";
    ctx.textBaseline = "alphabetic";
    ctx.font = "600 12px -apple-system, sans-serif";
    ctx.fillText(axes.title, vp.x + vp.width / 2, Math.max(14, vp.y - 8));
    this.drawLegend(axes, vp, palette);
    ctx.restore();
  }
  drawLegend(axes, vp, palette) {
    if (!axes.legend.visible) return;
    const ctx = this.ctx;
    ctx.font = "10px -apple-system, sans-serif";
    const items = axes.series.map((series, i) => ({ series, label: series.display_name || axes.legend.labels[i] })).filter((item) => item.label);
    if (!items.length) return;
    const width = Math.min(vp.width, Math.max(...items.map((item) => ctx.measureText(item.label).width)) + 36);
    const height = items.length * 16 + 8;
    const location = axes.legend.location || "northeast";
    const x = location.includes("west") ? vp.x + 6 : vp.x + vp.width - width - 6;
    const y = location.includes("south") ? vp.y + vp.height - height - 6 : vp.y + 6;
    ctx.fillStyle = rgbText(palette.background);
    ctx.fillRect(x, y, width, height);
    ctx.strokeStyle = rgbText(colorMath.adjustForContrast(palette.axis, palette.background, palette.text));
    ctx.strokeRect(x, y, width, height);
    ctx.textAlign = "left";
    items.forEach(({ series, label }, i) => {
      const yy = y + 13 + i * 16;
      const raw = series.kind === "surface" ? colorMath.descriptorColor(series.face_color, series.cell_origins[0] || 0, axes) : series.line_color === "none" ? series.marker_edge_color : series.line_color;
      if (series.kind === "surface") {
        ctx.fillStyle = rgbText(raw || palette.background);
        ctx.fillRect(x + 6, yy - 9, 17, 9);
        const edge = colorMath.descriptorColor(series.edge_color, series.edge_indices.values[0] || 0, axes);
        if (edge) {
          ctx.strokeStyle = rgbText(colorMath.adjustForContrast(edge, palette.background, palette.text));
          ctx.strokeRect(x + 6, yy - 9, 17, 9);
        }
      } else {
        if (series.line_color !== "none" && series.line_style !== "none") {
          ctx.strokeStyle = rgbText(colorMath.adjustForContrast(raw, palette.background, palette.text));
          ctx.setLineDash({ "--": [7, 4], ":": [2, 3], "-.": [7, 3, 2, 3] }[series.line_style] || []);
          ctx.lineWidth = Math.max(1, (series.line_width ?? 0.5) * 96 / 72);
          ctx.beginPath();
          ctx.moveTo(x + 6, yy - 3);
          ctx.lineTo(x + 23, yy - 3);
          ctx.stroke();
          ctx.lineWidth = 1;
          ctx.setLineDash([]);
        }
        this.legendMarker(x + 14.5, yy - 3, series, palette);
      }
      ctx.fillStyle = rgbText(palette.text);
      ctx.fillText(label, x + 27, yy);
    });
  }
  legendMarker(x, y, series, palette) {
    const marker = series.marker;
    if (!marker || marker === "none") return;
    const ctx = this.ctx;
    const edge = series.marker_edge_color === "none" ? null : colorMath.adjustForContrast(series.marker_edge_color, palette.background, palette.text);
    const face = series.marker_face_auto ? colorMath.adjustForContrast(palette.background, palette.background, palette.text) : series.marker_face_color === "none" ? null : colorMath.adjustForContrast(series.marker_face_color, palette.background, palette.text);
    const radius = marker === "." ? 2 : 3.5;
    const open = ["+", "x", "*", "."].includes(marker);
    ctx.beginPath();
    if (["+", "x", "*"].includes(marker)) {
      if (marker !== "x") {
        ctx.moveTo(x - radius, y);
        ctx.lineTo(x + radius, y);
        ctx.moveTo(x, y - radius);
        ctx.lineTo(x, y + radius);
      }
      if (marker !== "+") {
        ctx.moveTo(x - radius, y - radius);
        ctx.lineTo(x + radius, y + radius);
        ctx.moveTo(x - radius, y + radius);
        ctx.lineTo(x + radius, y - radius);
      }
    } else if (["s", "square"].includes(marker)) ctx.rect(x - radius, y - radius, radius * 2, radius * 2);
    else if (["^", "v", ">", "<"].includes(marker)) {
      const angle = { "^": -Math.PI / 2, v: Math.PI / 2, ">": 0, "<": Math.PI }[marker];
      for (let i = 0; i < 3; i++) {
        const xx = x + Math.cos(angle + i * Math.PI * 2 / 3) * radius;
        const yy = y + Math.sin(angle + i * Math.PI * 2 / 3) * radius;
        if (i) ctx.lineTo(xx, yy);
        else ctx.moveTo(xx, yy);
      }
      ctx.closePath();
    } else ctx.arc(x, y, radius, 0, Math.PI * 2);
    if (face && !open || marker === "." && edge) {
      ctx.fillStyle = rgbText(marker === "." ? edge : face);
      ctx.fill();
    }
    if (edge && marker !== ".") {
      ctx.strokeStyle = rgbText(edge);
      ctx.stroke();
    }
  }
  destroy() {
    this.dispose();
  }
  dispose() {
    if (this.disposed) return;
    this.saveState(this.states());
    this.disposed = true;
    unmountFigureTools();
    this.observer.disconnect();
    this.unsubscribe();
    window.removeEventListener("resize", this.windowResize);
    this.dprQuery?.removeEventListener("change", this.windowResize);
    this.wrap.removeEventListener("pointerdown", this.pointer, true);
    this.wrap.removeEventListener("wheel", this.pointer, true);
    delete this.wrap.figureTest;
    this.renderer.dispose();
    this.wrap.remove();
    this.scenes = [];
    this.cameras = [];
    this.data = null;
    this.ctx = null;
  }
}
