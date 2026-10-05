const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const owned = [
  "frontend/files.js",
  "frontend/file_ops.js",
  "frontend/console.js",
  "frontend/command_window.js",
  "frontend/command_window_utils.cjs",
  "frontend/publish.js",
  "frontend/figures.js"
];
const localeFiles = ["frontend/locales/tr.js", "frontend/locales/tr_dosyalar.js"];
const translations = new Set();
for (const file of localeFiles) {
  const source = fs.readFileSync(path.join(root, file), "utf8");
  for (const match of source.matchAll(/^\s*"((?:\\.|[^"\\])+)"\s*:\s*"((?:\\.|[^"\\])*)"\s*,?$/gm)) {
    const value = JSON.parse(`"${match[2]}"`);
    assert.notEqual(value, "", `${file}: translation for ${match[1]} is empty`);
    translations.add(JSON.parse(`"${match[1]}"`));
  }
}

function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
}
function decodeLiteral(value) {
  return value.replace(/\\u([0-9a-fA-F]{4})/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
    .replace(/\\x([0-9a-fA-F]{2})/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)));
}
const turkish = /[çğıöşüİÇĞÖŞÜ]/;
for (const file of owned) {
  const original = fs.readFileSync(path.join(root, file), "utf8");
  const source = stripComments(original);
  const calls = [...source.matchAll(/\bt\(\s*(["'])(.*?)\1/g)].map((match) => match[2]);
  const attrs = [...source.matchAll(/data-i18n(?:-[a-z-]+)?=["']([^"']+)["']/g)].map((match) => match[1]);
  for (const key of [...calls, ...attrs]) assert(translations.has(key), `${file}: missing Turkish translation for ${key}`);
  const literals = source.matchAll(/(["'`])((?:\\.|(?!\1)[^\\])*)\1/g);
  for (const match of literals) {
    const decoded = decodeLiteral(match[2]);
    assert(!turkish.test(decoded), `${file}: Turkish-specific character remains in a string literal: ${match[0]}`);
  }
}
console.log("Lane D i18n coverage passed.");
