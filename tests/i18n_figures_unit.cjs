"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { translate } = require("../frontend/i18n_utils.cjs");
const toolUtils = require("../frontend/figure_tool_utils.cjs");
const { REASON_CODES, validate } = require("../frontend/figure_data_utils.cjs");
const root = path.join(__dirname, "..");
const source = fs.readFileSync(path.join(root, "frontend/figure_tools.js"), "utf8");
const locale = {};
for (const filename of fs.readdirSync(path.join(root, "frontend/locales")).filter((name) => name.endsWith(".js"))) {
  const content = fs.readFileSync(path.join(root, "frontend/locales", filename), "utf8").replace(/^import .*$/gm, "").replace("export default", "return");
  Object.assign(locale, new Function(content)());
}
const keys = [...source.matchAll(/\bt\(\s*"((?:\\.|[^"\\])*)"/g)].map((match) => JSON.parse('"' + match[1] + '"'));
assert.equal(keys.length, [...source.matchAll(/\bt\(/g)].length, "every t() call must have a literal English key");
for (const key of keys) {
  assert.equal(typeof locale[key], "string", "missing translation: " + key);
  assert.equal(translate(key, {}, "en", locale), key);
  assert.notEqual(translate(key, {}, "tr", locale), key);
  const englishSlots = [...key.matchAll(/\{\w+\}/g)].map((match) => match[0]).sort();
  const turkishSlots = [...locale[key].matchAll(/\{\w+\}/g)].map((match) => match[0]).sort();
  assert.deepEqual(turkishSlots, englishSlots, "placeholder drift: " + key);
}
const html = fs.readFileSync(path.join(root, "static/index.html"), "utf8");
const controls = html.slice(html.indexOf('<div id="figure-tools"'), html.indexOf('<div id="plot-area"'));
assert(controls.startsWith('<div id="figure-tools" hidden>'));
assert(controls.includes('role="radiogroup"'));
assert.equal([...controls.matchAll(/type="radio"/g)].length, 4);
assert.equal([...controls.matchAll(/data-i18n-term=/g)].length, 5);
for (const match of controls.matchAll(/data-i18n(?:-[a-z-]+)?="([^"]+)"/g)) assert.equal(typeof locale[match[1]], "string", "missing control key: " + match[1]);
for (const [key, value] of Object.entries({ "Rotate 3D": "3B Döndür", Zoom: "Yakınlaştır", Pan: "Kaydır", "Data Tips": "Veri İpuçları", "Figure tools": "Grafik araçları", Colormap: "Renk haritası", Colorbar: "Renk çubuğu", Row: "Satır", Column: "Sütun", Index: "İndeks" })) assert.equal(locale[key], value);

// Load the ES module's pure exported text functions with an injected translator.
// This does not pretend to exercise browser DOM behavior.
let language = "en";
const moduleText = source.replace(/^import .*$/gm, "").replace(/^export /gm, "");
const api = new Function("tools", "t", "onLanguageChange", "applyStaticTranslations", moduleText + "\nreturn {figureReasonText, figureReductionText, figureCanvasDescription};")(
  toolUtils, (key, args) => translate(key, args, language, locale), () => () => {}, () => {}
);
const codes = [...REASON_CODES, "unknown_version", "invalid_index", "webgl_unavailable", "shader_failure", "context_lost"];
for (const code of codes) {
  const cases = [...source.matchAll(new RegExp('case "' + code + '": return t\\(', "g"))];
  assert.equal(cases.length, 1, "reason code must have one literal t() call: " + code);
  language = "en";
  const english = api.figureReasonText(code);
  language = "tr";
  const turkish = api.figureReasonText(code);
  assert(english.startsWith("Only the PNG view is available:"));
  assert(turkish.startsWith("Yalnızca PNG görünümü kullanılabilir:"));
  assert(!/\{\w+\}/.test(english + turkish), "unfilled optional reason argument: " + code);
}
let fixtureCount = 0;
for (const filename of fs.readdirSync(path.join(__dirname, "fixtures/figure_v3")).filter((name) => name.endsWith(".json"))) {
  const result = validate(JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures/figure_v3", filename), "utf8")));
  assert(result.ok, filename);
  if (result.supported) continue;
  fixtureCount++;
  for (language of ["en", "tr"]) {
    const text = api.figureReasonText(result.reason_code, result.reason_args);
    assert(!/\{\w+\}/.test(text), filename + ": unfilled reason arguments");
    if (language === "tr" || result.reason_code.includes("_")) assert(!text.includes(result.reason_code), filename + ": untranslated enum leaked into text");
  }
}
language = "en";
assert.equal(api.figureReductionText(5001, 2000), "Interactive data reduced from 5001 to 2000 points.");
assert(api.figureCanvasDescription().includes("Shift"));
assert(api.figureReasonText("budget_exceeded", { budget: "surface_vertices", actual: 40200, limit: 40000 }).includes("40200 > 40000"));
assert.equal(api.figureReasonText("future_unknown"), "Only the PNG view is available.");
language = "tr";
assert.equal(api.figureReductionText(5001, 2000), "Etkileşimli veri 5001 noktadan 2000 noktaya azaltıldı.");
assert(api.figureCanvasDescription().includes("Escape"));
assert.equal(api.figureReasonText("future_unknown"), "Yalnızca PNG görünümü kullanılabilir.");
assert(!/\b(?:fetch|XMLHttpRequest|WebSocket)\b|\/api\//.test(source), "figure tools must not acquire a network path");
console.log(`FIGURE I18N PASS: ${keys.length} literal keys, ${codes.length} reason codes, ${fixtureCount} real fallback fixtures, English/Turkish text.`);
