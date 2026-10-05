const assert = require("node:assert/strict");
const fs = require("node:fs");
const utils = require("../frontend/i18n_utils.cjs");
const settings = require("../frontend/shortcut_registry_utils.cjs");
const turkish = JSON.parse(fs.readFileSync(require.resolve("../frontend/locales/tr.js"), "utf8").replace(/^export default /, "").replace(/;\s*$/, ""));

assert.equal(utils.resolveLanguage(), "en");
assert.equal(utils.resolveLanguage("system", ["tr-TR", "en-US"]), "tr");
assert.equal(utils.resolveLanguage("system", ["en-US", "tr-TR"]), "en");
assert.equal(utils.resolveLanguage("system", ["de-DE", "TR"]), "tr");
assert.equal(utils.resolveLanguage("system", ["fr-FR"]), "en");
assert.equal(utils.resolveLanguage("tr", ["en-US"]), "tr");
assert.equal(utils.resolveLanguage("en", ["tr-TR"]), "en");
assert.equal(utils.resolveLanguage("invalid", null), "en");
assert.equal(utils.translate("Run", {}, "tr", turkish), "Çalıştır");
assert.equal(utils.translate("Run", {}, "en", turkish), "Run");
assert.equal(utils.translate("Missing {count}", { count: 60 }, "tr", turkish), "Missing 60");
assert.equal(utils.translate("{count} lines", { count: 60 }, "tr", { "{count} lines": "{count} satır" }), "60 satır");
assert.equal(utils.translate("{count} lines", {}, "tr", {}), "{count} lines");
assert.equal(utils.translate("{value} / {value}", { value: "$& <script>" }), "$& <script> / $& <script>");
assert.equal(utils.translate("toString", {}, "tr", {}), "toString");
assert.equal(utils.translate("X", {}, "tr", { X: null }), "X");
assert.equal(utils.translate("{zero}:{no}:{empty}", { zero: 0, no: false, empty: "" }), "0:false:");

const text = (value) => ({ nodeType: 3, textContent: value });
class Element {
  constructor(attributes = {}, children = []) {
    this.nodeType = 1;
    this.attributes = { ...attributes };
    this.childNodes = children;
  }
  getAttribute(name) { return Object.hasOwn(this.attributes, name) ? this.attributes[name] : null; }
  setAttribute(name, value) { this.attributes[name] = value; }
  removeAttribute(name) { delete this.attributes[name]; }
  matches(selector) { return selector.split(",").some((part) => this.getAttribute(part.trim().slice(1, -1)) !== null); }
  querySelectorAll(selector) {
    return this.childNodes.filter((node) => node.nodeType === 1).flatMap((node) => [...(node.matches(selector) ? [node] : []), ...node.querySelectorAll(selector)]);
  }
  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
  get textContent() { return this.childNodes.map((node) => node.textContent).join(""); }
  set textContent(value) { this.childNodes = [text(value)]; }
}
const icon = new Element({}, []);
const run = new Element({ "data-i18n": "Run", "data-i18n-title": "Run", "data-i18n-aria-label": "Run", "data-i18n-term": "Run" }, [icon, text("Run")]);
const section = new Element({ "data-i18n": "Run Section", "data-i18n-title": "Run the %% section at the cursor (⌘Enter)", "data-i18n-term": "Run Section" }, [text("Run Section")]);
const menuLabel = new Element({ "data-i18n": "Run Current Section" }, [text("Run Current Section")]);
const menu = new Element({ "data-i18n-term": "Run Section" }, [menuLabel, new Element({}, [text("⌘↵")])]);
const input = new Element({ "data-i18n-placeholder": "Search", "data-i18n-aria-label": "Search variables" });
const count = new Element({}, [text("7")]);
const heading = new Element({ "data-i18n": "Workspace " }, [text("Workspace "), count]);
const plotSource = "No figures yet.{i18nChild0}{i18nChild1}, {i18nChild2} or {i18nChild3} will appear here when run.";
const plotChildren = [new Element(), new Element({}, [text("plot")]), new Element({}, [text("surf")]), new Element({}, [text("imagesc")])];
const plot = new Element({ "data-i18n": plotSource }, [text("No figures yet."), plotChildren[0], plotChildren[1], text(", "), plotChildren[2], text(" or "), plotChildren[3], text(" will appear here when run.")]);
const root = new Element({}, [run, section, menu, input, heading, plot]);
for (let i = 0; i < 3; i++) utils.applyStaticTranslations(root, "tr", turkish);
assert.equal(run.textContent, "Çalıştır");
assert.equal(run.childNodes[0], icon);
assert.equal(run.getAttribute("title"), "Çalıştır — Run");
assert.equal(run.getAttribute("aria-label"), "Çalıştır");
assert(section.getAttribute("title").endsWith(" — Run Section"));
assert.equal(menu.getAttribute("title"), "Bölümü çalıştır — Run Section");
assert.equal(input.getAttribute("placeholder"), "Ara");
assert.equal(input.getAttribute("aria-label"), "Değişken ara");
assert.equal(heading.textContent, "Çalışma alanı 7");
assert.equal(heading.childNodes[1], count);
assert.equal(plot.textContent, "Henüz grafik yok.plot, surf veya imagesc çalıştırınca burada görünür.");
assert(plotChildren.every((child) => plot.childNodes.includes(child)));
for (let i = 0; i < 3; i++) utils.applyStaticTranslations(root, "en", turkish);
assert.equal(run.textContent, "Run");
assert.equal(run.getAttribute("title"), "Run");
assert.equal(menu.getAttribute("title"), null);
assert.equal(plot.textContent, "No figures yet.plot, surf or imagesc will appear here when run.");
assert.equal(heading.textContent, "Workspace 7");
utils.applyStaticTranslations(section, "tr", turkish);
assert.equal(section.textContent, "Bölüm", "root itself must be translated");
const dynamic = new Element({ "data-i18n": "New File" }, [text("New File")]);
utils.applyStaticTranslations(dynamic, "tr", turkish);
dynamic.textContent = "my_script.m";
utils.applyStaticTranslations(dynamic, "en", turkish);
assert.equal(dynamic.textContent, "my_script.m", "language switch must preserve feature-owned text");
const unsafe = new Element({ "data-i18n": "Unsafe" }, [text("Unsafe")]);
utils.applyStaticTranslations(unsafe, "tr", { Unsafe: "<script>alert(1)</script>" });
assert.equal(unsafe.childNodes.length, 1);
assert.equal(unsafe.childNodes[0].nodeType, 3);
assert.equal(unsafe.textContent, "<script>alert(1)</script>");

