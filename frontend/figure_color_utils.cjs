"use strict";

/* Pure colour API; all RGB inputs/outputs are sRGB triples in [0,1], NOT bytes.
 * resolveSurfaceColors(surface, axes) returns {faceColors,vertexColors,edgeColors}:
 * Float32 RGB triples ordered by cell_origins and edge_indices respectively.
 * Flat edges have one RGB per edge; interp edges have start/end RGB pairs.
 * Interp faces instead return vertexColors indexed by original grid vertex.
 * Only referenced vertices are resolved; unused entries are NaN, never zero
 * substitutes for missing data. A 'none' descriptor returns an empty buffer.
 * Flat faces use grid origin; flat edges use FIRST endpoint. Descriptor data is planar
 * column-major; truecolor uint8/uint16 divides by 255/65535. Indexed scaled
 * uses floor(N*(v-low)/(high-low)); direct uses integer v, floating trunc(v)-1;
 * both clip to the existing map without resampling. These are the FROZEN
 * provisional contract rules, not a claim of measured Octave pixel equivalence.
 * colorbarGradient(record) accepts a colorbar OR axes. Returns discrete stops
 * [{offset,rgb}], TWO per map entry (no interpolation/resampling), offset 0..1
 * runs bottom->top (vertical), left->right (horizontal); reverse reverses map.
 * contrast(a,b) is WCAG sRGB luminance ratio. adjustForContrast(rgb,bg,text)
 * clamps RGB, preserves 'none', and substitutes text iff ratio <3 (same as
 * figures.js). Theme RGBs must be supplied by R3 from --plot-* tokens.
 * Never apply this adjustment to faceColors or colorbarGradient.
 * Extra helpers: mapColor(map,index), descriptorColor(descriptor,owner,axes),
 * rawColorValue(surface,index) (original scalar/RGB class values for tips).
 */
const clamp = (v, low, high) => Math.max(low, Math.min(high, v));
// A: map original CData to sRGB at vertices, then let the GPU interpolate RGB.
// A future scalar/fragment-map policy belongs at this single policy switch,
// resolveSurfaceColors, and surfaceColorShaderPath in figure_webgl.js.
const SURFACE_COLOR_INTERPOLATION = "vertex-rgb";
// Column-major map and clipped zero-based entry -> original unit sRGB triple.
function mapColor(map, index) {
  const n = map.shape[0];
  const i = clamp(index, 0, n - 1);
  return [map.values[i], map.values[i + n], map.values[i + 2 * n]];
}
// Contract colour descriptor at grid owner index -> sRGB triple or null.
function descriptorColor(descriptor, owner, axes) {
  if (descriptor.mode === "none") return null;
  if (descriptor.mode === "constant") return descriptor.rgb.slice();
  const values = descriptor.data.values;
  if (descriptor.encoding === "truecolor") {
    const n = descriptor.data.shape[0] * descriptor.data.shape[1];
    if ([owner, owner + n, owner + 2 * n].some((i) => !Number.isFinite(values[i]))) return null;
    const divisor = descriptor.cdata_class === "uint8" ? 255 : descriptor.cdata_class === "uint16" ? 65535 : 1;
    return [values[owner] / divisor, values[owner + n] / divisor, values[owner + 2 * n] / divisor];
  }
  const n = axes.colormap.shape[0];
  const value = values[owner];
  if (!Number.isFinite(value)) return null;
  const integerClass = /^(u?int)(8|16|32|64)$/.test(descriptor.cdata_class);
  const low = axes.clim[0];
  const high = axes.clim[1];
  const span = high - low;
  const fraction = Number.isFinite(span) ? (value - low) / span : (value / 2 - low / 2) / (high / 2 - low / 2);
  const index = descriptor.mapping === "direct" ? (integerClass ? value : Math.trunc(value) - 1) :
    value <= low ? 0 : value >= high ? n - 1 : Math.floor(n * fraction);
  return mapColor(axes.colormap, index);
}
// Float32 RGB per valid cell and per contract edge; never theme-adjust faces.
function resolveSurfaceColors(surface, axes) {
  const faces = surface.cell_origins;
  const count = surface.edge_indices.shape[0];
  const interp = surface.face_color.mode === "interp";
  const faceColors = new Float32Array(surface.face_color.mode === "none" || interp ? 0 : faces.length * 3);
  const vertexColors = new Float32Array(interp ? surface.shape[0] * surface.shape[1] * 3 : 0);
  if (interp) {
    vertexColors.fill(NaN);
    const rows = surface.shape[0];
    const vertices = new Set(faces.flatMap((i) => [i, i + 1, i + rows, i + rows + 1]));
    for (const i of vertices) vertexColors.set(descriptorColor(surface.face_color, i, axes), i * 3);
  }
  const edgeStride = surface.edge_color.mode === "interp" ? 2 : 1;
  const edgeColors = new Float32Array(surface.edge_color.mode === "none" ? 0 : count * 3 * edgeStride);
  for (let i = 0; i < faceColors.length / 3; i++) faceColors.set(descriptorColor(surface.face_color, faces[i], axes), i * 3);
  if (surface.edge_color.mode !== "none") for (let i = 0; i < count; i++) {
    for (let end = 0; end < edgeStride; end++) edgeColors.set(descriptorColor(surface.edge_color, surface.edge_indices.values[i + end * count], axes), (i * edgeStride + end) * 3);
  }
  return { faceColors, vertexColors, edgeColors };
}
// Two unit-offset stops per colormap entry; reversed physical direction supported.
function colorbarGradient(record) {
  const count = record.colormap.shape[0];
  const stops = [];
  for (let i = 0; i < count; i++) {
    const rgb = mapColor(record.colormap, record.direction === "reverse" ? count - 1 - i : i);
    stops.push({ offset: i / count, rgb: rgb.slice() });
    stops.push({ offset: (i + 1) / count, rgb: rgb.slice() });
  }
  return stops;
}
// Unit sRGB -> relative linear-light luminance, using figures.js coefficients.
function luminance(rgb) {
  return rgb.reduce((sum, value, i) => sum + (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4) * [0.2126, 0.7152, 0.0722][i], 0);
}
// Unit sRGB pair -> dimensionless luminance contrast ratio.
function contrast(rgbA, rgbB) {
  const a = luminance(rgbA);
  const b = luminance(rgbB);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}
