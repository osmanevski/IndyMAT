"use strict";

/* Pure scene builder. Input is an axes record from validate().data.axes; no
 * validation/admission is duplicated here. Undefined axes produces an empty
 * scene (for fixture iteration only; unsupported figures are NOT drawable).
 * Normalised coordinates follow figure_camera_utils, uploaded as Float32 xyz.
 * source.series is the ORIGINAL series record (double JSON numbers); never use
 * rounded positions for tips. series fields on drawable objects are zero-based
 * IDs. Mesh source.shape/cellOrigins reference original arrays; vertexIndices
 * maps each duplicated vertex to row+column*rows. cellOfTriangle contains grid
 * origins, NOT ordinal cell numbers. Four vertices per cell, source order
 * [i,i+1,i+rows,i+rows+1]; triangles [0,2,1],[1,2,3], two-sided. Axis reversals
 * can flip winding; R3 must disable culling. No triangulation diagonals in edges.
 * Mesh colors are RGB per duplicated vertex; edge colors RGB per endpoint.
 * None face/edge colours or none/zero-width edge styles omit that draw pass.
 * Lines compact finite samples, breaks=[runStart,...,FINAL_COUNT] (Uint32);
 * no run bridges a null XYZ. Stem baselines and stairs corners are expanded
 * for v3 axes containing legacy 2D series; source.pickableVertices marks real
 * samples (1) versus generated corners/baselines (0). sourceIndices map to original zero-based samples;
 * source.sampleIndices map to rendered record offsets (for original doubles).
 * points use same maps; sizes are points-squared for scatter, diameter points
 * for line markers (sizeUnits). width is points, not pixels; caller converts
 * points to pixels (usually 96/72) before expansion. markerFaceAuto is retained
 * for theme background selection. bounds={min,max} is the axes CLIPPING box.
 * counts distinguishes source grid samples (incl null) from uploaded duplicates.
 * surfacePoint(series,index) / samplePoint(series,index) read original triples.
 * expandPolyline(projected,widthPixels,breaks?,style='-') -> Float32 xyz triangle
 * vertices in TOP-LEFT pixel space, depth [0,1]; projected is [{x,y,depth}].
 * Butt-ended independent segment quads (no miter spikes), preserves run gaps,
 * skips zero-length segments, dash phase continuous within each finite run.
 * expandMarkers(projected,sizesPixels) -> {positions,uv,sourceIndices}: six
 * vertices per nonzero marker, sizesPixels scalar/array DIAMETERS, uv [-1,1]
 * for R3's marker shader; sourceIndices maps QUADS to projected offsets.
 * These helpers do not clip; R3 clips viewport and axes box in its shader.
 */
