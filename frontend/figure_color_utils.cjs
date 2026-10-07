"use strict";

/* Pure colour API; all RGB inputs/outputs are sRGB triples in [0,1], NOT bytes.
 * resolveSurfaceColors(surface, axes) returns {faceColors,edgeColors}:
 * Float32 RGB triples ordered by cell_origins and edge_indices respectively.
 * A 'none' descriptor returns an empty buffer; callers omit that draw pass.
 * Faces use grid origin; edges use FIRST endpoint. Descriptor data is planar
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
    const divisor = descriptor.cdata_class === "uint8" ? 255 : descriptor.cdata_class === "uint16" ? 65535 : 1;
    return [values[owner] / divisor, values[owner + n] / divisor, values[owner + 2 * n] / divisor];
  }
  const n = axes.colormap.shape[0];
  const value = values[owner];
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
  const faceColors = new Float32Array(surface.face_color.mode === "none" ? 0 : faces.length * 3);
  const edgeColors = new Float32Array(surface.edge_color.mode === "none" ? 0 : count * 3);
  for (let i = 0; i < faceColors.length / 3; i++) faceColors.set(descriptorColor(surface.face_color, faces[i], axes), i * 3);
  for (let i = 0; i < edgeColors.length / 3; i++) edgeColors.set(descriptorColor(surface.edge_color, surface.edge_indices.values[i], axes), i * 3);
  return { faceColors, edgeColors };
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
// Read original scalar or RGB CData units at column-major grid vertex index.
function rawColorValue(surface, index) {
  const values = surface.cdata.values;
  const n = surface.shape[0] * surface.shape[1];
  return surface.cdata.shape.length === 3 ? [values[index], values[index + n], values[index + 2 * n]] : values[index];
}
module.exports = { resolveSurfaceColors, colorbarGradient, contrast, adjustForContrast, mapColor, descriptorColor, rawColorValue };
