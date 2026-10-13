"use strict";

// Source-derived shader interface guard. This checks shader pairing and
// declarations passed to WebGL; it does not invoke a GLSL compiler.
const assert = require("node:assert/strict");
const fs = require("node:fs");

const renderer = fs.readFileSync("frontend/figure_webgl.js", "utf8");

function shaderSources(source) {
  const constants = new Map();
  for (const match of source.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*`([\s\S]*?)`\s*;/g)) {
    constants.set(match[1], match[2]);
  }
  return constants;
}

function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, "");
}

function parseShader(source, stage) {
  const clean = stripComments(source);
  const defaults = { float: stage === "vertex" ? "highp" : undefined,
    int: stage === "vertex" ? "highp" : undefined, sampler2D: "lowp", samplerCube: "lowp" };
  for (const [, precision, type] of clean.matchAll(/\bprecision\s+(lowp|mediump|highp)\s+(\w+)\s*;/g)) defaults[type] = precision;
  const precision = (explicit, type) => explicit || defaults[/^(?:[iu]vec\d|u?int|bool)$/.test(type) ? "int" :
    /^(?:vec\d|mat\d(?:x\d)?|float)$/.test(type) ? "float" : type];
  const uniforms = new Map();
  for (const [, declaration] of clean.matchAll(/^\s*uniform\s+([^;]+);/gm)) {
    const parsed = declaration.trim().match(/^(?:(lowp|mediump|highp)\s+)?(\w+)\s+(.+)$/);
    assert(parsed, "unparsed uniform declaration: " + declaration);
    const [, explicit, type, names] = parsed;
    for (const name of names.split(",")) {
      const item = name.trim().match(/^(\w+)(?:\s*\[\s*(\d+)\s*\])?$/);
      assert(item, "unparsed uniform name: " + name);
      uniforms.set(item[1], { type: type + (item[2] ? `[${item[2]}]` : ""), precision: precision(explicit, type) });
    }
  }
  const varyings = new Map();
  for (const [, interpolation, direction, explicit, type, names] of clean.matchAll(
    /^\s*(?:(flat|smooth|noperspective)\s+)?(in|out)\s+(?:(lowp|mediump|highp)\s+)?(\w+)\s+([^;]+);/gm)) {
    if ((stage === "vertex" && direction !== "out") || (stage === "fragment" && direction !== "in")) continue;
    for (const name of names.split(",")) {
      const item = name.trim().match(/^(\w+)(?:\s*\[\s*(\d+)\s*\])?$/);
      assert(item, "unparsed varying name: " + name);
      varyings.set(item[1], { type: type + (item[2] ? `[${item[2]}]` : ""),
        precision: precision(explicit, type), interpolation: interpolation || "smooth" });
    }
  }
  return { uniforms, varyings, clean };
}

function compareStages(vertexText, fragmentText) {
  const vertex = parseShader(vertexText, "vertex");
  const fragment = parseShader(fragmentText, "fragment");
  let sharedUniforms = 0;
  for (const [name, declaration] of vertex.uniforms) if (fragment.uniforms.has(name)) {
    sharedUniforms++;
    assert.deepEqual(declaration, fragment.uniforms.get(name), `shared uniform mismatch: ${name}`);
  }
  for (const [name, declaration] of vertex.varyings) {
    assert(fragment.varyings.has(name), `vertex varying missing from fragment input: ${name}`);
    assert.deepEqual(declaration, fragment.varyings.get(name), `varying interface mismatch: ${name}`);
  }
  for (const name of fragment.varyings.keys()) assert(vertex.varyings.has(name), `fragment input missing vertex output: ${name}`);
  return { sharedUniforms, varyingCount: vertex.varyings.size };
}

