const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

const owned = [
  "frontend/settings.js",
  "frontend/shortcut_registry.js",
  "frontend/shortcut_registry_utils.cjs",
  "frontend/dialogs.js",
  "frontend/bootstrap.js",
  "frontend/core.js",
  "frontend/poll.js",
  "frontend/app.js",
  "frontend/state.js"
];
const localeFiles = ["frontend/locales/tr.js", "frontend/locales/tr_ayarlar.js", "frontend/locales/tr_assistant.js"];
function locale(file) {
  const source = fs.readFileSync(file, "utf8").replace(/^\s*\/\/[^\n]*\n/, "").replace(/^export default\s*/, "").replace(/;\s*$/, "");
  return vm.runInNewContext(`(${source})`);
}
const translations = Object.assign({}, ...localeFiles.map(locale));
const turkish = /[çğıöşüİÇĞÖŞÜ]|\\x(?:[0-9a-f]{2})|\\u(?:[0-9a-f]{4})/i;
const allowedTurkish = new Map([
  // These two event.key literals detect Turkish keyboard layouts; they are input values, not UI copy.
  ["frontend/shortcut_registry_utils.cjs", new Set(['  if (event.key === "ı" || event.key === "İ") return "KeyI";'])],
  // The language name "Türkçe" is shown in its own language and is never translated.
  ["frontend/settings.js", new Set(["const LANGUAGE_NAME_TR = \"T\\u00FCrk\\u00E7e\";"])]
]);

for (const file of owned) {
  const source = fs.readFileSync(file, "utf8");
  const calls = [...source.matchAll(/\bt\(\s*(["'`])((?:\\.|(?!\1)[\s\S])*)\1/g)].map((match) => match[2]);
  for (const key of calls) {
    assert.ok(Object.hasOwn(translations, key), `${file}: missing translation for ${JSON.stringify(key)}`);
    assert.notEqual(translations[key], "", `${file}: empty translation for ${JSON.stringify(key)}`);
  }
  const attrs = [...source.matchAll(/data-i18n(?:-(?:title|placeholder|aria-label|term))?=["']([^"']+)["']/g)].map((match) => match[1]);
  for (const key of attrs) {
    assert.ok(Object.hasOwn(translations, key), `${file}: missing attribute translation for ${JSON.stringify(key)}`);
    assert.notEqual(translations[key], "", `${file}: empty attribute translation for ${JSON.stringify(key)}`);
  }
  for (const [index, line] of source.split(/\r?\n/).entries()) {
    if (!turkish.test(line)) continue;
    // Allowed lines are named by their text, not their number: the files grow.
    assert.ok(allowedTurkish.get(file)?.has(line), `${file}:${index + 1} contains Turkish-specific text outside comments`);
  }
}

for (const [key, value] of Object.entries(localeFiles.map(locale)[1])) assert.ok(typeof value === "string" && value.length > 0, `empty or invalid Turkish value for ${key}`);
console.log("Lane C i18n source and locale checks passed.");
