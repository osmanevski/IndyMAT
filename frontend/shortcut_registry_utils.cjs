const PUNCTUATION_CODES = new Set(["Backquote", "Minus", "Equal", "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote", "Comma", "Period", "Slash"]);
const KEY_CODE_NAMES = {
  8: "Backspace",
  9: "Tab",
  13: "Enter",
  27: "Escape",
  32: "Space",
  37: "ArrowLeft",
  38: "ArrowUp",
  39: "ArrowRight",
  40: "ArrowDown",
  46: "Delete",
  188: "Comma",
  189: "Minus",
  190: "Period",
  191: "Slash",
  187: "Equal"
};
const DISPLAY_CODES = {
  Backspace: "⌫",
  Delete: "Delete",
  Enter: "↵",
  Tab: "Tab",
  Space: "Space",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Equal: "=",
  Minus: "-"
};
const RESERVED_CODES = new Set(["KeyQ", "KeyW", "KeyT", "KeyN", "KeyL", "KeyR"]);
const SETTINGS_KEY = "mf-settings-v1";
const MAX_STORAGE_LENGTH = 65536;
const MAX_SHORTCUTS = 128;
const MAX_BINDINGS = 4;
const BINDING_PATTERN = /^(?:Mod\+)?(?:Ctrl\+)?(?:Alt\+)?(?:Shift\+)?(?:Key[A-Z]|Digit[0-9]|F(?:[1-9]|1[0-9]|2[0-4])|Backquote|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash|Space|Enter|Tab|Escape|Backspace|Delete|Insert|Home|End|PageUp|PageDown|ArrowUp|ArrowDown|ArrowLeft|ArrowRight)$/;
const DEFAULT_SETTINGS = Object.freeze({
  version: 1,
  preferences: Object.freeze({ language: "system", theme: "dark", editorFontSize: 13, consoleFontSize: 13, indentWidth: 4, useTabs: false }),
  panels: Object.freeze({ files: true, workspace: true, history: true, figures: true, debugger: true }),
  layout: Object.freeze({ left: 220, right: 300, editorHeight: 56, consoleWidth: 50 }),
  activeTabs: Object.freeze({ plotMode: "interactive", figureName: "" }),
  shortcuts: Object.freeze({})
});

function cloneDefaults() {
  const result = JSON.parse(JSON.stringify(DEFAULT_SETTINGS));
  result.shortcuts = Object.create(null);
  return result;
}

function codeFromEvent(event) {
  if (event.code && !["Unidentified", ""].includes(event.code)) return event.code;
  if (KEY_CODE_NAMES[event.keyCode]) return KEY_CODE_NAMES[event.keyCode];
  if (event.keyCode >= 65 && event.keyCode <= 90) return "Key" + String.fromCharCode(event.keyCode);
  if (event.keyCode >= 48 && event.keyCode <= 57) return "Digit" + String.fromCharCode(event.keyCode);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.key || "")) return event.key;
  if (/^[a-zA-Z]$/.test(event.key || "")) return "Key" + event.key.toUpperCase();
  if (/^[0-9]$/.test(event.key || "")) return "Digit" + event.key;
  if (event.key === "ı" || event.key === "İ") return "KeyI";
  const byKey = { ",": "Comma", ".": "Period", "/": "Slash", "=": "Equal", "+": "Equal", "-": "Minus", " ": "Space" };
  return byKey[event.key] || event.key || "";
}

