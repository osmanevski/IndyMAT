"use strict";

// Pure bounded polygon geometry. No graphics, triangulation diagonals in edges,
// or coordinate replacement. Faces are simple planar polygons, at most 64
// vertices. Predicate tolerance is 1e-10 after face-local normalisation.
// Near-degenerate/ambiguous geometry conservatively falls back. The Octave
// helper implements the same predicates and first-valid-ear source-order scan.
const EPS = 1e-10;
const MAX_FACE_VERTICES = 64;
const LIMITS = Object.freeze({ vertices: 40000, triangles: 80000, faceVertices: MAX_FACE_VERTICES });
function patchPoint(series, index) {
  const n = series.vertices.shape[0];
  return [series.vertices.values[index], series.vertices.values[index + n], series.vertices.values[index + 2 * n]];
}
const cross = (a, b, c) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
function onSegment(a, b, p) {
  return Math.abs(cross(a, b, p)) <= EPS && [0, 1].every((k) => p[k] >= Math.min(a[k], b[k]) - EPS && p[k] <= Math.max(a[k], b[k]) + EPS);
}
function intersects(a, b, c, d) {
  const abC = cross(a, b, c), abD = cross(a, b, d), cdA = cross(c, d, a), cdB = cross(c, d, b);
  return ((abC > EPS && abD < -EPS || abC < -EPS && abD > EPS) &&
    (cdA > EPS && cdB < -EPS || cdA < -EPS && cdB > EPS)) ||
    onSegment(a, b, c) || onSegment(a, b, d) || onSegment(c, d, a) || onSegment(c, d, b);
}
function polygon(vertices, face) {
  let points = face.map((i) => vertices[i]);
  const scale = Math.max(...points.flat().map(Math.abs));
  if (!(scale > 0)) return null;
  points = points.map((p) => p.map((v) => v / scale));
  const origin = points[0];
  points = points.map((p) => p.map((v, k) => v - origin[k]));
  const extent = Math.max(...points.flat().map(Math.abs));
  if (!(extent > 0)) return null;
  points = points.map((p) => p.map((v) => v / extent));
  const n = points.length;
  for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) {
    if (Math.max(...points[i].map((v, k) => Math.abs(v - points[j][k]))) <= EPS) return null;
  }
  const normal = [0, 0, 0];
  for (let i = 0; i < n; i++) {
    const a = points[i], b = points[(i + 1) % n];
    normal[0] += (a[1] - b[1]) * (a[2] + b[2]);
    normal[1] += (a[2] - b[2]) * (a[0] + b[0]);
    normal[2] += (a[0] - b[0]) * (a[1] + b[1]);
  }
  const length = Math.sqrt(normal.reduce((sum, v) => sum + v * v, 0));
  if (!(length > EPS) || points.some((p) => Math.abs(p.reduce((sum, v, k) => sum + v * normal[k], 0)) > EPS * length)) return null;
  let drop = 0;
  for (let k = 1; k < 3; k++) if (Math.abs(normal[k]) > Math.abs(normal[drop])) drop = k;
  const xy = points.map((p) => p.filter((_, k) => k !== drop));
  for (let i = 0; i < n; i++) {
    // Adjacent collinear backtracking is overlapping, not a simple polygon.
    const previous = xy[(i + n - 1) % n], next = xy[(i + 1) % n];
    if (Math.abs(cross(previous, xy[i], next)) <= EPS &&
      (previous[0] - xy[i][0]) * (next[0] - xy[i][0]) + (previous[1] - xy[i][1]) * (next[1] - xy[i][1]) > EPS) return null;
    for (let j = i + 1; j < n; j++) {
      if (j === i + 1 || i === 0 && j === n - 1) continue;
      if (intersects(xy[i], xy[(i + 1) % n], xy[j], xy[(j + 1) % n])) return null;
    }
  }
  let area = 0;
  for (let i = 0; i < n; i++) area += xy[i][0] * xy[(i + 1) % n][1] - xy[(i + 1) % n][0] * xy[i][1];
  if (Math.abs(area) <= EPS) return null;
  const winding = area > 0 ? 1 : -1;
  const remaining = Array.from({ length: n }, (_, i) => i), triangles = [];
  while (remaining.length > 3) {
    let ear = -1;
    for (let i = 0; i < remaining.length; i++) {
      const a = remaining[(i + remaining.length - 1) % remaining.length], b = remaining[i], c = remaining[(i + 1) % remaining.length];
      if (winding * cross(xy[a], xy[b], xy[c]) <= EPS) continue;
      if (remaining.some((p) => p !== a && p !== b && p !== c &&
        winding * cross(xy[a], xy[b], xy[p]) >= -EPS && winding * cross(xy[b], xy[c], xy[p]) >= -EPS && winding * cross(xy[c], xy[a], xy[p]) >= -EPS)) continue;
      ear = i;
      triangles.push([face[a], face[b], face[c]]);
      break;
    }
    if (ear < 0) return null;
    remaining.splice(ear, 1);
  }
  if (winding * cross(...remaining.map((i) => xy[i])) <= EPS) return null;
  triangles.push(remaining.map((i) => face[i]));
  return triangles;
}
function triangulatePatch(vertices, faces) {
  const fail = (code, args) => ({ triangles: [], faceOfTriangle: [], edges: [], code, args });
  const budget = (name, actual, limit) => fail("budget_exceeded", { budget: name, actual, limit });
  if (!Array.isArray(vertices) || !Array.isArray(faces)) return fail("invalid_data", { property: "vertices" });
  if (vertices.length > LIMITS.vertices) return budget("patch_vertices", vertices.length, LIMITS.vertices);
  if (faces.length > LIMITS.triangles) return budget("patch_triangles", faces.length, LIMITS.triangles);
  for (const face of faces) if (Array.isArray(face) && face.length > LIMITS.faceVertices + 1) return budget("polygon_vertices", face.length, LIMITS.faceVertices);
  faces = faces.map((face) => Array.isArray(face) && face.length > 3 && face[0] === face[face.length - 1] ? face.slice(0, -1) : face);
  let count = 0;
  for (const face of faces) {
    if (!Array.isArray(face) || face.length < 3) return fail("invalid_data", { property: "faces" });
    if (face.length > LIMITS.faceVertices) return budget("polygon_vertices", face.length, LIMITS.faceVertices);
    count += face.length - 2;
    if (count > LIMITS.triangles) return budget("patch_triangles", count, LIMITS.triangles);
    for (let j = 0; j < face.length; j++) {
      const i = face[j];
      if (!Number.isInteger(i) || i < 0 || i >= vertices.length) return fail("invalid_index", { property: "faces" });
    }
  }
  for (let i = 0; i < vertices.length; i++) {
    const p = vertices[i];
    if (!Array.isArray(p) || p.length !== 3 || ![0, 1, 2].every((k) => Number.isFinite(p[k]))) return fail("invalid_data", { property: "vertices" });
  }
  const result = { triangles: [], faceOfTriangle: [], edges: [], code: "", args: {} };
  for (let i = 0; i < faces.length; i++) {
    const triangles = polygon(vertices, faces[i]);
    if (!triangles) return fail("unsupported_patch", { property: "polygon", face: i, detail: "invalid_geometry" });
    result.triangles.push(...triangles);
    result.faceOfTriangle.push(...triangles.map(() => i));
    for (let j = 0; j < faces[i].length; j++) result.edges.push([faces[i][j], faces[i][(j + 1) % faces[i].length], i]);
  }
  return result;
}
module.exports = { patchPoint, triangulatePatch, LIMITS, MAX_FACE_VERTICES };