function discoverLinkedShaderSets(source, constants) {
  const found = [];
  // Follow the actual iterable consumed by shaderSource in each compile loop.
  // Entries can refer to const template strings or inline template literals.
  const loop = /for\s*\(\s*const\s*\[\s*\w+\s*,\s*(\w+)\s*\]\s*of\s*\[([\s\S]*?)\]\s*\)\s*\{/g;
  for (const [, sourceVar, body] of source.matchAll(loop)) {
    const compileLoop = source.slice(source.indexOf(body), source.indexOf(body) + body.length + 500);
    if (!compileLoop.includes(`shaderSource(`) || !compileLoop.includes(`${sourceVar})`)) continue;
    const entries = [];
    for (const [, stage, expression] of body.matchAll(/\[\s*[^,]+\.(VERTEX_SHADER|FRAGMENT_SHADER)\s*,\s*([^\]]+)\]/g)) {
      const expr = expression.trim().replace(/,$/, "");
      const named = expr.match(/^([A-Za-z_$][\w$]*)$/);
      const templated = expr.match(/^`([\s\S]*)`$/);
      let text;
      if (named) text = constants.get(named[1]);
      else if (templated) text = templated[1];
      else continue;
      assert.equal(typeof text, "string", `shader source expression not found: ${expr}`);
      entries.push({ stage, source: text });
    }
    if (entries.length) found.push(entries);
  }
  return found;
}

assert.match(renderer, /\.createShader\s*\(/, "renderer must create shaders");
assert.match(renderer, /\.compileShader\s*\(/, "renderer must compile shaders");
assert.match(renderer, /\.shaderSource\s*\(/, "renderer must supply shader source");
assert.match(renderer, /\.linkProgram\s*\(/, "renderer must link a program");
const constants = shaderSources(renderer);
const linkedSets = discoverLinkedShaderSets(renderer, constants);
assert(linkedSets.length > 0, "no shader pair could be derived from the actual compile/link path");
const countCalls = (name) => [...renderer.matchAll(new RegExp(`\\.${name}\\s*\\(`, "g"))].length;
assert.equal(countCalls("shaderSource"), linkedSets.length,
  "every shaderSource call site must belong to a source-derived compile loop");
assert.equal(countCalls("compileShader"), linkedSets.length,
  "every compileShader call site must belong to a source-derived compile loop");
assert.equal(countCalls("linkProgram"), linkedSets.length,
  "every link call must have a discovered source-derived shader set");
let pairs = 0;
for (const entries of linkedSets) {
  const vertex = entries.filter((entry) => entry.stage === "VERTEX_SHADER");
  const fragment = entries.filter((entry) => entry.stage === "FRAGMENT_SHADER");
  assert.equal(vertex.length, 1, "each linked program needs exactly one vertex shader source");
  assert.equal(fragment.length, 1, "each linked program needs exactly one fragment shader source");
  for (const entry of [...vertex, ...fragment]) {
    assert.match(entry.source, /^\s*#version\s+300\s+es\b/, "expected WebGL2 GLSL ES source");
    assert.equal((entry.source.match(/\{/g) || []).length, (entry.source.match(/\}/g) || []).length,
      "unbalanced shader braces (static syntax sanity check)");
  }
  const checked = compareStages(vertex[0].source, fragment[0].source);
  assert(checked.sharedUniforms > 0, "the linked stages should share at least one checked uniform");
  assert(checked.varyingCount > 0, "the linked stages should expose a checked varying interface");
  pairs++;
}

// Guard controls use copied source strings and prove that precision/type drift
// in either a shared uniform or an inter-stage varying is rejected.
const current = linkedSets[0];
const originalVertex = current.find((entry) => entry.stage === "VERTEX_SHADER").source;
const originalFragment = current.find((entry) => entry.stage === "FRAGMENT_SHADER").source;
assert.throws(() => compareStages(originalVertex,
  originalFragment.replace("smooth in vec3 ink;", "smooth in mediump vec3 ink;")), /ink/);
assert.throws(() => compareStages(originalVertex,
  originalFragment.replace("uniform highp int marker;", "uniform mediump int marker;")), /marker/);
assert.throws(() => compareStages(originalVertex,
  originalFragment.replace("smooth in vec3 ink;", "smooth in vec4 ink;")), /ink/);

console.log(`PASS source-derived shader guards: ${pairs} linked pair(s), shared uniforms and varying type/precision; mutation controls passed`);
console.log("NOTE static source analysis only; GLSL compiler/linker was not run.");
