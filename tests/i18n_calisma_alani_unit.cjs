const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

const owned = [
  "frontend/workspace.js",
  "frontend/variable_editor.js",
  "frontend/variable_editor_utils.cjs",
  "frontend/history_panel.js",
  "frontend/debugger.js"
];

function readLocale(path) {
  const source = fs.readFileSync(path, "utf8").replace(/export default\s*/, "module.exports = ");
  const context = { module: { exports: {} } };
  vm.runInNewContext(source, context, { filename: path });
  return context.module.exports;
}

const translations = {
  ...readLocale("frontend/locales/tr.js"),
  ...readLocale("frontend/locales/tr_calisma_alani.js")
};

for (const [key, value] of Object.entries(translations)) {
  assert.equal(typeof key, "string");
  assert.equal(typeof value, "string");
  assert.ok(value.trim(), `empty Turkish translation for ${key}`);
}

const sources = new Set();
const turkishChars = /[çğıöşüİÇĞÖŞÜ]/;
const turkishEscapes = /\\(?:x(?:C7|E7|D6|F6|DC|FC|DE|FE)|u(?:00C7|00E7|011E|011F|0130|0131|00D6|00F6|015E|015F|00DC|00FC))/i;
const decodeLiteral = (quote, value) => quote === '"' ? JSON.parse(`"${value}"`) : value.replace(/\\'/g, "'");

for (const path of owned) {
  const text = fs.readFileSync(path, "utf8");
  const code = text.replace(/\/\*[\s\S]*?\*\/|(^|\s)\/\/.*$/gm, " ");
  for (const match of code.matchAll(/\b(?:t|translate)\(\s*(["'])(.*?)\1/g)) sources.add(decodeLiteral(match[1], match[2]));
  for (const match of code.matchAll(/\bdata-i18n(?:-[a-z-]+)?\s*=\s*(["'])(.*?)\1/g)) sources.add(decodeLiteral(match[1], match[2]));
  assert.ok(!turkishChars.test(code), `${path} contains a Turkish-specific character outside comments`);
  assert.ok(!turkishEscapes.test(code), `${path} contains a Turkish-specific escape outside comments`);
}

for (const source of sources) {
  assert.ok(Object.hasOwn(translations, source), `missing Turkish translation for ${JSON.stringify(source)}`);
  assert.ok(translations[source].trim(), `empty Turkish translation for ${JSON.stringify(source)}`);
}

console.log(`CALISMA ALANI I18N PASS: ${sources.size} translation sources covered; ${owned.length} owned files contain no Turkish-specific literals.`);
