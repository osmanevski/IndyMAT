const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const { buildSync } = require("esbuild");
const utils = require("../frontend/layout_utils.cjs");
const settings = require("../frontend/shortcut_registry_utils.cjs");

// Today's v1 storage migrates by defaulting only the new bottom visibility field.
const old = { version: 1, layout: { left: 280, right: 310, editorHeight: 62, consoleWidth: 38 }, panels: { files: false, workspace: false, history: true, debugger: false, figures: false } };
const migrated = settings.sanitizeSettings(old);
assert.deepEqual(migrated.layout, old.layout);
assert.equal(migrated.panels.bottom, true);
assert.equal(migrated.panels.files, false);
assert.equal(migrated.panels.figures, false);
assert.equal(utils.panelVisible(migrated.panels, "right"), true);
utils.setPanelVisible(migrated, "right", false);
assert.equal(migrated.layout.rightPanels, 2);
assert.equal(utils.panelVisible(migrated.panels, "right"), false);
const reloaded = settings.sanitizeSettings(JSON.parse(JSON.stringify(migrated)));
utils.setPanelVisible(reloaded, "right", true);
assert.deepEqual(utils.RIGHT_PANELS.map((name) => reloaded.panels[name]), [false, true, false]);
assert.equal(reloaded.layout.right, 310);
for (const area of ["left", "right", "bottom"]) {
  utils.setPanelVisible(reloaded, area, false);
  assert.equal(utils.panelVisible(reloaded.panels, area), false);
}
assert.equal(settings.sanitizeSettings(reloaded).panels.bottom, false);
assert.deepEqual(utils.sanitizeLayout({ left: Infinity, right: -1, editorHeight: 999, consoleWidth: "bad", rightPanels: 99 }), { left: 220, right: 190, editorHeight: 78, consoleWidth: 50 });
assert.equal(utils.sanitizeLayout({ rightPanels: 5 }).rightPanels, 5);
for (const area of ["left", "right", "bottom"]) {
  const half = utils.MINIMUM_SIZE[area] / 2;
  assert.equal(utils.shouldSnap(area, half - 0.1), true);
  assert.equal(utils.shouldSnap(area, half), false);
  assert.equal(utils.shouldSnap(area, NaN), false);
}
assert.equal(utils.nextMaximized(true, true), true);
assert.equal(utils.nextMaximized(true, false), false);
assert.equal(utils.nextMaximized(true, true, true), false);
assert.equal(utils.nextMaximized(false, true), false);
assert.equal(Object.hasOwn(settings.sanitizeSettings({ version: 1, layout: { maximized: true } }).layout, "maximized"), false);

// Exercise the actual registrations, shortcut conflicts and shared pointer helper without a browser.
const built = buildSync({ stdin: { contents: `import registry from './frontend/registry.js'; import './frontend/shortcut_registry.js'; import './frontend/layout.js'; import './frontend/bootstrap.js'; globalThis.layoutTest = registry;`, resolveDir: process.cwd() }, bundle: true, write: false, platform: "node", format: "cjs", packages: "external" });
const bodyClasses = new Set();
const node = { classList: { add() {}, remove() {} }, setAttribute() {}, setPointerCapture() {}, hasPointerCapture: () => true, releasePointerCapture() {} };
const context = { console, require, navigator: { platform: "MacIntel" }, document: { body: { classList: { add(...names) { names.forEach((name) => bodyClasses.add(name)); }, remove(...names) { names.forEach((name) => bodyClasses.delete(name)); }, toggle(name, visible) { if (visible) bodyClasses.add(name); else bodyClasses.delete(name); } } } } };
vm.runInNewContext(built.outputFiles[0].text, context);
const registry = context.layoutTest;
const definitions = registry.shortcutDefinitions;
for (const [id, binding] of [["layout.primary", "Mod+KeyB"], ["layout.bottom", "Mod+KeyJ"], ["layout.secondary", "Mod+Alt+KeyB"]]) {
  for (const mac of [false, true]) {
    assert.equal(settings.findConflict(definitions, {}, id, binding, mac), null);
    const definition = definitions.find((item) => item.id === id);
    const event = { code: binding.endsWith("KeyJ") ? "KeyJ" : "KeyB", key: binding.includes("Alt") ? "∫" : binding.endsWith("KeyJ") ? "j" : "b", metaKey: mac, ctrlKey: !mac, altKey: binding.includes("Alt"), shiftKey: false };
    assert.equal(settings.matchesBinding(definition, binding, event, mac), true);
  }
}
assert.equal(settings.findConflict(definitions, {}, "global.font-increase", "Mod+KeyB").id, "layout.primary");
const rebound = settings.sanitizeSettings({ version: 1, shortcuts: { "global.font-increase": [{ binding: "Mod+KeyB" }] } }, definitions);
assert.equal(rebound.shortcuts["global.font-increase"], undefined, "old conflicting custom binding must restore the font command's default");
// CodeMirror's real default keymap has no Mod+B/J/Alt+B binding to move.
const keymap = fs.readFileSync(require.resolve("@codemirror/commands"), "utf8");
assert(!/key: ["']Mod-(?:b|j|Alt-b)["']/.test(keymap));
let persisted = 0, cancelled = 0, hidden = 0, reset = 0;
Object.assign(registry, { $: () => node, persistLayout() { persisted++; }, applyLayout() { cancelled++; }, setLayoutPanel(area, visible) { assert.equal(area, "left"); assert.equal(visible, false); hidden++; }, resetLayoutSize() { reset++; } });
registry.divider("#test", "x", (delta, drag, event) => drag ? event.clientX : 200 + delta, { area: "left", snap: true });
const pointer = { button: 0, pointerId: 1, clientX: 200, preventDefault() {} };
node.onpointerdown(pointer);
node.onpointermove({ clientX: 30 });
assert(bodyClasses.has("layout-snap-left"));
assert.equal(hidden, 0, "snap persisted before release");
node.onpointerup();
node.onlostpointercapture();
assert.equal(hidden, 1);
assert.equal(bodyClasses.size, 0);
node.onpointerdown(pointer);
node.onpointermove({ clientX: 30 });
node.onpointermove({ clientX: 210 });
assert(!bodyClasses.has("layout-snap-left"));
node.onpointerup();
assert.equal(persisted, 1);
node.onpointerdown(pointer);
node.onpointermove({ clientX: 30 });
node.onpointercancel();
assert.equal(cancelled, 1);
assert.equal(bodyClasses.size, 0);
node.onkeydown({ key: "ArrowRight", preventDefault() {} });
assert.equal(persisted, 2);
node.ondblclick({ preventDefault() {} });
assert.equal(reset, 1);
console.log("LAYOUT UNIT PASS: v1 migration, visibility/restore masks, bounded sizes, snap thresholds, transient maximise, shortcut conflicts, pointer completion/cancellation and keyboard/double-click handlers.");