// Unit sRGB -> clamped RGB or supplied plot-text RGB when contrast is below 3.
function adjustForContrast(rgb, background, textRgb) {
  if (rgb === "none") return "none";
  if (!rgb || rgb.length !== 3 || !Array.from(rgb).every(Number.isFinite)) return Array.from(textRgb);
  const bounded = Array.from(rgb, (v) => clamp(v, 0, 1));
  return contrast(bounded, background) < 3 ? Array.from(textRgb) : bounded;
}
// Preserve HSL hue/saturation while solving a target WCAG luminance.
// Endpoints black/white are reachable for every hue; no desaturation is needed.
function atLuminance(rgb, target) {
  const min = Math.min(...rgb), max = Math.max(...rgb), light = (min + max) / 2;
  const saturation = max === min ? 0 : (max - min) / (1 - Math.abs(2 * light - 1));
  const at = (l) => {
    const chroma = (1 - Math.abs(2 * l - 1)) * saturation;
    return rgb.map((v) => l - chroma / 2 + (max === min ? 0 : (v - min) / (max - min)) * chroma);
  };
  let low = 0, high = 1;
  for (let i = 0; i < 52; i++) {
    const mid = (low + high) / 2;
    if (luminance(at(mid)) < target) low = mid;
    else high = mid;
  }
  return at((low + high) / 2).map((v) => clamp(v, 0, 1));
}
// One palette per source contour group. Map only the background-side tail by
// an increasing affine luminance map [original extreme,P] -> [threshold,P].
// The pivot is at most .20 luminance beyond threshold and bounded by the
// original readable extreme where possible. All-unreadable palettes relax
// that bound. Beyond-pivot entries (and all-readable palettes) are exact.
function adjustContourPalette(rgbs, background, textRgb, minimum = 3) {
  if (rgbs.every((rgb) => contrast(rgb, background) >= minimum)) return rgbs;
  const bg = luminance(background);
  const lighter = contrast([1, 1, 1], background) >= contrast([0, 0, 0], background);
  const threshold = lighter ? minimum * (bg + .05) - .05 : (bg + .05) / minimum - .05;
  if (threshold < 0 || threshold > 1) {
    return rgbs.map((rgb) => contrast(rgb, background) >= minimum ? rgb : Array.from(textRgb));
  }
  const ys = rgbs.map(luminance);
  // Work in a coordinate that increases away from the background.
  const t = lighter ? threshold : 1 - threshold;
  const xs = ys.map((y) => lighter ? y : 1 - y);
  const start = Math.min(...xs), end = Math.max(...xs);
  const pivot = end > t ? Math.min(t + .20, end) : Math.min(1, t + .20);
  return rgbs.map((rgb, i) => {
    if (xs[i] >= pivot) return rgb;
    const x = start === end ? t : t + (xs[i] - start) * (pivot - t) / (pivot - start);
    return atLuminance(rgb, lighter ? x : 1 - x);
  });
}
function contourDisplayColors(axes, background, textRgb) {
  const result = new Map(), groups = new Map();
  for (const series of axes?.series || []) {
    if (series.role !== "contour" || series.contour_color?.association !== "level") continue;
    // Older v3 records lack group identity; their axes is the compatibility group.
    const key = series.contour_group ?? "legacy";
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(series);
  }
  for (const group of groups.values()) {
    const adjusted = adjustContourPalette(group.map((s) => resolveContourColor(s, axes)), background, textRgb);
    group.forEach((s, i) => result.set(s, adjusted[i]));
  }
  return result;
}
// Read original scalar or RGB CData units at column-major grid vertex index.
function rawColorValue(surface, index) {
  const values = surface.cdata.values;
  const n = surface.shape[0] * surface.shape[1];
  return surface.cdata.shape.length === 3 ? [values[index], values[index + n], values[index + 2 * n]] : values[index];
}
function resolveContourColor(series, axes) {
  return descriptorColor(series.contour_color, 0, axes) ?? "none";
}
// Owner is a source face for flat data, or source vertex for explicit interp.
function patchColor(descriptor, owner, axes) {
  return descriptorColor(descriptor, descriptor.mode === "flat" && descriptor.data.shape[0] === 1 ? 0 : owner, axes);
}
function rawPatchColor(series, face, vertex) {
  const c = [series.face_color, series.edge_color].find((c) => ["flat", "interp"].includes(c.mode));
  if (!c) return undefined;
  const n = c.data.shape[0], i = c.mode === "interp" ? vertex : n === 1 ? 0 : face;
  return c.encoding === "truecolor" ? [c.data.values[i], c.data.values[i + n], c.data.values[i + 2 * n]] : c.data.values[i];
}
module.exports = { luminance, adjustContourPalette, contourDisplayColors, SURFACE_COLOR_INTERPOLATION, resolveSurfaceColors, resolveContourColor, patchColor, rawPatchColor, colorbarGradient, contrast, adjustForContrast, mapColor, descriptorColor, rawColorValue };
