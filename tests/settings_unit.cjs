const assert = require("node:assert/strict");
const utils = require("../frontend/shortcut_registry_utils.cjs");

function event(key, code, modifiers = {}) {
  return { key, code, keyCode: 0, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers };
}

assert.equal(utils.canonicalEvent(event("i", "KeyI", { ctrlKey: true }), true), "Ctrl+KeyI");
assert.equal(utils.canonicalEvent(event("ı", "KeyI", { ctrlKey: true }), true), "Ctrl+KeyI");
assert.equal(utils.canonicalEvent({ ...event("ı", "", { ctrlKey: true }), keyCode: 73 }, true), "Ctrl+KeyI");
assert.equal(utils.canonicalEvent(event("/", "Slash", { metaKey: true }), true), "Mod+Slash");
assert.equal(utils.canonicalEvent(event("/", "Slash", { metaKey: true, shiftKey: true }), true), "Mod+Slash", "punctuation must not be distinguished only by Shift");
assert.equal(utils.canonicalEvent(event("/", "Digit7", { metaKey: true, shiftKey: true }), true), "Mod+Digit7", "Turkish Q punctuation must not be distinguished only by Shift");
assert.equal(utils.canonicalEvent(event("F9", "F9", { shiftKey: true }), true), "Shift+F9");
assert.equal(utils.canonicalEvent(event("s", "KeyS", { ctrlKey: true }), false), "Mod+KeyS");

const definitions = [
  { id: "save", label: "Kaydet", scope: "global", bindings: ["Mod+KeyS"] },
  { id: "editor-a", label: "Editör A", scope: "editor", bindings: ["Ctrl+KeyG"] },
  { id: "editor-b", label: "Editör B", scope: "editor", bindings: ["F8"] },
  { id: "files", label: "Dosya", scope: "file-list", bindings: ["Enter"] },
  { id: "workspace", label: "Değişken", scope: "workspace", bindings: ["Enter"] }
];
assert.equal(utils.findConflict(definitions, {}, "editor-b", "Ctrl+KeyG").id, "editor-a");
assert.equal(utils.findConflict(definitions, {}, "files", "Enter"), null, "disjoint scopes may share a binding");
assert.equal(utils.findConflict(definitions, {}, "editor-b", "Mod+KeyS").id, "save", "global shortcuts overlap every scope");
assert.equal(utils.reservedReason("Mod+KeyQ").length > 0, true);
assert.equal(utils.reservedReason("Mod+KeyW").length > 0, true);
assert.equal(utils.reservedReason("Alt+F4").length > 0, true);
assert.equal(utils.reservedReason("Mod+Space").length > 0, true);
assert.equal(utils.reservedReason("Ctrl+KeyQ"), "");
assert.equal(utils.reservedReason("Alt+F4"), "Alt+F4 is reserved for closing the window.");
assert.equal(utils.reservedReason("Alt+F4", {}, null, true, (source) => ({ "Alt+F4 is reserved for closing the window.": "Alt+F4 pencereyi kapatmak için ayrılmıştır." })[source] || source), "Alt+F4 pencereyi kapatmak için ayrılmıştır.");
assert.equal(utils.displayBinding("Mod+Space", true, "", (source) => source === "Space" ? "Boşluk" : source), "⌘Boşluk");

function storage(values = {}, failures = {}) {
  return {
    getItem(key) {
      if (failures.read) throw new Error("unavailable");
      return Object.hasOwn(values, key) ? values[key] : null;
    },
    setItem(key, value) {
      if (failures.write) throw new Error("full");
      values[key] = value;
    },
    removeItem(key) {
      delete values[key];
    }
  };
}

