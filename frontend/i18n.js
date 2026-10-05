/* Language contract for feature lanes:
 * t(EnglishSource, {name: value}) interpolates named {name} placeholders;
 * English source is the key; Turkish must equal the pre-existing text.
 * Add flat translations to locales/tr.js. Missing keys fall back to English.
 * data-i18n updates text; data-i18n-title/-placeholder/-aria-label update attributes.
 * Icons/counters stay intact; mixed prose may use {i18nChild0}, ... child slots.
 * Text replaced by a feature is left to its onLanguageChange renderer.
 * data-i18n-term adds " — MATLAB Term" to Turkish titles only, never aria-labels.
 * getLanguage() returns en/tr; setLanguage(system/en/tr) applies immediately.
 * Persist the setting through registry.updateSetting("preferences", "language", value).
 * onLanguageChange(fn) re-renders dynamic text and returns an unsubscribe function.
 */
import utils from "./i18n_utils.cjs";
import assistantText from "./locales/tr_assistant.js";
import base from "./locales/tr.js";
import workspaceText from "./locales/tr_calisma_alani.js";
import editorText from "./locales/tr_editor.js";
import settingsText from "./locales/tr_ayarlar.js";
import filesText from "./locales/tr_dosyalar.js";

const turkish = { ...assistantText, ...base, ...workspaceText, ...editorText, ...settingsText, ...filesText };

let language = utils.resolveLanguage("system", globalThis.navigator?.languages || [globalThis.navigator?.language]);
const subscribers = new Set();

export function t(source, params) {
  return utils.translate(source, params, language, turkish);
}

export function getLanguage() {
  return language;
}

export function applyStaticTranslations(root = document) {
  utils.applyStaticTranslations(root, language, turkish);
}

export function setLanguage(setting = "system") {
  const next = utils.resolveLanguage(setting, globalThis.navigator?.languages || [globalThis.navigator?.language]);
  const changed = language !== next;
  language = next;
  document.documentElement.lang = language;
  applyStaticTranslations();
  if (changed) {
    for (const subscriber of [...subscribers]) {
      try { subscriber(language); } catch (error) { console.error(error); }
    }
  }
  return language;
}

export function onLanguageChange(fn) {
  subscribers.add(fn);
  return () => subscribers.delete(fn);
}
