"use strict";
// Validator mutations of a recorded real Octave fixture; no DOM or runtime.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { validate } = require("../frontend/figure_data_utils.cjs");
const fixture = JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/text_boxes.json", "utf8"));
const copy = (value) => JSON.parse(JSON.stringify(value));
const text = (data) => data.axes.flatMap((axis) => axis.series).find((series) => series.kind === "text");
assert.equal(validate(fixture).ok, true);
const annotated = copy(fixture);
annotated.source.figure_size = [640, 480];
annotated.axes[0].title_layout_position = [.13, .11, .775, .815];
text(annotated).figure_title = true;
const before = copy(annotated);
assert.equal(validate(annotated).ok, true);
assert.deepEqual(annotated, before, "validator changed metadata");
assert.equal(validate(fixture).ok, true, "older artifact without metadata rejected");
for (const position of [null, [0, 0, 1], [0, 0, 0, 1], [0, 0, 1, -1], [0, 0, 1, Infinity], ["0", 0, 1, 1]]) {
  const data = copy(annotated);
  data.axes[0].title_layout_position = position;
  assert.equal(validate(data).ok, false, "malformed managed position accepted");
}
for (const size of [null, [640], [640, 0], [-1, 480], [640, NaN], [640, 480, 1], ["640", 480]]) {
  const data = copy(annotated);
  data.source.figure_size = size;
  assert.equal(validate(data).ok, false, "malformed source figure size accepted");
}
for (const marker of [1, "true", null, {}, []]) {
  const data = copy(annotated);
  text(data).figure_title = marker;
  assert.equal(validate(data).ok, false, "malformed title marker accepted");
}
console.log("SGTITLE METADATA UNIT PASS: optional managed layout/title/source-size fields, older artifacts, malformed values, no mutation.");