let loaded = utils.loadSettings(storage({ "mf-theme": "light", "mf-editor-font-size": "17", "mf-drafts-v2": "keep" }));
assert.equal(loaded.migrated, true);
assert.equal(loaded.missing, true);
assert.equal(loaded.settings.preferences.theme, "light");
assert.equal(loaded.settings.preferences.editorFontSize, 17);
assert.equal(loaded.settings.preferences.consoleFontSize, 13);
loaded = utils.loadSettings(storage({ "mf-settings-v1": JSON.stringify({ ...utils.cloneDefaults(), preferences: { ...utils.cloneDefaults().preferences, theme: "dark", editorFontSize: 19 } }), "mf-theme": "light", "mf-editor-font-size": "11" }));
assert.equal(loaded.migrated, false);
assert.equal(loaded.settings.preferences.theme, "dark", "legacy theme overwrote newer settings");
assert.equal(loaded.settings.preferences.editorFontSize, 19, "legacy font size overwrote newer settings");

loaded = utils.loadSettings(storage({ "mf-settings-v1": "{" }));
assert.equal(loaded.malformed, true);
assert.deepEqual(loaded.settings, utils.cloneDefaults());
loaded = utils.loadSettings(storage({ "mf-settings-v1": JSON.stringify({ version: 99, preferences: { theme: "light" } }) }));
assert.equal(loaded.malformed, true);
assert.equal(loaded.settings.preferences.theme, "dark");
loaded = utils.loadSettings(storage({}, { read: true }));
assert.equal(loaded.available, false);
assert.deepEqual(loaded.settings, utils.cloneDefaults());
assert.equal(utils.storeSettings(storage({}, { write: true }), utils.cloneDefaults()), false, "full storage did not degrade cleanly");
const writableValues = {};
assert.equal(utils.storeSettings(storage(writableValues), utils.cloneDefaults()), true);
assert.equal(JSON.parse(writableValues["mf-settings-v1"]).version, 1);

const sanitized = utils.sanitizeSettings({ version: 1, preferences: { theme: "light", editorFontSize: 200, consoleFontSize: "bad", indentWidth: 0, useTabs: true }, panels: { files: false }, layout: { left: 999, right: 1, editorHeight: 90, consoleWidth: 5 }, activeTabs: { plotMode: "png", figureName: "Figure 9" }, shortcuts: { save: [{ binding: "Mod+Shift+KeyB", label: "⌘⇧B" }] } }, definitions);
assert.equal(sanitized.preferences.editorFontSize, 24);
assert.equal(sanitized.preferences.consoleFontSize, 13);
assert.equal(sanitized.preferences.indentWidth, 1);
assert.equal(sanitized.preferences.useTabs, true);
assert.equal(sanitized.panels.files, false);
assert.equal(sanitized.panels.workspace, true);
assert.deepEqual(sanitized.layout, { left: 360, right: 190, editorHeight: 78, consoleWidth: 20 });
assert.deepEqual(sanitized.activeTabs, { plotMode: "png", figureName: "Figure 9" });
assert.deepEqual(sanitized.shortcuts.save, [{ binding: "Mod+Shift+KeyB", label: "⌘⇧B" }]);

