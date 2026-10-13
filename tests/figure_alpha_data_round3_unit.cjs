"use strict";
const assert = require("node:assert/strict");
const { validateAlphaDescriptor, alphaAt } = require("../frontend/figure_alpha_utils.cjs");
assert.equal(alphaAt(undefined, 12), 1);
assert.equal(validateAlphaDescriptor(undefined).ok, true);
for (const value of [0, 0.25, 0.75, 1]) {
  const d = { mode: "constant", association: "constant", value };
  assert.equal(validateAlphaDescriptor(d).ok, true);
  assert.equal(alphaAt(d, 40), value);
}
for (const value of [-1, 2, NaN, Infinity, "0.5", null])
  assert.equal(validateAlphaDescriptor({ mode: "constant", association: "constant", value }).ok, false);
for (const mode of ["flat", "interp", "texturemap"])
  assert.equal(validateAlphaDescriptor({ mode, association: "vertex", values: [0, 0.5, 1] }, 3).reason_code, "transparency");
assert.throws(() => alphaAt({ mode: "interp" }, 0));
console.log("PASS scalar alpha admission and explicit unmeasured data fallback");
