"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { createHash } = require("node:crypto");
const { validate } = require("../frontend/figure_data_utils.cjs");
const { buildScene } = require("../frontend/figure_geometry_utils.cjs");

// Frozen from 346ea65 BEFORE the measured interp-diagonal follow-up. Hash the
// actual typed-array bytes, including flat/constant nonplanar surfaces, missing
// cells and mesh edges. These are buffer regressions, not pixel measurements.
const baseline = {
  surf_vector: "a8f60bbc1a5de91d5d7fa6c5474173dbe9721952d407138c90f3afddb685775a",
  surf_matrix_nan: "8562b4d036d9cd19d84985f55a610da8ce6db4e9c938fbd46aead639f9596bcb",
  surf_integer_clim: "490b46fd68220b5dcbe173bbdd88bbf10d8ff3926b65a8fa6d1087ea56ed3513",
  mesh_default: "ab5144fcc301a46b5b6e3a6a65d4d252dfda15aa620016154f40a799bd0742c5",
  colorbar: "a8f60bbc1a5de91d5d7fa6c5474173dbe9721952d407138c90f3afddb685775a"
};
for (const [name, expected] of Object.entries(baseline)) {
  const input = JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", name + ".json")));
  const result = validate(input);
  assert(result.ok && result.supported, name);
  const scene = buildScene(result.data.axes[0]);
  assert(scene.meshes.length > 0, name + ": empty regression");
  const bytes = [];
  for (const mesh of scene.meshes) {
    assert(["flat", "constant"].includes(mesh.source.series.face_color.mode));
    for (const array of [mesh.positions, mesh.colors, mesh.indices, mesh.cellOfTriangle, mesh.source.vertexIndices]) {
      bytes.push(Buffer.from(array.buffer, array.byteOffset, array.byteLength));
    }
  }
  for (const edge of scene.edges) {
    for (const array of [edge.positions, edge.colors, edge.source.vertexIndices]) {
      bytes.push(Buffer.from(array.buffer, array.byteOffset, array.byteLength));
    }
  }
  assert.equal(createHash("sha256").update(Buffer.concat(bytes)).digest("hex"), expected,
    name + ": flat/constant face or edge buffer bytes changed");
}
console.log("5 flat/constant surface buffer snapshots are byte-identical to 346ea65.");
