"use strict";

/* Pure orthographic camera API (no DOM).
 * Coordinates: right-handed, Z-up, centred box [-extent,+extent], with the
 * largest extent = 1. Data limits map to this box; reverse flips that axis.
 * extent is (limit span / DAR), divided by its largest component. A manual
 * PBAR with automatic DAR instead supplies extent; manual DAR takes priority
 * when both are manual. No claim of Octave pixel-identical stretch-to-fill.
 * view=[az,el] degrees is applied ONCE: back=(sin(az)cos(el),-cos(az)cos(el),
 * sin(el)). At [0,90], +X is right and +Y is up. Raw camera orientation is not
 * applied a second time. Initial fit encloses the box's sphere with 5% margin;
 * exported view_angle is metadata, not a perspective/orthographic conversion.
 * All transforms compute in float64. GL matrices are column-major, column
 * vectors, right-handed view looking down -Z, NDC depth [-1,1]. project depth
 * is window depth [0,1], smaller is nearer. Pixel origin is TOP LEFT; viewport
 * is {width,height,x?:0,y?:0}, in the SAME pixels as pointer/line coordinates.
 * Gestures mutate and return camera. Positive dx orbits positive azimuth;
 * positive dy lowers elevation, at 0.5 degrees/pixel. Pan drags content with
 * the pointer. zoom factor>1 magnifies; optional anchorNdc=[x,y] has Y up and
 * uses the last viewport recorded by matrices/project/unprojectRay/pan.
 * serialize(camera, identity?) returns JSON-safe state, scoped to optional
 * {epoch,job,figure}; restore(camera,state,identity?) rejects mismatched state
 * with false and otherwise mutates/returns camera. reset restores exported fit.
 * dataToNormalised(axes, [x,y,z]) and inverse return ordinary double arrays.
 * axisBox returns outward face normals/away flags, 12 edges (away iff both
 * adjacent faces face away, behind iff at least one does: the outline of the
 * back walls), and axes.{x,y,z}: chosen silhouette edge, tick positions
 * and labelAnchor, all normalised. Tick indices are original axes tick indices.
 */
