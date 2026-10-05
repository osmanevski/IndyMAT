// Pure translation and DOM logic; no browser globals or storage access.
function resolveLanguage(setting = "system", navigatorLanguages = []) {
  if (setting === "en" || setting === "tr") return setting;
  const languages = typeof navigatorLanguages === "string" ? [navigatorLanguages] : navigatorLanguages;
  for (const language of Array.isArray(languages) ? languages : []) {
    if (typeof language !== "string") continue;
    if (/^tr(?:-|_|$)/i.test(language)) return "tr";
    if (/^en(?:-|_|$)/i.test(language)) return "en";
  }
  return "en";
}

function translate(source, params = {}, language = "en", translations = {}) {
  const translated = language === "tr" && Object.hasOwn(translations, source) && typeof translations[source] === "string" ? translations[source] : source;
  return translated.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (placeholder, name) => params && Object.hasOwn(params, name) ? String(params[name]) : placeholder);
}

const renderedTexts = new WeakMap();

function applyText(element, source, translated) {
  // Translate direct text only so icons, counters and their listeners survive.
  // Mixed prose can reserve {i18nChild0}, {i18nChild1}, ... for existing children.
  const slots = source.match(/\{i18nChild\d+\}/g) || [];
  const directText = () => [...element.childNodes].filter((node) => node.nodeType === 3).map((node) => node.textContent).join("");
  const previous = renderedTexts.get(element);
  // Dynamic feature output owns any text it replaced (file names, status, etc.).
  // Subscribers can render that output in the new language after this pass.
  if (previous?.source === source && directText() !== previous.text) return;
  const translatedSlots = translated.match(/\{i18nChild\d+\}/g) || [];
  if (slots.join() !== translatedSlots.join()) translated = source;
  const parts = slots.length ? translated.split(/\{i18nChild\d+\}/g) : [translated];
  let index = 0;
  let written = false;
  for (const node of element.childNodes) {
    if (node.nodeType === 3) {
      node.textContent = written ? "" : parts[index] || "";
      written = true;
    } else if (slots.length) {
      index += 1;
      written = false;
    }
  }
  if (!element.childNodes.length) element.textContent = translated;
  renderedTexts.set(element, { source, text: directText() });
}

function applyStaticTranslations(root, language, translations) {
  const selector = "[data-i18n], [data-i18n-title], [data-i18n-placeholder], [data-i18n-aria-label], [data-i18n-term]";
  const elements = [...(root.matches?.(selector) ? [root] : []), ...(root.querySelectorAll?.(selector) || [])];
  for (const element of elements) {
    const source = element.getAttribute("data-i18n");
    if (source !== null) applyText(element, source, translate(source, {}, language, translations));
    for (const attribute of ["title", "placeholder", "aria-label"]) {
      const value = element.getAttribute("data-i18n-" + attribute);
      if (value !== null) element.setAttribute(attribute, translate(value, {}, language, translations));
    }
    const term = element.getAttribute("data-i18n-term");
    if (term) {
      const titleSource = element.getAttribute("data-i18n-title") ?? source ?? element.querySelector?.("[data-i18n]")?.getAttribute("data-i18n") ?? term;
      const title = translate(titleSource, {}, language, translations);
      if (language === "tr") element.setAttribute("title", title + " — " + term);
      else if (element.getAttribute("data-i18n-title") !== null) element.setAttribute("title", title);
      else element.removeAttribute("title");
    }
  }
}

module.exports = { resolveLanguage, translate, applyStaticTranslations };
