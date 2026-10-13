"use strict";
const assert = require("node:assert/strict");
const { expandMesh, capture, shaderSources, LIMITS } = require("../frontend/figure_peeling.cjs");
const { mock, actualSources } = require("./figure_peeling_harness.cjs");
const mesh = { positions: new Float32Array([1, 2, 3, 4, 5, 6, 7, 8, 9]),
  colors: new Float32Array([.1, .2, .3, .4, .5, .6, .7, .8, .9]),
  alphas: new Float32Array([0, .5, 1]), indices: new Uint32Array([2, 1, 0, 0, 1, 2]) };
const expanded = expandMesh(mesh);
assert.deepEqual([...expanded.positions], [7, 8, 9, 4, 5, 6, 1, 2, 3, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
assert.deepEqual([...expanded.alphas], [1, .5, 0, 0, .5, 1]);
assert.deepEqual([...expanded.indices], [0, 1, 2, 3, 4, 5]);
assert.deepEqual([...mesh.indices], [2, 1, 0, 0, 1, 2]);
assert.equal(expandMesh({ ...mesh, alphas: undefined }).alphas, undefined);
const plain = { positions: mesh.positions }; assert.equal(expandMesh(plain), plain);

const gl = mock(), dynamic = { vao: {}, attributes: ["position", "color", "uv", "marker", "alpha"] };
const uniforms = { marker: "marker", clip: "clip" }, values = new Float32Array([.2, .5, .9]);
const captured = capture(gl, uniforms, dynamic, proxy => {
  proxy.viewport(1, 2, 30, 40); proxy.bindVertexArray(dynamic.vao);
  proxy.uniform1i(uniforms.marker, 0); proxy.disable(gl.DEPTH_TEST);
  for (let i = 0; i < 3; i++) { proxy.bindBuffer(gl.ARRAY_BUFFER, dynamic.attributes[i]); proxy.bufferData(gl.ARRAY_BUFFER, values); }
  proxy.drawArrays(gl.TRIANGLES, 0, 3);
  proxy.enable(gl.DEPTH_TEST); proxy.enableVertexAttribArray(4);
  proxy.bindBuffer(gl.ARRAY_BUFFER, dynamic.attributes[4]); proxy.bufferData(gl.ARRAY_BUFFER, values);
  proxy.uniform1i(uniforms.clip, 1); proxy.drawArrays(gl.TRIANGLES, 0, 3);
  proxy.disableVertexAttribArray(4); proxy.drawArrays(gl.TRIANGLES, 0, 3);
});
values.fill(1);
assert.equal(gl.calls.length, 0, "capture must not render");
assert.deepEqual(captured[0].draws.map(d => d.primitiveBase), [1, 2, 3]);
assert.deepEqual(captured[0].draws.map(d => d.decoration), [true, false, false]);
assert.equal(captured[0].draws[0].attributes[4], null);
assert.deepEqual(captured[0].draws[1].attributes[4], new Float32Array([.2, .5, .9]));
assert.equal(captured[0].draws[2].attributes[4], null);
assert(!captured[0].draws[0].uniforms.has("clip"));
assert.equal(captured[0].draws[1].uniforms.get("clip").args[0], 1);

// Execute the two actual shader discard expressions on synthetic pixel
// fragments. This verifies contribution/tie rules, not rasterization or GLSL.
const original = actualSources(), fragment = shaderSources(original.vertex, original.fragment).fragment;
const alphaRule = fragment.match(/if \((opacity <=[^\n]+)\) discard;/)[1];
const depthRule = fragment.match(/if \((previous\.x ==[^\n]+)\) discard;/)[1];
const js = expression => expression.replace(/\b(0x[\da-f]+|\d+)u\b/g, "$1");
const alphaDiscard = new Function("opacity", "primitiveID", "primitiveBase", "return " + js(alphaRule));
const depthDiscard = new Function("previous", "depth", "primitiveID", "return " + js(depthRule));
function peel(fragments) {
  // IDs are canonical before any arbitrary storage permutation.
  const submission = [...fragments].sort((a, b) => a.id - b.id), output = [];
  let previous;
  for (let layer = 0; layer <= LIMITS.layers; layer++) {
    let best;
    for (const f of submission) {
      if (alphaDiscard(f.alpha, f.id, 1) || previous && depthDiscard(previous, f.depth, f.id)) continue;
      if (!best || f.depth <= best.depth) best = f;
    }
    if (!best) break;
    output.push(best);
    previous = { x: best.depth, y: (best.id | (best.alpha >= 1 ? 0x80000000 : 0)) >>> 0 };
  }
  return { output, overflow: output.length > LIMITS.layers };
}
const item = (id, depth, alpha = .5) => ({ id, depth, alpha });
assert.deepEqual(peel([item(1, 20), item(2, 20), item(3, 50)]).output.map(f => f.id), [2, 1, 3]);
assert.deepEqual(peel([item(3, 50), item(2, 20), item(1, 20)]).output.map(f => f.id), [2, 1, 3]);
assert.deepEqual(peel([item(1, 0, 0), item(2, 10), item(3, 20, 1), item(4, 30)]).output.map(f => f.id), [2, 3]);
const eight = Array.from({ length: 8 }, (_, i) => item(i + 1, i * 10));
assert.equal(peel(eight).overflow, false);
assert.equal(peel([...eight, item(9, 90)]).overflow, true);
assert.equal(peel([...eight, item(9, 90, 1)]).overflow, true, "terminal opaque geometry counts toward eight");
assert.equal(peel([...eight, item(9, 90, 0)]).overflow, false, "zero alpha does not count");
assert.deepEqual(peel([item(99, 0, 1), ...eight.slice(1)]).output.map(f => f.id), [99]);
assert.equal(depthDiscard({ x: 0xffffffff, y: 0xffffffff }, 10, 1), true);
console.log("PASS indexed alpha expansion, immutable capture, source-derived fragment discard/tie/occlusion and 8/9-layer boundary oracles; no rasterization run.");