const { dataToNormalised, extents } = require("./figure_camera_utils.cjs");
const { resolveSurfaceColors } = require("./figure_color_utils.cjs");
// Grid vertex index -> original double xyz; vector and matrix layouts supported.
function surfacePoint(series, index) {
  const rows = series.shape[0];
  return [series.x.values[series.coordinate_layout.x === "vector" ? Math.floor(index / rows) : index],
    series.y.values[series.coordinate_layout.y === "vector" ? index % rows : index], series.z.values[index]];
}
// Rendered sample offset -> original double xyz (missing 2D Z defaults to zero).
function samplePoint(series, index) {
  return [series.x[index], series.y[index], series.z ? series.z[index] : 0];
}
const valid = (point) => point.every((v) => v !== null && Number.isFinite(v));
// Validated axes -> typed render buffers, original-record references and index maps.
function buildScene(axesRecord) {
  const extent = axesRecord ? extents(axesRecord) : [1, 1, 1];
  const scene = { meshes: [], edges: [], lines: [], points: [], axes: axesRecord,
    bounds: { min: extent.map((v) => -v), max: extent.slice() },
    counts: { vertices: 0, meshVertices: 0, cells: 0, triangles: 0, edges: 0, lineSamples: 0, points: 0 } };
  if (!axesRecord) return scene;
  for (let seriesIndex = 0; seriesIndex < axesRecord.series.length; seriesIndex++) {
    const s = axesRecord.series[seriesIndex];
    const series = s.id ?? seriesIndex;
    if (s.kind === "surface") {
      const rows = s.shape[0];
      const { faceColors, edgeColors } = resolveSurfaceColors(s, axesRecord);
      const owners = new Map(s.cell_origins.map((origin, index) => [origin, index]));
      const cells = s.cell_origins.filter((i) => [i, i + 1, i + rows, i + rows + 1].every((j) => valid(surfacePoint(s, j))));
      scene.counts.vertices += rows * s.shape[1];
      if (s.face_color.mode !== "none") {
        const positions = new Float32Array(cells.length * 12);
        const colors = new Float32Array(cells.length * 12);
        const indices = new Uint32Array(cells.length * 6);
        const vertexIndices = new Uint32Array(cells.length * 4);
        const cellOfTriangle = new Uint32Array(cells.length * 2);
        for (let cell = 0; cell < cells.length; cell++) {
          const i = cells[cell];
          const corners = [i, i + 1, i + rows, i + rows + 1];
          const owner = owners.get(i);
          for (let k = 0; k < 4; k++) {
            positions.set(dataToNormalised(axesRecord, surfacePoint(s, corners[k])), cell * 12 + k * 3);
            colors.set(faceColors.subarray(owner * 3, owner * 3 + 3), cell * 12 + k * 3);
            vertexIndices[cell * 4 + k] = corners[k];
          }
          indices.set([cell * 4, cell * 4 + 2, cell * 4 + 1, cell * 4 + 1, cell * 4 + 2, cell * 4 + 3], cell * 6);
          cellOfTriangle.set([i, i], cell * 2);
        }
        scene.meshes.push({ positions, colors, indices, cellOfTriangle, series,
          source: { series: s, shape: s.shape, cellOrigins: s.cell_origins, vertexIndices } });
        scene.counts.meshVertices += cells.length * 4;
        scene.counts.cells += cells.length;
        scene.counts.triangles += cells.length * 2;
      }
      if (s.edge_color.mode !== "none" && s.line_style !== "none" && s.line_width > 0) {
        const count = s.edge_indices.shape[0];
        const positions = new Float32Array(count * 6);
        const colors = new Float32Array(count * 6);
        const vertexIndices = new Uint32Array(count * 2);
        for (let i = 0; i < count; i++) {
          for (let end = 0; end < 2; end++) {
            const vertex = s.edge_indices.values[i + end * count];
            positions.set(dataToNormalised(axesRecord, surfacePoint(s, vertex)), i * 6 + end * 3);
            colors.set(edgeColors.subarray(i * 3, i * 3 + 3), i * 6 + end * 3);
            vertexIndices[i * 2 + end] = vertex;
          }
        }
        scene.edges.push({ positions, colors, series, width: s.line_width, style: s.line_style, faced: s.face_color.mode !== "none", source: { series: s, vertexIndices } });
        scene.counts.edges += count;
      }
      continue;
    }
    const coords = [];
    const sampleIndices = [];
    const sourceIndices = [];
    const breaks = [];
    const sizes = [];
    let inRun = false;
    scene.counts.vertices += s.x.length;
    for (let i = 0; i < s.x.length; i++) {
      const point = samplePoint(s, i);
      if (!valid(point)) {
        inRun = false;
        continue;
      }
      if (!inRun) breaks.push(sampleIndices.length);
      inRun = true;
      coords.push(...dataToNormalised(axesRecord, point));
      sampleIndices.push(i);
      // Missing mappings in legacy reduced v2 remain unknown; sentinel is NOT
      // an invented original index. V3 always supplies source_indices.
      sourceIndices.push(s.source_indices ? s.source_indices[i] : s.decimated ? 0xffffffff : i);
      sizes.push(s.kind === "scatter" ? (s.sizes?.[s.sizes.length === 1 ? 0 : i] ?? (s.marker_size ?? 6) ** 2) : (s.marker_size ?? 6));
    }
    breaks.push(sampleIndices.length);
    const positions = new Float32Array(coords);
    const sources = new Uint32Array(sourceIndices);
    const source = { series: s, sampleIndices: new Uint32Array(sampleIndices) };
    if (s.kind !== "scatter" && s.line_style !== "none" && s.line_color !== "none" && (s.line_width ?? 0.5) > 0) {
      const expanded = s.kind === "stem" || s.kind === "stairs" ? legacyLine(s, axesRecord, source.sampleIndices, breaks) :
        { positions, breaks: new Uint32Array(breaks), sourceIndices: sources, source };
      scene.lines.push({ ...expanded, color: s.line_color,
        width: s.line_width ?? 0.5, style: s.line_style, series });
      scene.counts.lineSamples += sampleIndices.length;
    }
    if (s.marker !== "none" && (s.marker_edge_color !== "none" || s.marker_face_color !== "none" || s.marker_face_auto)) {
      scene.points.push({ positions, sizes: new Float32Array(sizes), sizeUnits: s.kind === "scatter" ? "points-squared" : "points",
        sourceIndices: sources, marker: s.marker, edgeColor: s.marker_edge_color, faceColor: s.marker_face_color,
        markerFaceAuto: s.marker_face_auto, series, source });
      scene.counts.points += sampleIndices.length;
    }
  }
  return scene;
}
// Expand legacy 2D paths in normalised xyz, retaining only source samples as tip targets.
function legacyLine(series, axes, samples, runs) {
  const coords = [];
  const breaks = [];
  const sourceIndices = [];
  const sampleIndices = [];
  const pickableVertices = [];
  const emit = (point, sampleIndex, pickable) => {
    coords.push(...dataToNormalised(axes, point));
    sampleIndices.push(sampleIndex);
    sourceIndices.push(series.source_indices ? series.source_indices[sampleIndex] : series.decimated ? 0xffffffff : sampleIndex);
    pickableVertices.push(pickable ? 1 : 0);
  };
  for (let run = 0; run < runs.length - 1; run++) {
    if (series.kind === "stairs") breaks.push(sampleIndices.length);
    for (let i = runs[run]; i < runs[run + 1]; i++) {
      const sampleIndex = samples[i];
      const point = samplePoint(series, sampleIndex);
      if (series.kind === "stem") {
        breaks.push(sampleIndices.length);
        emit([point[0], series.base_value, point[2]], sampleIndex, false);
      } else if (i > runs[run]) {
        const previousIndex = samples[i - 1];
        const previous = samplePoint(series, previousIndex);
        emit([point[0], previous[1], previous[2]], previousIndex, false);
      }
      emit(point, sampleIndex, true);
    }
  }
  breaks.push(sampleIndices.length);
  return { positions: new Float32Array(coords), breaks: new Uint32Array(breaks), sourceIndices: new Uint32Array(sourceIndices),
    source: { series, sampleIndices: new Uint32Array(sampleIndices), pickableVertices: new Uint8Array(pickableVertices) } };
}
// Append six pixel xyz vertices for a butt-ended width-pixel segment.
function quad(a, b, width, output) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const length = Math.hypot(dx, dy);
  if (!(length > 0 && width > 0)) return;
  const nx = -dy / length * width / 2;
  const ny = dx / length * width / 2;
  const corners = [[a.x + nx, a.y + ny, a.depth], [a.x - nx, a.y - ny, a.depth],
    [b.x + nx, b.y + ny, b.depth], [b.x - nx, b.y - ny, b.depth]];
  for (const i of [0, 1, 2, 2, 1, 3]) output.push(...corners[i]);
}
// Projected pixel/depth points -> Float32 triangle vertices, preserving run breaks/dash phase.
function expandPolyline(projected, widthPixels, breaks = [0, projected.length], style = "-") {
  const output = [];
  const pattern = { "--": [7, 4], ":": [2, 3], "-.": [7, 3, 2, 3] }[style];
  if (style === "none" || !(widthPixels > 0)) return new Float32Array();
  for (let run = 0; run < breaks.length - 1; run++) {
    let phase = 0;
    for (let i = breaks[run] + 1; i < breaks[run + 1]; i++) {
      const a = projected[i - 1];
      const b = projected[i];
      if (!a || !b || ![a.x, a.y, a.depth, b.x, b.y, b.depth].every(Number.isFinite)) continue;
      if (!pattern) {
        quad(a, b, widthPixels, output);
        continue;
      }
      const distance = Math.hypot(b.x - a.x, b.y - a.y);
      const cycle = pattern.reduce((sum, v) => sum + v, 0);
      let offset = 0;
      while (offset < distance) {
        let remaining = phase % cycle;
        let part = 0;
        while (remaining >= pattern[part]) remaining -= pattern[part++];
        const step = Math.min(distance - offset, pattern[part] - remaining);
        const lerp = (t) => ({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, depth: a.depth + (b.depth - a.depth) * t });
        if (part % 2 === 0) quad(lerp(offset / distance), lerp((offset + step) / distance), widthPixels, output);
        offset += step;
        phase += step;
      }
    }
  }
  return new Float32Array(output);
}
// Projected pixels and diameter-pixel sizes -> Float32 quad xyz/uv and source maps.
function expandMarkers(projected, sizesPixels) {
  const positions = [];
  const uv = [];
  const sourceIndices = [];
  const corners = [[-1, -1], [1, -1], [-1, 1], [1, 1]];
  for (let i = 0; i < projected.length; i++) {
    const p = projected[i];
    const size = typeof sizesPixels === "number" ? sizesPixels : sizesPixels[i];
    if (!p || ![p.x, p.y, p.depth, size].every(Number.isFinite) || size <= 0) continue;
    for (const corner of [0, 1, 2, 2, 1, 3]) {
      const [x, y] = corners[corner];
      positions.push(p.x + x * size / 2, p.y + y * size / 2, p.depth);
      uv.push(x, y);
    }
    sourceIndices.push(i);
  }
  return { positions: new Float32Array(positions), uv: new Float32Array(uv), sourceIndices: new Uint32Array(sourceIndices) };
}
module.exports = { buildScene, surfacePoint, samplePoint, expandPolyline, expandMarkers };
