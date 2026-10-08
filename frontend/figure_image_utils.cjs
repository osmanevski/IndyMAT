"use strict";
const colors = require("./figure_color_utils.cjs");
// First/last pixel centers, including descending extents, become outer edges.
function bounds(centers, count) {
  const step = count > 1 ? (centers[1] - centers[0]) / (count - 1) : 1;
  return [centers[0] - step / 2, centers[1] + step / 2];
}
function rgba(image, axes) {
  const [rows, columns] = image.shape, values = new Uint8ClampedArray(rows * columns * 4);
  const descriptor = { mode: "data", encoding: image.encoding, mapping: image.mapping, cdata_class: image.cdata_class, data: image.cdata };
  for (let row = 0; row < rows; row++) for (let col = 0; col < columns; col++) {
    const rgb = colors.descriptorColor(descriptor, row + col * rows, axes), offset = (row * columns + col) * 4;
    for (let k = 0; k < 3; k++) values[offset + k] = Math.round(Math.max(0, Math.min(1, rgb[k])) * 255);
    values[offset + 3] = 255;
  }
  return values;
}
function pick(image, x, y) {
  const [rows, columns] = image.shape, bx = bounds(image.x, columns), by = bounds(image.y, rows);
  const u = (x - bx[0]) / (bx[1] - bx[0]), v = (y - by[0]) / (by[1] - by[0]);
  if (u < 0 || u >= 1 || v < 0 || v >= 1) return null;
  const column = Math.floor(u * columns), row = Math.floor(v * rows), index = row + column * rows;
  const value = image.encoding === "truecolor" ? [0, 1, 2].map((k) => image.cdata.values[index + k * rows * columns]) : image.cdata.values[index];
  return { row, column, value };
}
function aspectBox(box, axes, view) {
  let ratio = null;
  if (axes.data_aspect_ratio_mode === "manual") ratio = ((view.x[1] - view.x[0]) / axes.data_aspect_ratio[0]) / ((view.y[1] - view.y[0]) / axes.data_aspect_ratio[1]);
  else if (axes.plot_box_aspect_ratio_mode === "manual") ratio = axes.plot_box_aspect_ratio[0] / axes.plot_box_aspect_ratio[1];
  if (!(ratio > 0) || !Number.isFinite(ratio)) return box;
  const w = Math.min(box.w, box.h * ratio), h = Math.min(box.h, box.w / ratio);
  return { x: box.x + (box.w - w) / 2, y: box.y + (box.h - h) / 2, w, h };
}
module.exports = { bounds, rgba, pick, aspectBox };