const KEYS = ["x", "y", "z"];
const RAD = Math.PI / 180;
const add = (a, b) => a.map((v, i) => v + b[i]);
const sub = (a, b) => a.map((v, i) => v - b[i]);
const scale = (a, s) => a.map((v) => v * s);
const dot = (a, b) => a.reduce((s, v, i) => s + v * b[i], 0);
const length = (a) => Math.hypot(...a);
// Dimensionless half extents; logarithms avoid overflow in span/DAR ratios.
function extents(axes) {
  if (axes.plot_box_aspect_ratio_mode === "manual" && axes.data_aspect_ratio_mode !== "manual") {
    const largest = Math.max(...axes.plot_box_aspect_ratio);
    return axes.plot_box_aspect_ratio.map((v) => v / largest);
  }
  const ratios = axes.data_aspect_ratio || [1, 1, 1];
  const logs = KEYS.map((key, i) => {
    const lim = axes[key + "lim"] || [-1, 1];
    const span = lim[1] - lim[0];
    const logSpan = Number.isFinite(span) ? Math.log(span) : Math.log(lim[1] / 2 - lim[0] / 2) + Math.LN2;
    return logSpan - Math.log(ratios[i]);
  });
  const largest = Math.max(...logs);
  return logs.map((v) => Math.exp(v - largest));
}
// Fraction of a finite increasing data interval, computed in double precision.
function fraction(value, low, high) {
  const span = high - low;
  const offset = value - low;
  return Number.isFinite(span) && Number.isFinite(offset) ? offset / span : (value / 2 - low / 2) / (high / 2 - low / 2);
}
// Data-unit xyz -> centred, aspect-scaled/reversed xyz; float64 array.
function dataToNormalised(axes, point) {
  const extent = extents(axes);
  return KEYS.map((key, i) => {
    const lim = axes[key + "lim"] || [-1, 1];
    return (fraction(point[i], lim[0], lim[1]) * 2 - 1) * extent[i] * (axes[key + "dir"] === "reverse" ? -1 : 1);
  });
}
// Normalised xyz -> original data-unit xyz; float64 array.
function normalisedToData(axes, point) {
  const extent = extents(axes);
  return KEYS.map((key, i) => {
    const lim = axes[key + "lim"] || [-1, 1];
    const f = (point[i] / extent[i] * (axes[key + "dir"] === "reverse" ? -1 : 1) + 1) / 2;
    const span = lim[1] - lim[0];
    if (!Number.isFinite(span)) return (1 - f) * lim[0] + f * lim[1];
    return f <= 0.5 ? lim[0] + f * span : lim[1] - (1 - f) * span;
  });
}
// Unit right/up/back vectors in the normalised right-handed Z-up frame.
function basis(camera) {
  const a = camera.azimuth * RAD;
  const e = camera.elevation * RAD;
  return { right: [Math.cos(a), Math.sin(a), 0],
    up: [-Math.sin(a) * Math.sin(e), Math.cos(a) * Math.sin(e), Math.cos(e)],
    back: [Math.sin(a) * Math.cos(e), -Math.cos(a) * Math.cos(e), Math.sin(e)] };
}
// Copy mutable view state (degrees, normalised target, dimensionless zoom).
function snapshot(camera) {
  return { azimuth: camera.azimuth, elevation: camera.elevation, target: camera.target.slice(), zoom: camera.zoom };
}
// Create mutable state and immutable-by-convention reset baseline from axes.
function createCamera(axes) {
  const extent = extents(axes);
  const radius = length(extent);
  const camera = { axes, extent, azimuth: (axes.view || [0, 90])[0], elevation: (axes.view || [0, 90])[1],
    target: dataToNormalised(axes, axes.camera?.target || KEYS.map((key) => {
      const lim = axes[key + "lim"] || [-1, 1];
      return lim[0] / 2 + lim[1] / 2;
    })), zoom: 1, halfHeight: radius * 1.05, distance: radius * 4, near: 0, far: radius * 8, viewportAspect: 1 };
  camera.initial = snapshot(camera);
  return camera;
}
// Resolve pixel width/height and optional top-left origin; w/h aliases accepted.
function viewportSize(viewport) {
  const width = viewport.width ?? viewport.w;
  const height = viewport.height ?? viewport.h;
  if (!(Number.isFinite(width) && width > 0 && Number.isFinite(height) && height > 0)) throw new RangeError("Positive viewport dimensions required");
  return { width, height, x: viewport.x || 0, y: viewport.y || 0 };
}
// Orthographic half width/height in normalised units for a viewport aspect.
function planeSize(camera, aspect = camera.viewportAspect) {
  const halfY = camera.halfHeight / Math.min(1, aspect) / camera.zoom;
  return [halfY * aspect, halfY];
}
// Float64 column-major 4x4 matrix product a*b, acting on column vectors.
function multiply(a, b) {
  const out = new Float64Array(16);
  for (let col = 0; col < 4; col++) {
    for (let row = 0; row < 4; row++) {
      for (let k = 0; k < 4; k++) out[col * 4 + row] += a[k * 4 + row] * b[col * 4 + k];
    }
  }
  return out;
}
// Return GL Float32 matrices plus view64/projection64/viewProjection64 originals; records viewport aspect.
function matrices(camera, viewportWidth, viewportHeight) {
  const vp = viewportSize({ width: viewportWidth, height: viewportHeight });
  camera.viewportAspect = vp.width / vp.height;
  const { right, up, back } = basis(camera);
  const eye = add(camera.target, scale(back, camera.distance));
  const view64 = new Float64Array([right[0], up[0], back[0], 0, right[1], up[1], back[1], 0,
    right[2], up[2], back[2], 0, -dot(right, eye), -dot(up, eye), -dot(back, eye), 1]);
  const [hx, hy] = planeSize(camera);
  const span = camera.far - camera.near;
  const projection64 = new Float64Array([1 / hx, 0, 0, 0, 0, 1 / hy, 0, 0, 0, 0, -2 / span, 0,
    0, 0, -(camera.far + camera.near) / span, 1]);
  const viewProjection64 = multiply(projection64, view64);
  return { view: new Float32Array(view64), projection: new Float32Array(projection64), viewProjection: new Float32Array(viewProjection64),
    view64, projection64, viewProjection64 };
}
// Mutate azimuth by degrees, wrapping into [0,360).
function rotateAzimuth(camera, degrees) {
  if (!Number.isFinite(degrees)) throw new RangeError("Finite angle required");
  camera.azimuth = ((camera.azimuth + degrees) % 360 + 360) % 360;
  return camera;
}
// Mutate view by top-left pixel deltas, 0.5 degrees/pixel, clamp before poles.
function orbit(camera, dxPixels, dyPixels) {
  if (![dxPixels, dyPixels].every(Number.isFinite)) throw new RangeError("Finite pixel deltas required");
  rotateAzimuth(camera, dxPixels * 0.5);
  camera.elevation = Math.max(-89.999, Math.min(89.999, camera.elevation - dyPixels * 0.5));
  return camera;
}
// Translate target in normalised view-plane units so content follows pixel drag.
function pan(camera, dxPixels, dyPixels, viewport) {
  const vp = viewportSize(viewport);
  if (![dxPixels, dyPixels].every(Number.isFinite)) throw new RangeError("Finite pixel deltas required");
  camera.viewportAspect = vp.width / vp.height;
  const [hx, hy] = planeSize(camera);
  const { right, up } = basis(camera);
  camera.target = add(camera.target, add(scale(right, -2 * hx * dxPixels / vp.width), scale(up, 2 * hy * dyPixels / vp.height)));
  return camera;
}
// Multiply magnification, optionally fixing a Y-up NDC anchor using last viewport.
function zoom(camera, factor, anchorNdc) {
  if (!(Number.isFinite(factor) && factor > 0)) throw new RangeError("Positive zoom factor required");
  if (anchorNdc && !(anchorNdc.length === 2 && anchorNdc.every(Number.isFinite))) throw new RangeError("Finite NDC anchor required");
  const before = planeSize(camera);
  camera.zoom = Math.max(1e-6, Math.min(1e6, camera.zoom * factor));
  if (anchorNdc) {
    const after = planeSize(camera);
    const { right, up } = basis(camera);
    camera.target = add(camera.target, add(scale(right, anchorNdc[0] * (before[0] - after[0])), scale(up, anchorNdc[1] * (before[1] - after[1]))));
  }
  return camera;
}
// Restore initial degrees/target/zoom; retain viewport aspect for next redraw.
function reset(camera) {
  Object.assign(camera, camera.initial, { target: camera.initial.target.slice() });
  return camera;
}
// JSON-safe copy; optional identity scopes local state to epoch/job/figure.
function serialize(camera, identity) {
  return { version: 1, identity: identity ? { epoch: identity.epoch, job: identity.job, figure: identity.figure } : null, ...snapshot(camera) };
}
// Restore a valid serialized state in place, or false without mutation.
function restore(camera, state, identity) {
  if (!state || state.version !== 1 || ![state.azimuth, state.elevation, state.zoom].every(Number.isFinite) || state.zoom < 1e-6 || state.zoom > 1e6 ||
    Math.abs(state.elevation) > 90 || !Array.isArray(state.target) || state.target.length !== 3 || !state.target.every(Number.isFinite)) return false;
  if (identity && (!state.identity || ["epoch", "job", "figure"].some((key) => state.identity[key] !== identity[key]))) return false;
  Object.assign(camera, { azimuth: state.azimuth, elevation: state.elevation, zoom: state.zoom, target: state.target.slice() });
  return camera;
}
// Normalised xyz -> top-left viewport pixels and window depth [0,1].
function project(camera, viewport, pointNormalised) {
  const vp = viewportSize(viewport);
  camera.viewportAspect = vp.width / vp.height;
  const [hx, hy] = planeSize(camera);
  const { right, up, back } = basis(camera);
  const relative = sub(pointNormalised, camera.target);
  return { x: vp.x + (dot(relative, right) / hx + 1) * vp.width / 2,
    y: vp.y + (1 - dot(relative, up) / hy) * vp.height / 2,
    depth: (camera.distance - dot(relative, back) - camera.near) / (camera.far - camera.near) };
}
// Viewport pixel -> near-plane origin and unit forward direction in normalised xyz.
function unprojectRay(camera, viewport, xPixel, yPixel) {
  const vp = viewportSize(viewport);
  camera.viewportAspect = vp.width / vp.height;
  const [hx, hy] = planeSize(camera);
  const { right, up, back } = basis(camera);
  const offset = add(scale(right, ((xPixel - vp.x) * 2 / vp.width - 1) * hx), scale(up, (1 - (yPixel - vp.y) * 2 / vp.height) * hy));
  return { origin: add(add(camera.target, offset), scale(back, camera.distance - camera.near)), direction: scale(back, -1) };
}
// Normalised box edges/faces, visibility classification and original tick/label anchors.
function axisBox(camera, axes) {
  const extent = extents(axes);
  const { back, up, right } = basis(camera);
  const faces = [];
  const edges = [];
  for (let i = 0; i < 3; i++) {
    for (const sign of [-1, 1]) {
      const normal = [0, 0, 0];
      normal[i] = sign;
      faces.push({ axis: KEYS[i], sign, normal, away: dot(normal, back) <= 0 });
    }
  }
  const result = {};
  for (let i = 0; i < 3; i++) {
    const other = [0, 1, 2].filter((j) => j !== i);
    const choices = [];
    for (const s0 of [-1, 1]) {
      for (const s1 of [-1, 1]) {
        const start = [0, 0, 0];
        start[i] = -extent[i];
        start[other[0]] = s0 * extent[other[0]];
        start[other[1]] = s1 * extent[other[1]];
        const end = start.slice();
        end[i] = extent[i];
        const adjacentFaces = [other[0] * 2 + (s0 > 0 ? 1 : 0), other[1] * 2 + (s1 > 0 ? 1 : 0)];
        const edge = { axis: KEYS[i], start, end, faces: adjacentFaces, away: adjacentFaces.every((j) => faces[j].away),
          behind: adjacentFaces.some((j) => faces[j].away) };
        edges.push(edge);
        choices.push(edge);
      }
    }
    // Octave-like placement: an axis sits on a silhouette edge (one adjacent
    // face towards the viewer, one away), X/Y on the lowest one on screen and Z
    // on the leftmost one, so ticks never run through the middle of the data.
    const silhouette = choices.filter((item) => item.faces.filter((j) => faces[j].away).length === 1);
    const order = i === 2 ? [right, up] : [up, right];
    const measure = (item, direction) => Math.round(dot(item.start.map((v, j) => j === i ? 0 : v), direction) * 1e9);
    const pool = silhouette.length ? silhouette : choices;
    pool.sort((a, b) => measure(a, order[0]) - measure(b, order[0]) || measure(a, order[1]) - measure(b, order[1]));
    const edge = pool[0];
    const key = KEYS[i];
    const lim = axes[key + "lim"] || [-1, 1];
    const ticks = (axes[key + "tick"] || []).flatMap((value, index) => {
      if (value < lim[0] || value > lim[1]) return [];
      const position = edge.start.slice();
      const data = [0, 0, 0];
      data[i] = value;
      position[i] = dataToNormalised(axes, data)[i];
      return [{ value, index, label: (axes[key + "ticklabel"] || [])[index] ?? String(value), position }];
    });
    const labelAnchor = edge.start.map((v, j) => j === i ? 0 : v * 1.12);
    result[key] = { edge, ticks, label: axes[key + "label"] || "", labelAnchor };
  }
  return { faces, edges, axes: result, extent };
}
module.exports = { createCamera, matrices, orbit, rotateAzimuth, pan, zoom, reset, serialize, restore,
  project, unprojectRay, dataToNormalised, normalisedToData, axisBox, extents, basis, viewportSize };