assert.equal(settings.cloneDefaults().preferences.language, "system");
for (const language of ["system", "en", "tr"]) {
  assert.equal(settings.sanitizeSettings({ version: 1, preferences: { language } }).preferences.language, language);
}
for (const language of [undefined, null, "", "TR", "fr", 1, true, {}, ["tr"]]) {
  assert.equal(settings.sanitizeSettings({ version: 1, preferences: { language } }).preferences.language, "system");
}
const old = settings.sanitizeSettings({ version: 1, preferences: { theme: "light", editorFontSize: 17 } });
assert.equal(old.preferences.language, "system");
assert.equal(old.preferences.theme, "light");
assert.equal(old.preferences.editorFontSize, 17);

// Exercise the actual ES-module facade with the already-used build dependency.
const Module = require("node:module");
const bundled = require("esbuild").buildSync({ entryPoints: [require.resolve("../frontend/i18n.js")], bundle: true, format: "cjs", platform: "node", write: false }).outputFiles[0].text;
const moduleInstance = new Module("i18n-test");
const savedDocument = global.document;
const savedNavigator = Object.getOwnPropertyDescriptor(global, "navigator");
try {
  global.document = root;
  root.documentElement = { lang: "en" };
  Object.defineProperty(global, "navigator", { value: { languages: ["tr-TR"] }, configurable: true });
  moduleInstance._compile(bundled, "i18n-test.cjs");
  const api = moduleInstance.exports;
  assert.equal(api.getLanguage(), "tr");
  assert.equal(api.t("Run"), "Çalıştır");
  const notified = [];
  const unsubscribe = api.onLanguageChange((language) => notified.push(language));
  api.setLanguage("en");
  assert.equal(root.documentElement.lang, "en");
  assert.equal(run.textContent, "Run");
  api.setLanguage("en");
  assert.deepEqual(notified, ["en"]);
  api.setLanguage("system");
  assert.equal(root.documentElement.lang, "tr");
  assert.equal(run.textContent, "Çalıştır");
  assert.deepEqual(notified, ["en", "tr"]);
  unsubscribe();
  api.setLanguage("en");
  assert.deepEqual(notified, ["en", "tr"]);
} finally {
  if (savedDocument === undefined) delete global.document;
  else global.document = savedDocument;
  if (savedNavigator) Object.defineProperty(global, "navigator", savedNavigator);
  else delete global.navigator;
}
console.log("I18N UNIT PASS: locale resolution, fallback, placeholders, safe repeated DOM translation, terms, subscriptions, settings migration/validation.");
