"use strict";

function fitFigureViewport(figureSize, width, height) {
  if (!(Number.isFinite(width) && width > 0 && Number.isFinite(height) && height > 0)) return { x: 0, y: 0, width: 0, height: 0 };
  if (!Array.isArray(figureSize) || figureSize.length !== 2 || !figureSize.every((value) => Number.isFinite(value) && value > 0)) return { x: 0, y: 0, width, height };
  const scale = Math.min(width / figureSize[0], height / figureSize[1]);
  const fittedWidth = figureSize[0] * scale, fittedHeight = figureSize[1] * scale;
  return { x: (width - fittedWidth) / 2, y: (height - fittedHeight) / 2, width: fittedWidth, height: fittedHeight };
}

function fitAxes(position, viewport, reserve = 0) {
  const fitted = position.slice();
  const scale = 1 - Math.max(0, Math.min(0.75, reserve));
  fitted[1] *= scale;
  fitted[3] *= scale;
  return { x: viewport.x + fitted[0] * viewport.width, y: viewport.y + (1 - fitted[1] - fitted[3]) * viewport.height, width: fitted[2] * viewport.width, height: fitted[3] * viewport.height };
}

// Both renderers use the complete figure to reserve shared-title space.
function fitFigureAxes(axes, viewport, figureAxes) {
  const base = axes.title_layout_position;
  if (!base) return fitAxes(axes.position, viewport);
  const sourceReserve = Math.max(0, 1 - axes.position[3] / base[3]);
  const title = figureAxes.flatMap((item) => item.series).find((series) => series.figure_title);
  const textHeight = title ? title.font_size * 1.2 * Math.max(1, title.lines.length) + title.margin * 2 + 36 : 0;
  const viewportReserve = viewport.height > 0 ? textHeight / viewport.height : 0;
  return fitAxes(base, viewport, Math.max(sourceReserve, viewportReserve));
}

module.exports = { fitFigureViewport, fitAxes, fitFigureAxes };
