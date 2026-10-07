"use strict";

/* pick(scene,camera,viewport,xPixel,yPixel,options={}) -> hit or null.
 * Pixels/normalisation/depth follow camera_utils; viewport origin TOP LEFT.
 * options: radiusPixels=6 (line-sample tolerance), pointRadiusPixels=6 (minimum
 * marker tolerance), pointsToPixels=96/72, depthEpsilon=1e-7. Points with size
 * zero and isolated line runs are not visible/pickable. Surface edges alone
 * are not pick targets. Picking clips against scene.bounds and GL depth [0,1].
 * Two-sided Moller-Trumbore triangles are depth-tested, then surface hits snap
 * to the closest of the FOUR cell vertices in normalised Euclidean distance;
 * ties choose the smallest column-major grid index. Hit depth is the triangle
 * intersection depth (not snapped vertex depth), to preserve occlusion.
 * Line/point candidates are original SAMPLE centres inside screen tolerance;
 * occlusion is checked at each centre's pixel as well as selecting the closest
 * depth at the clicked ray. No interpolated numerical values are invented.
 * {kind:'surface',series,row,column,data:{x,y,z},colorValue,depth} uses zero-based
 * grid row/column. {kind:'line'|'point',series,index,data,depth} uses original
 * source index (null only for unknown legacy reduced v2 mappings). Scalars or
 * RGB colorValue retain ORIGINAL CData class units, not display RGB values.
 * All tips read source.series doubles, never rounded Float32 upload positions.
 * Extra intersectTriangle(ray,a,b,c) returns {distance,point} or null; ray
 * direction should be unit length; distance is normalised-space units.
 */
