const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const owned = [
  "frontend/editor.js",
  "frontend/editor_commands.js",
  "frontend/editor_command_utils.cjs",
  "frontend/outline.js",
  "frontend/symbols.js",
  "frontend/editor_intel_utils.cjs",
  "frontend/editor_shortcuts.cjs",
  "frontend/editor_intel_shortcuts.cjs"
];
const locales = ["frontend/locales/tr.js", "frontend/locales/tr_editor.js"];
const localeMaps = locales.map((file) => {
  const source = fs.readFileSync(path.join(root, file), "utf8");
  const entries = [...source.matchAll(/^\s*"((?:\\.|[^"\\])+)"\s*:\s*"((?:\\.|[^"\\])*)"\s*,?$/gm)];
  for (const [, key, value] of entries) assert.notEqual(value, "", `${file}: empty translation for ${key}`);
  return new Map(entries.map(([, key, value]) => [key, value]));
});
const localeEntries = new Set(localeMaps.flatMap((map) => [...map.keys()]));
for (const [key, value] of localeMaps[0]) {
  if (localeMaps[1].has(key)) assert.equal(localeMaps[1].get(key), value, `conflicting translations for ${key}`);
}

function withoutComments(source) {
  return source.replace(/("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\/\/[^\n]*|\/\*[\s\S]*?\*\/)/g, (match, literal) => literal ? match : " ".repeat(match.length));
}

const allowedTurkishEscapes = new Map([
  ["frontend/editor.js", ["\\xE7al\\u0131\\u015Fma"]]
]);
const localeFailures = [];
const turkishFailures = [];
const turkishPattern = /[çğıöşüİÇĞÖŞÜ]|\\x(?:e7|f6|fc|c7|d6|dc)|\\u(?:00e7|011f|0131|00f6|015f|00fc|0130|00c7|011e|015e|00d6|00dc)/ig;

for (const file of owned) {
  const source = fs.readFileSync(path.join(root, file), "utf8");
  const visible = withoutComments(source);
  const keys = [
    ...[...visible.matchAll(/\bt\s*\(\s*(["'])(.*?)\1/g)].map((match) => match[2]),
    ...[...visible.matchAll(/data-i18n(?:-[\w-]+)?=["']([^"']+)["']/g)].map((match) => match[1])
  ];
  for (const key of keys) {
    if (!localeEntries.has(key)) localeFailures.push(`${file}: missing locale key ${JSON.stringify(key)}`);
  }
  let checked = visible;
  for (const allowed of allowedTurkishEscapes.get(file) || []) checked = checked.replace(allowed, "");
  for (const match of checked.matchAll(turkishPattern)) turkishFailures.push(`${file}: untranslated Turkish text ${JSON.stringify(match[0])}`);
}

assert.deepEqual(localeFailures, [], localeFailures.join("\n"));
assert.deepEqual(turkishFailures, [], turkishFailures.join("\n"));
console.log(`EDITOR I18N UNIT PASS: ${owned.length} files, ${localeEntries.size} locale keys, no untranslated Turkish literals.`);