function canonicalEvent(event, mac = true, options = {}) {
  if (event.isComposing || ["Dead", "Process", "Unidentified"].includes(event.key)) return "";
  const code = codeFromEvent(event);
  if (!code || ["Shift", "Control", "Alt", "Meta"].includes(code)) return "";
  const parts = [];
  if (mac ? event.metaKey : event.ctrlKey) parts.push("Mod");
  if (mac && event.ctrlKey || !mac && event.metaKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  const punctuationKey = typeof event.key === "string" && event.key.length === 1 && !/[\p{L}\p{N}]/u.test(event.key);
  if (event.shiftKey && !PUNCTUATION_CODES.has(code) && !punctuationKey && !options.ignoreShift) parts.push("Shift");
  parts.push(code);
  return parts.join("+");
}

function displayBinding(binding, mac = true, capturedLabel = "", translator = (source) => source) {
  if (capturedLabel) return capturedLabel;
  const parts = String(binding).split("+");
  const code = parts.pop();
  const modifiers = parts.map((part) => part === "Mod" ? mac ? "⌘" : "Ctrl+" : part === "Ctrl" ? mac ? "⌃" : "Meta+" : part === "Alt" ? mac ? "⌥" : "Alt+" : part === "Shift" ? "⇧" : part + "+").join("");
  let key = code === "Space" ? translator("Space") : DISPLAY_CODES[code] || (code.startsWith("Key") ? code.slice(3) : code.startsWith("Digit") ? code.slice(5) : code);
  return modifiers + key;
}

function captureLabel(event, mac = true) {
  const binding = canonicalEvent(event, mac);
  if (!binding) return "";
  const parts = binding.split("+");
  const code = parts.at(-1);
  const printable = event.key && event.key.length === 1 ? event.key.toLocaleUpperCase("tr") : "";
  return displayBinding(binding, mac, printable ? displayBinding(parts.slice(0, -1).concat(code).join("+"), mac).replace(displayBinding(code, mac), printable) : "");
}

function scopesOverlap(first, second) {
  return first === second || first === "global" || second === "global";
}

function matchesBinding(definition, binding, event, mac = true, override = false) {
  const policy = !override && definition.defaultMatch;
  if (policy) {
    const key = policy.lowercase ? event.key?.toLowerCase() : event.key;
    if (!policy.keys.includes(key)) return false;
    if (policy.modifiers === "primary") return !!(event.metaKey || event.ctrlKey);
    if (policy.modifiers === "meta") return !!event.metaKey;
    if (policy.modifiers === "control") return !!event.ctrlKey && !event.metaKey;
    if (policy.modifiers === "shift") return !!event.shiftKey;
    if (policy.modifiers === "no-shift") return !event.shiftKey;
    return true;
  }
  return canonicalEvent(event, mac, { ignoreShift: definition.ignoreShift }) === canonicalBinding(binding, definition.ignoreShift);
}

function canonicalBinding(binding, ignoreShift = false) {
  const parts = binding.split("+");
  return ignoreShift || PUNCTUATION_CODES.has(parts.at(-1)) ? parts.filter((part) => part !== "Shift").join("+") : binding;
}

function bindingsOverlap(first, firstBinding, second, secondBinding, secondOverride, mac) {
  const codes = new Set([firstBinding.split("+").at(-1), secondBinding.split("+").at(-1)]);
  for (const code of codes) {
    const baseKey = { Space: " ", Equal: "=", Minus: "-", Slash: "/", Comma: ",", Period: "." }[code] || code.replace(/^Key/, "").replace(/^Digit/, "");
    const layoutKeys = { Equal: ["=", "+", "-", "_"], Minus: ["-", "_", "*", "?"], Slash: ["/", "?", ".", ":"], Digit0: ["0", ")", "="], Digit2: ["2", "@", "'"], Digit3: ["3", "#", "^"], Digit4: ["4", "$", "+"], Digit6: ["6", "^", "&"], Digit7: ["7", "&", "/"], Digit8: ["8", "*", "("], Digit9: ["9", "(", ")"] };
    const keys = new Set([baseKey, baseKey.toLowerCase(), ...(layoutKeys[code] || [])]);
    for (let mask = 0; mask < 16; mask++) {
      for (const key of keys) {
        const event = { code, key, metaKey: !!(mask & 1), ctrlKey: !!(mask & 2), altKey: !!(mask & 4), shiftKey: !!(mask & 8) };
        if (matchesBinding(first, firstBinding, event, mac, true) && matchesBinding(second, secondBinding, event, mac, secondOverride)) return true;
      }
    }
  }
  return false;
}

function findConflict(definitions, overrides, id, binding, mac = true) {
  const current = definitions.find((item) => item.id === id);
  if (!current) return null;
  for (const candidate of definitions) {
    if (candidate.id === id || !scopesOverlap(current.scope, candidate.scope)) continue;
    const custom = Object.hasOwn(overrides, candidate.id) && overrides[candidate.id];
    const values = custom ? custom.map((item) => item.binding) : candidate.bindings;
    if (values.some((value) => bindingsOverlap(current, binding, candidate, value, !!custom, mac))) return candidate;
  }
  return null;
}

function reservedReason(binding, definition = {}, event = null, mac = true, translator = (source) => source) {
  const tr = (source) => translator(source);
  if (event && (event.isComposing || ["Dead", "Process", "Unidentified"].includes(event.key) || event.getModifierState?.("AltGraph"))) return tr("Text composition keys cannot be assigned as shortcuts.");
  if (!BINDING_PATTERN.test(binding)) return tr("Choose a valid shortcut combination.");
  const parts = String(binding).split("+");
  const code = parts.at(-1);
  if (parts.includes("Mod") && RESERVED_CODES.has(code)) return tr("This combination is reserved by the browser or operating system.");
  if (parts.includes("Alt") && code === "F4") return tr("Alt+F4 is reserved for closing the window.");
  if (parts.includes("Mod") && ["KeyH", "KeyM", "Space", "Tab", "Comma"].includes(code)) return tr("This combination is reserved for an operating system window or search command.");
  if (parts.includes("Alt") && ["Tab", "Escape"].includes(code)) return tr("This combination is reserved by the operating system.");
  const textScope = ["global", "editor", "command-line"].includes(definition.scope);
  if (textScope && parts.includes("Mod") && ["KeyA", "KeyC", "KeyF", "KeyV", "KeyX", "KeyZ", "KeyY"].includes(code)) return tr("This combination is reserved for basic text editing commands.");
  if (textScope && mac && parts.includes("Ctrl") && ["KeyA", "KeyB", "KeyD", "KeyE", "KeyF", "KeyH", "KeyK", "KeyN", "KeyO", "KeyP", "KeyT", "KeyU", "KeyV", "KeyW", "KeyY"].includes(code)) return tr("This combination is reserved for moving or editing the text cursor.");
  const printable = /^(Key|Digit)/.test(code) || PUNCTUATION_CODES.has(code) || code === "Space";
  if (printable && parts.includes("Alt") && (!parts.includes("Mod") || !mac && !parts.includes("Ctrl"))) return tr("Combinations that produce text with Option / AltGr cannot be assigned as shortcuts.");
  if (printable && !parts.includes("Mod") && !parts.includes("Ctrl")) return tr("Choose a combination with Ctrl or Command to preserve text input.");
  const editing = ["Enter", "Escape", "Tab", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Backspace", "Delete", "Home", "End", "PageUp", "PageDown"];
  if (editing.includes(code) && !definition.allowEditingKeys?.includes(code)) return tr("This key is reserved for text editing and navigation.");
  return "";
}

function finiteNumber(value, fallback, minimum, maximum) {
  value = Number(value);
  return Number.isFinite(value) ? Math.max(minimum, Math.min(maximum, value)) : fallback;
}

function sanitizeSettings(value, definitions = [], mac = true, translator = (source) => source) {
  const result = cloneDefaults();
  if (!value || value.version !== 1) return result;
  result.revision = Number.isSafeInteger(value.revision) && value.revision >= 0 ? value.revision : 0;
  if (["system", "en", "tr"].includes(value.preferences?.language)) result.preferences.language = value.preferences.language;
  if (["dark", "light"].includes(value.preferences?.theme)) result.preferences.theme = value.preferences.theme;
  result.preferences.editorFontSize = finiteNumber(value.preferences?.editorFontSize, 13, 10, 24);
  result.preferences.consoleFontSize = finiteNumber(value.preferences?.consoleFontSize, 13, 10, 24);
  result.preferences.indentWidth = finiteNumber(value.preferences?.indentWidth, 4, 1, 8);
  result.preferences.useTabs = value.preferences?.useTabs === true;
  for (const name of Object.keys(result.panels)) if (typeof value.panels?.[name] === "boolean") result.panels[name] = value.panels[name];
  result.layout.left = finiteNumber(value.layout?.left, 220, 140, 360);
  result.layout.right = finiteNumber(value.layout?.right, 300, 190, 500);
  result.layout.editorHeight = finiteNumber(value.layout?.editorHeight, 56, 20, 78);
  result.layout.consoleWidth = finiteNumber(value.layout?.consoleWidth, 50, 20, 80);
  if (["interactive", "png"].includes(value.activeTabs?.plotMode)) result.activeTabs.plotMode = value.activeTabs.plotMode;
  if (typeof value.activeTabs?.figureName === "string") result.activeTabs.figureName = value.activeTabs.figureName.slice(0, 120);
  if (value.shortcuts && typeof value.shortcuts === "object" && !Array.isArray(value.shortcuts)) {
    for (const definition of definitions.slice(0, MAX_SHORTCUTS)) {
      const id = definition.id;
      if (["__proto__", "constructor", "prototype"].includes(id) || definition.configurable === false || !Object.hasOwn(value.shortcuts, id)) continue;
      const bindings = value.shortcuts[id];
      if (!Array.isArray(bindings) || bindings.length > MAX_BINDINGS) continue;
      const valid = bindings.filter((item) => item && typeof item.binding === "string" && item.binding.length < 80 && !reservedReason(item.binding, definition, null, mac, translator)).map((item) => ({ binding: canonicalBinding(item.binding, definition.ignoreShift), label: typeof item.label === "string" ? item.label.slice(0, 40) : "" }));
      if (valid.length) result.shortcuts[id] = valid;
    }
    // Removing an invalid override restores its defaults, which may expose a
    // second collision. Iterate to stability; every pass removes an entry.
    let removed;
    do {
      removed = false;
      for (const definition of definitions.slice(0, MAX_SHORTCUTS)) {
        if (result.shortcuts[definition.id]?.some((item) => findConflict(definitions, result.shortcuts, definition.id, item.binding, mac))) {
          delete result.shortcuts[definition.id];
          removed = true;
        }
      }
    } while (removed);
  }
  return result;
}

function loadSettings(storage, definitions = [], mac = true, translator = (source) => source) {
  let raw;
  try {
    raw = storage.getItem(SETTINGS_KEY);
  } catch (error) {
    return { settings: cloneDefaults(), available: false, migrated: false, error };
  }
  if (raw !== null) {
    try {
      if (raw.length > MAX_STORAGE_LENGTH) throw new Error(translator("Settings exceed the size limit."));
      const parsed = JSON.parse(raw);
      return { settings: parsed?.version === 1 ? sanitizeSettings(parsed, definitions, mac, translator) : cloneDefaults(), available: true, migrated: false, malformed: parsed?.version !== 1 };
    } catch {
      return { settings: cloneDefaults(), available: true, migrated: false, malformed: true };
    }
  }
  const settings = cloneDefaults();
  let migrated = false;
  try {
    const theme = storage.getItem("mf-theme");
    const font = Number(storage.getItem("mf-editor-font-size"));
    if (["dark", "light"].includes(theme)) {
      settings.preferences.theme = theme;
      migrated = true;
    }
    if (Number.isFinite(font) && font >= 10 && font <= 24) {
      settings.preferences.editorFontSize = font;
      migrated = true;
    }
  } catch {
  }
  return { settings, available: true, migrated, missing: true };
}

function storeSettings(storage, settings) {
  try {
    const raw = JSON.stringify(settings);
    if (raw.length > MAX_STORAGE_LENGTH) return false;
    storage.setItem(SETTINGS_KEY, raw);
    return true;
  } catch {
    return false;
  }
}

function writePreferenceMirrors(storage, preferences, previous = null, missingOnly = false) {
  // mf-settings-v1 is authoritative; these are write-through compatibility
  // mirrors, never inputs except during migration without a versioned store.
  for (const [name, key] of [["theme", "mf-theme"], ["editorFontSize", "mf-editor-font-size"]]) {
    if (missingOnly ? storage.getItem(key) === null : !previous || previous[name] !== preferences[name]) storage.setItem(key, String(preferences[name]));
  }
}

function settingsPatch(base, next) {
  const patch = [];
  for (const section of ["preferences", "panels", "layout", "activeTabs", "shortcuts"]) {
    for (const key of new Set([...Object.keys(base[section]), ...Object.keys(next[section])])) {
      if (JSON.stringify(base[section][key]) !== JSON.stringify(next[section][key])) patch.push({ section, key, value: next[section][key] });
    }
  }
  return patch;
}

function applyPatch(settings, patch) {
  for (const { section, key, value } of patch) {
    if (["__proto__", "constructor", "prototype"].includes(key)) continue;
    if (value === undefined) delete settings[section][key];
    else settings[section][key] = value;
  }
  return settings;
}

function commitPatch(storage, patch, definitions = [], mac = true, translator = (source) => source) {
  const loaded = loadSettings(storage, definitions, mac, translator);
  if (!loaded.available) return loaded;
  const next = sanitizeSettings(applyPatch(loaded.settings, patch), definitions, mac, translator);
  next.revision = (loaded.settings.revision || 0) + 1;
  const available = storeSettings(storage, next);
  return { settings: next, available };
}

module.exports = { SETTINGS_KEY, MAX_STORAGE_LENGTH, MAX_BINDINGS, DEFAULT_SETTINGS, cloneDefaults, canonicalEvent, canonicalBinding, matchesBinding, captureLabel, displayBinding, scopesOverlap, findConflict, reservedReason, sanitizeSettings, loadSettings, storeSettings, writePreferenceMirrors, settingsPatch, applyPatch, commitPatch };