const { project, unprojectRay, dataToNormalised, viewportSize } = require("./figure_camera_utils.cjs");
const { surfacePoint, samplePoint } = require("./figure_geometry_utils.cjs");
const { rawColorValue } = require("./figure_color_utils.cjs");
const sub = (a, b) => a.map((v, i) => v - b[i]);
const dot = (a, b) => a.reduce((sum, v, i) => sum + v * b[i], 0);
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
// Unit normalised-space ray and triangle xyz -> nonnegative distance/xyz or null.
function intersectTriangle(ray, a, b, c) {
  const ab = sub(b, a);
  const ac = sub(c, a);
  const p = cross(ray.direction, ac);
  const determinant = dot(ab, p);
  // Relative test also handles small aspect-ratio triangles.
  const tolerance = Number.EPSILON * 16 * Math.hypot(...ab) * Math.hypot(...ac);
  if (Math.abs(determinant) <= tolerance) return null;
  const t = sub(ray.origin, a);
  const u = dot(t, p) / determinant;
  if (u < -1e-12 || u > 1 + 1e-12) return null;
  const q = cross(t, ab);
  const v = dot(ray.direction, q) / determinant;
  if (v < -1e-12 || u + v > 1 + 1e-12) return null;
  const distance = dot(ac, q) / determinant;
  if (distance < 0) return null;
  return { distance, point: ray.origin.map((value, i) => value + ray.direction[i] * distance) };
}
// Test normalised xyz against axes clipping bounds with Float32-sized tolerance.
function inside(scene, point) {
  return point.every((v, i) => v >= scene.bounds.min[i] - 1e-5 && v <= scene.bounds.max[i] + 1e-5);
}
// Read one normalised xyz upload vertex by zero-based vertex index.
function vertex(positions, index) {
  return Array.from(positions.subarray(index * 3, index * 3 + 3));
}
// Closest two-sided visible triangle at a top-left viewport pixel, or null.
function frontSurface(scene, camera, viewport, x, y) {
  const ray = unprojectRay(camera, viewport, x, y);
  let best = null;
  for (const mesh of scene.meshes) {
    for (let triangle = 0; triangle < mesh.indices.length / 3; triangle++) {
      const offset = triangle * 3;
      const hit = intersectTriangle(ray, vertex(mesh.positions, mesh.indices[offset]),
        vertex(mesh.positions, mesh.indices[offset + 1]), vertex(mesh.positions, mesh.indices[offset + 2]));
      if (!hit || !inside(scene, hit.point)) continue;
      const depth = project(camera, viewport, hit.point).depth;
      if (depth < 0 || depth > 1 || best && depth >= best.depth) continue;
      best = { mesh, origin: mesh.cellOfTriangle[triangle], point: hit.point, depth };
    }
  }
  return best;
}
// Snap intersection to original cell vertex; return data units and raw CData.
function surfaceTip(scene, hit) {
  const s = hit.mesh.source.series;
  const rows = s.shape[0];
  let index = hit.origin;
  let nearest = Infinity;
  for (const candidate of [hit.origin, hit.origin + 1, hit.origin + rows, hit.origin + rows + 1]) {
    const point = dataToNormalised(scene.axes, surfacePoint(s, candidate));
    const distance = dot(sub(point, hit.point), sub(point, hit.point));
    if (distance < nearest) {
      index = candidate;
      nearest = distance;
    }
  }
  const [x, y, z] = surfacePoint(s, index);
  return { kind: "surface", series: hit.mesh.series, row: index % rows, column: Math.floor(index / rows),
    data: { x, y, z }, colorValue: rawColorValue(s, index), depth: hit.depth };
}
// Top-left viewport pixel -> nearest visible original sample/grid tip, or null.
function pick(scene, camera, viewport, xPixel, yPixel, options = {}) {
  const vp = viewportSize(viewport);
  if (!Number.isFinite(xPixel) || !Number.isFinite(yPixel) || xPixel < vp.x || yPixel < vp.y || xPixel > vp.x + vp.width || yPixel > vp.y + vp.height) return null;
  const surface = frontSurface(scene, camera, viewport, xPixel, yPixel);
  let best = surface ? surfaceTip(scene, surface) : null;
  let bestDistance = Infinity;
  const epsilon = options.depthEpsilon ?? 1e-7;
  for (const [kind, objects] of [["line", scene.lines], ["point", scene.points]]) {
    for (const object of objects) {
      for (let i = 0; i < object.positions.length / 3; i++) {
        if (kind === "line") {
          if (object.source.pickableVertices && !object.source.pickableVertices[i]) continue;
          let run = 0;
          while (run + 1 < object.breaks.length && object.breaks[run + 1] <= i) run++;
          if (object.breaks[run + 1] - object.breaks[run] < 2) continue;
        }
        if (kind === "point" && object.sizes[i] <= 0) continue;
        const point = vertex(object.positions, i);
        if (!inside(scene, point)) continue;
        const projected = project(camera, viewport, point);
        if (projected.depth < 0 || projected.depth > 1 || projected.x < vp.x || projected.x > vp.x + vp.width || projected.y < vp.y || projected.y > vp.y + vp.height) continue;
        const distance = Math.hypot(projected.x - xPixel, projected.y - yPixel);
        const diameter = kind === "point" ? (object.sizeUnits === "points-squared" ? Math.sqrt(object.sizes[i]) : object.sizes[i]) * (options.pointsToPixels ?? 96 / 72) : 0;
        const radius = kind === "point" ? Math.max(options.pointRadiusPixels ?? 6, diameter / 2) : options.radiusPixels ?? 6;
        if (distance > radius) continue;
        const occluder = frontSurface(scene, camera, viewport, projected.x, projected.y);
        if (occluder && projected.depth > occluder.depth + epsilon) continue;
        if (best && (projected.depth > best.depth + epsilon || Math.abs(projected.depth - best.depth) <= epsilon && distance >= bestDistance)) continue;
        const s = object.source.series;
        const sampleIndex = object.source.sampleIndices[i];
        const [x, y, z] = samplePoint(s, sampleIndex);
        best = { kind, series: object.series, index: object.sourceIndices[i] === 0xffffffff ? null : object.sourceIndices[i], data: { x, y, z }, depth: projected.depth };
        if (s.kind === "scatter" && s.color_data) best.colorValue = s.color_data.encoding === "indexed" ? s.color_data.data.values[0] : s.color_data.data.values.slice();
        bestDistance = distance;
      }
    }
  }
  return best;
}
module.exports = { pick, intersectTriangle };