// Findings 2/3: safety and conflicts use dispatch semantics, not string equality.
for (const binding of ['KeyA', 'Digit2', 'Enter', 'Escape', 'Tab', 'ArrowLeft', 'Alt+Digit2', 'Alt+BracketLeft', 'Alt+KeyB', 'Mod+Tab', 'Mod+Comma']) {
  assert(utils.reservedReason(binding), `unsafe rebind accepted: ${binding}`);
  const settings = utils.sanitizeSettings({ version: 1, shortcuts: { save: [{ binding }] } }, definitions);
  assert.equal(settings.shortcuts.save, undefined);
}
assert(utils.reservedReason('Mod+Alt+KeyA', {}, null, false), 'AltGr printable accepted');
for (const key of ['Dead', 'Process', 'Unidentified']) {
  assert.equal(utils.canonicalEvent(event(key, 'BracketLeft', { altKey: true })), '');
  assert(utils.reservedReason('Mod+KeyB', {}, event(key, 'KeyB', { metaKey: true })));
}
assert(utils.reservedReason('Mod+KeyB', {}, event('b', 'KeyB', { metaKey: true, isComposing: true })));
assert(utils.reservedReason('Mod+KeyB', {}, { getModifierState: name => name === 'AltGraph' }));
assert.equal(utils.reservedReason('Enter', { scope: 'command-line', allowEditingKeys: ['Enter'] }), '');
const shifted = [
  { id: 'move', scope: 'workspace', bindings: ['ArrowUp'], ignoreShift: true },
  { id: 'open', scope: 'workspace', bindings: ['Enter'] },
  { id: 'punctuation', scope: 'editor', bindings: ['Ctrl+Comma'] },
  { id: 'other', scope: 'editor', bindings: ['F8'] }
];
assert.equal(utils.findConflict(shifted, {}, 'open', 'Shift+ArrowUp').id, 'move');
const shiftedUp = event('ArrowUp', 'ArrowUp', { shiftKey: true });
assert.equal(utils.matchesBinding(shifted[0], 'ArrowUp', shiftedUp), true);
assert.equal(utils.matchesBinding(shifted[1], 'Shift+ArrowUp', shiftedUp, true, true), true);
assert.equal(utils.findConflict(shifted, {}, 'other', 'Ctrl+Shift+Comma').id, 'punctuation');
assert.equal(utils.findConflict(shifted, {}, 'other', 'Shift+ArrowUp'), null);
const tolerant = [...definitions, { id: 'legacy', scope: 'global', bindings: ['Mod+KeyP'], defaultMatch: { keys: ['p'], lowercase: true, modifiers: 'primary' } }];
assert.equal(utils.findConflict(tolerant, {}, 'editor-a', 'Ctrl+Alt+Shift+KeyP').id, 'legacy');
for (const scope of ['global', 'editor', 'command-line']) assert(utils.reservedReason('Mod+KeyV', { scope }), 'paste shortcut stolen');
assert(utils.reservedReason('Ctrl+KeyB', { scope: 'editor' }), 'macOS caret shortcut stolen');
const cascading = [
  { id: 'a', scope: 'editor', bindings: ['F6'] },
  { id: 'b', scope: 'editor', bindings: ['F8'] },
  { id: 'c', scope: 'editor', bindings: ['F7'] }
];
const cascadingSettings = utils.sanitizeSettings({ version: 1, shortcuts: { a: [{ binding: 'F8' }], b: [{ binding: 'F7' }] } }, cascading);
assert.equal(Object.keys(cascadingSettings.shortcuts).length, 0, 'restored default left another override shadowed');

// Finding 7: bound parsing and per-command work, reject unknown IDs/prototypes.
const hostile = JSON.parse('{"version":1,"shortcuts":{"__proto__":[{"binding":"F8"}],"constructor":[{"binding":"F8"}],"unknown":[{"binding":"F8"}]}}');
const cleaned = utils.sanitizeSettings(hostile, definitions);
assert.equal(Object.getPrototypeOf(cleaned.shortcuts), null);
assert.equal(Object.keys(cleaned.shortcuts).length, 0);
for (const bindings of [Array(150000).fill({ binding: 'F8' }), [{ binding: 'Ctrl+Ctrl+KeyB' }], [{ binding: 'garbage' }]]) {
  assert.equal(utils.sanitizeSettings({ version: 1, shortcuts: { save: bindings } }, definitions).shortcuts.save, undefined);
}
loaded = utils.loadSettings(storage({ 'mf-settings-v1': JSON.stringify({ version: 1, shortcuts: { save: Array(150000).fill({ binding: 'F8' }) } }) }), definitions);
assert.equal(loaded.malformed, true);
assert.deepEqual(loaded.settings, utils.cloneDefaults());
assert.equal(utils.storeSettings(storage(), { oversized: 'a'.repeat(utils.MAX_STORAGE_LENGTH) }), false);

