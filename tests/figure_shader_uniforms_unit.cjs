"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");

// Parse the actual GLSL template sources, independent of the mocked GL linker.
const source = fs.readFileSync("frontend/figure_webgl.js", "utf8");
const shader = (name) => {
  const match = source.match(new RegExp("const " + name + " = `([\\s\\S]*?)`;"));
  assert(match, name + " shader template missing");
  return match[1];
};
function uniforms(source, stage) {
  source = source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, "");
  const defaults = { int: stage === "vertex" ? "highp" : "mediump",
    float: stage === "vertex" ? "highp" : undefined, sampler2D: "lowp", samplerCube: "lowp" };
  for (const [, precision, type] of source.matchAll(/\bprecision\s+(lowp|mediump|highp)\s+(\w+)\s*;/g)) defaults[type] = precision;
  const result = new Map();
  for (const [, declaration] of source.matchAll(/\buniform\s+([^;]+);/g)) {
    const match = declaration.match(/^(?:(lowp|mediump|highp)\s+)?(\w+)\s+(.+)$/);
    assert(match, "unparsed uniform: " + declaration);
    const [, explicit, type, names] = match;
    const family = /^(?:[iu]vec\d|u?int)$/.test(type) ? "int" :
      /^(?:vec\d|mat\d(?:x\d)?|float)$/.test(type) ? "float" : type;
    const precision = explicit || defaults[family];
    for (const name of names.split(",")) {
      const parsed = name.trim().match(/^(\w+)(\s*\[[^\]]+\])?$/);
      assert(parsed, "unparsed uniform name: " + name);
      result.set(parsed[1], { type: type + (parsed[2] || "").replace(/\s/g, ""), precision });
    }
  }
  return result;
}
function check(vertex, fragment) {
  const v = uniforms(vertex, "vertex"), f = uniforms(fragment, "fragment");
  let shared = 0;
  for (const [name, definition] of v) if (f.has(name)) {
    shared++;
    assert.deepEqual(definition, f.get(name), "shared uniform mismatch: " + name);
  }
  return shared;
}
assert(check(shader("VERTEX"), shader("FRAGMENT")) > 0);
assert.throws(() => check("uniform int marker;", "uniform int marker;"), /marker/);
assert.throws(() => check("uniform highp int marker;", "uniform mediump int marker;"), /marker/);
assert.throws(() => check("uniform highp int marker;", "uniform highp float marker;"), /marker/);
assert.equal(check("precision highp float; uniform vec3 a, b;", "precision highp float; uniform vec3 b, a;"), 2);
assert.equal(check("uniform int a;", "uniform highp int a;"), 1);
assert.throws(() => check("uniform highp vec3 a[2];", "uniform highp vec3 a[3];"), /a/);
console.log("PASS shader shared uniforms: types, array shapes and effective precision; mismatch controls");
