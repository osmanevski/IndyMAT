"use strict";
// Block diagram editor language contract: every t() call takes a literal English
// source that has a Turkish text, and the editor sources contain no Turkish copy.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const root = path.resolve(__dirname, "..");
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
const locale = (file) => vm.runInNewContext("(" + read(file).replace(/^\s*\/\/[^\n]*\n/, "").replace(/^export default\s*/, "").replace(/;\s*$/, "") + ")");
const blockTable = locale("frontend/locales/tr_blok.js");
const merged = Object.assign({}, ...fs.readdirSync(path.join(root, "frontend/locales")).filter((f) => f.endsWith(".js")).map((f) => locale("frontend/locales/" + f)));
const withoutComments = (source) => source.replace(/("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\/\/[^\n]*|\/\*[\s\S]*?\*\/)/g, (match, literal) => literal ? match : " ".repeat(match.length));

const editor = withoutComments(read("frontend/block_editor.js"));
const literal = [...editor.matchAll(/\bt\(\s*"((?:\\.|[^"\\])*)"/g)].map((m) => m[1]);
const all = [...editor.matchAll(/\bt\(/g)].length;
assert.equal(literal.length, all, "every t() call in block_editor.js takes a literal key");
for (const key of literal) assert.ok(Object.hasOwn(merged, key) && merged[key], "missing Turkish text for " + JSON.stringify(key));
for (const [key, value] of Object.entries(blockTable)) {
  assert.ok(typeof value === "string" && value.length, "empty Turkish text for " + key);
  const placeholders = (s) => (s.match(/\{[A-Za-z_][A-Za-z0-9_]*\}/g) || []).sort().join();
  assert.equal(placeholders(value), placeholders(key), "placeholders differ for " + key);
  assert.ok(literal.includes(key), "unused Turkish entry " + JSON.stringify(key));
}
const turkish = /[çğıöşüİÇĞÖŞÜ]/;
for (const file of ["frontend/block_editor.js", "frontend/block_model_utils.cjs"]) assert.ok(!turkish.test(withoutComments(read(file))), file + " contains Turkish copy outside the locale table");
// Wording: no compatibility claim with another tool anywhere in the editor.
for (const file of ["frontend/block_editor.js", "frontend/block_model_utils.cjs", "frontend/locales/tr_blok.js"]) assert.ok(!/simulink|\.slx/i.test(read(file)), file + " names another tool");
console.log(`BLOCK EDITOR I18N PASS: ${literal.length} literal keys, ${Object.keys(blockTable).length} Turkish entries.`);