// Finding 4: independently loaded tabs commit field patches against latest data.
const concurrentValues = {};
const concurrent = storage(concurrentValues);
const baseA = utils.cloneDefaults();
const baseB = utils.cloneDefaults();
const tabA = utils.cloneDefaults();
const tabB = utils.cloneDefaults();
tabA.preferences.theme = 'light';
tabB.preferences.editorFontSize = 19;
utils.commitPatch(concurrent, utils.settingsPatch(baseA, tabA), definitions);
let committed = utils.commitPatch(concurrent, utils.settingsPatch(baseB, tabB), definitions);
assert.equal(committed.settings.preferences.theme, 'light');
assert.equal(committed.settings.preferences.editorFontSize, 19);
assert.equal(committed.settings.revision, 2);
tabA.preferences.editorFontSize = 20;
committed = utils.commitPatch(concurrent, utils.settingsPatch({ ...tabA, preferences: { ...tabA.preferences, editorFontSize: 13 } }, tabA), definitions);
assert.equal(committed.settings.preferences.editorFontSize, 20, 'last serialized writer must win the same field');
assert.equal(committed.settings.preferences.theme, 'light');

// Compatibility mirrors remain available, but never override valid settings.
const legacyValues = { 'mf-theme': 'light', 'mf-editor-font-size': '17', 'mf-drafts-v2': 'keep', 'mf-drafts': 'also keep' };
const legacy = storage(legacyValues);
assert.equal(utils.loadSettings(legacy).migrated, true);
utils.commitPatch(legacy, []);
assert.equal(legacy.getItem('mf-theme'), 'light');
assert.equal(legacy.getItem('mf-editor-font-size'), '17');
assert.equal(legacy.getItem('mf-drafts-v2'), 'keep');
assert.equal(legacy.getItem('mf-drafts'), 'also keep');
assert.equal(utils.loadSettings(legacy).migrated, false, 'valid settings migrated twice');
legacy.setItem('mf-theme', 'dark');
legacy.setItem('mf-editor-font-size', '11');
let authoritative = utils.loadSettings(legacy);
assert.equal(authoritative.settings.preferences.theme, 'light');
assert.equal(authoritative.settings.preferences.editorFontSize, 17);
assert.equal(authoritative.migrated, false);
const mirrorReads = [];
utils.loadSettings({ getItem(key) { mirrorReads.push(key); return legacy.getItem(key); } });
assert.deepEqual(mirrorReads, ['mf-settings-v1'], 'valid settings consulted mirrors');
utils.writePreferenceMirrors(legacy, authoritative.settings.preferences);
assert.equal(legacy.getItem('mf-theme'), 'light');
assert.equal(legacy.getItem('mf-editor-font-size'), '17');
legacy.removeItem('mf-editor-font-size');
legacy.setItem('mf-theme', 'dark');
utils.writePreferenceMirrors(legacy, authoritative.settings.preferences, null, true);
assert.equal(legacy.getItem('mf-theme'), 'dark', 'startup replaced an existing mirror');
assert.equal(legacy.getItem('mf-editor-font-size'), '17', 'startup did not populate missing mirror');
utils.writePreferenceMirrors(legacy, { theme: 'light', editorFontSize: 19 }, authoritative.settings.preferences);
assert.equal(legacy.getItem('mf-theme'), 'dark', 'font change wrote an unrelated mirror');
assert.equal(legacy.getItem('mf-editor-font-size'), '19');
const failedLegacy = { 'mf-theme': 'light', 'mf-editor-font-size': '17' };
assert.equal(utils.commitPatch(storage(failedLegacy, { write: true }), []).available, false);
assert.equal(failedLegacy['mf-theme'], 'light', 'failed migration deleted recoverable preference');

console.log("SETTINGS UNIT PASS: Turkish/US matching, punctuation, conflicts, reserved keys, legacy migration and malformed storage fallback.");
