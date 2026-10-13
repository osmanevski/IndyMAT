"use strict";
const assert = require("node:assert/strict");
const { Peeling, shaderSources } = require("../frontend/figure_peeling.cjs");
const { mock, actualSources } = require("./figure_peeling_harness.cjs");
const original = actualSources(), sources = shaderSources(original.vertex, original.fragment);
const gl = mock(), p = new Peeling(gl, sources, {});
// Inspect the actual sources and stage types supplied by the compile/link path.
const pairs = [...gl.links.values()].map(shaders => {
  assert.deepEqual(shaders.map(s => s.args[0]), [gl.VERTEX_SHADER, gl.FRAGMENT_SHADER]);
  return shaders.map(s => gl.shaders.get(s));
});
assert.equal(pairs.length, 3);
function declarations(source, stage) {
  source = source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, "");
  const defaults = { float: stage === "vertex" ? "highp" : undefined,
    int: stage === "vertex" ? "highp" : "mediump", sampler2D: "lowp" };
  for (const [, precision, type] of source.matchAll(/\bprecision\s+(lowp|mediump|highp)\s+(\w+)\s*;/g)) defaults[type] = precision;
  const result = [];
  for (const [, interpolation, storage, explicit, type, names] of source.matchAll(
    /(?:layout\s*\([^)]*\)\s*)?\b(?:(flat|smooth)\s+)?(uniform|in|out)\s+(?:(lowp|mediump|highp)\s+)?(\w+)\s+([^;]+);/g)) {
    const family = /^(?:u?int|[iu]vec[234]|bool)$/.test(type) ? "int" : /^(?:float|vec[234]|mat[234])$/.test(type) ? "float" : type;
    for (const name of names.split(",")) result.push({ name: name.trim(), storage, type,
      precision: explicit || defaults[family], interpolation: interpolation || "smooth" });
  }
  return result;
}
function check(vertexText, fragmentText) {
  const vertex = declarations(vertexText, "vertex"), fragment = declarations(fragmentText, "fragment");
  for (const f of fragment.filter(d => d.storage === "in" || d.storage === "uniform")) {
    const v = vertex.find(d => d.name === f.name && d.storage === (f.storage === "in" ? "out" : "uniform"));
    if (!v) { assert.notEqual(f.storage, "in", "missing vertex varying: " + f.name); continue; }
    for (const key of ["type", "precision", ...(f.storage === "in" ? ["interpolation"] : [])]) {
      assert.equal(v[key], f[key], `${f.storage} ${f.name} ${key}`);
    }
  }
}
for (const pair of pairs) {
  for (const source of pair) assert.match(source, /^#version 300 es/);
  check(...pair);
}
const [vertex, fragment] = pairs[0];
for (const [before, after, reason] of [
  ["uniform highp uint primitiveBase;", "uniform mediump uint primitiveBase;", /primitiveBase precision/],
  ["uniform highp uint primitiveBase;", "uniform highp float primitiveBase;", /primitiveBase type/],
  ["flat in highp uint primitiveID;", "smooth in highp uint primitiveID;", /primitiveID interpolation/],
  ["flat in highp uint primitiveID;", "flat in mediump uint primitiveID;", /primitiveID precision/],
  ["smooth in float opacity;", "smooth in vec2 opacity;", /opacity type/],
  ["smooth in float opacity;", "smooth in mediump float opacity;", /opacity precision/]
]) {
  assert(fragment.includes(before), "mutation anchor changed");
  assert.throws(() => check(vertex, fragment.replace(before, after)), reason);
}
assert.match(fragment, /uniform highp usampler2D previousKey/);
assert.match(fragment, /layout\(location=1\) out highp uvec2 outputKey/);
assert.match(fragment, /if \(opacity <= 0\.0/);
assert.match(fragment, /gl_FragDepth = float\(depth\)\/16777215\.0/);
assert(!fragment.includes("decoration"));
assert.equal(pairs[2][0], original.vertex); assert.equal(pairs[2][1], original.fragment);
p.dispose(); assert.equal(gl.live.size, 0);
console.log("PASS three source-derived linked shader pairs, six precision/type/interpolation mutation controls; static guards only, no GLSL compiler run.");
