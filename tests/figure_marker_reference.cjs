"use strict";

// Frozen pre-batching marker loop. Keep independent of the batched expansion
// so tests and CPU benchmarks can compare the previous draw/uniform path.
const loop = `      for (const points of scene.points) {
        const sizes = Array.from(points.sizes, (size) => (points.sizeUnits === "points-squared" ? Math.sqrt(size) : size) * 96 / 72);
        const projected = project(points.positions);
        // Individual quads preserve each marker's size and edge thickness.
        gl.uniform1i(u.marker, MARKERS[points.marker]);
        gl.uniform1i(u.hasFace, points.markerFaceAuto || points.faceColor !== "none");
        gl.uniform1i(u.hasEdge, points.edgeColor !== "none");
        gl.uniform3fv(u.faceInk, points.markerFaceAuto ? adjust(palette.background) : points.faceColor === "none" ? palette.text : adjust(points.faceColor));
        gl.uniform3fv(u.edgeInk, points.edgeColor === "none" ? palette.text : adjust(points.edgeColor));
        for (let i = 0; i < sizes.length; i++) {
          const p = projected[i];
          const centre = points.positions.subarray(i * 3, i * 3 + 3);
          // Float32 centres against float64 limits: a marker exactly on a limit stays.
          if (centre.some((v, k) => v < scene.bounds.min[k] - 0.00001 || v > scene.bounds.max[k] + 0.00001)) continue;
          const size = sizes[i];
          if (!(size > 0) || ![p.x, p.y, p.depth, size].every(Number.isFinite)) continue;
          gl.uniform1f(u.border, Math.min(1, 2 * 96 / 72 / Math.max(1, sizes[i])));
          // Clip marker centres, not their pixel offsets, to retain edge glyphs.
          gl.uniform3fv(u.origin, centre);
          gl.uniform3fv(u.pixelX, [0, 0, 0]);
          gl.uniform3fv(u.pixelY, [0, 0, 0]);
          gl.uniform3fv(u.depthVector, [0, 0, 0]);
          // Each marker still has its own draw/uniforms. Reuse the same six
          // vertices and UVs; bufferData copies them before the next marker.
          for (let vertex = 0; vertex < 6; vertex++) {
            this.markerPositions[vertex * 3] = p.x + this.markerUV[vertex * 2] * size / 2;
            this.markerPositions[vertex * 3 + 1] = p.y + this.markerUV[vertex * 2 + 1] * size / 2;
            this.markerPositions[vertex * 3 + 2] = p.depth;
          }
          this.solidMarker({ positions: this.markerPositions, uv: this.markerUV });
        }
      }`;

function unbatchedSource(source) {
  const start = source.indexOf("      for (const points of scene.points) {");
  const end = source.indexOf("\n      gl.uniform1i(u.marker, 0);", start);
  if (start < 0 || end < 0) throw new Error("Marker reference boundaries changed");
  return (source.slice(0, start) + loop + source.slice(end))
    .replace("world = marker > 0 ? markerData.xyz : screen ?", "world = screen ?")
    .replace("uniform int marker;\nuniform vec2 viewport;", "uniform int marker;\nuniform float border;\nuniform vec2 viewport;")
    .replace("markerBorder = markerData.w;", "markerBorder = border;")
    .replace('"hasEdge", "clip"', '"hasEdge", "border", "clip"')
    .replace("this.markerPositions = new Float32Array();", "this.markerPositions = new Float32Array(18);")
    .replace("this.markerUV = new Float32Array();", "this.markerUV = new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]);");
}
module.exports = { unbatchedSource };
