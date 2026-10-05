import shared from "./state.js";
import registry from "./registry.js";
import shortcutUtils from "./shortcut_registry_utils.cjs";
import { t } from "./i18n.js";

const definitions = [];
const mac = /Mac|iPhone|iPad/.test(globalThis.navigator?.platform || globalThis.navigator?.userAgent || "");
let installed = false;
const bindingCache = new WeakMap();

function registerShortcut(definition) {
  if (!definition?.id || definitions.some((item) => item.id === definition.id)) throw new Error(t("Shortcut IDs must be unique: {id}", { id: definition?.id }));
  definitions.push({ configurable: true, ...definition, bindings: [...definition.bindings] });
}

function activeBindings(definition) {
  const source = shared.settings?.shortcuts?.[definition.id];
  const cached = bindingCache.get(definition);
  if (cached && cached.source === source) return cached.bindings;
  const valid = definition.configurable !== false && Array.isArray(source) && source.length > 0 && source.length <= shortcutUtils.MAX_BINDINGS && source.every((item) => item && typeof item.binding === "string" && !shortcutUtils.reservedReason(item.binding, definition, null, mac, t));
  const bindings = valid ? source.map((item) => item.binding) : definition.bindings;
  bindingCache.set(definition, { source, bindings, override: valid });
  return bindings;
}

function shortcutScope(event) {
  if (document.querySelector("dialog[open]")) return null;
  const target = event.target;
  if (target?.closest?.("#editor")) return "editor";
  if (target === registry.$("#command")) return "command-line";
  if (target?.isContentEditable || target?.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return null;
  if (target?.closest?.("#left-panel")) return "file-list";
  if (target?.closest?.("#right-panel")) return "workspace";
  return "global";
}

function handleShortcut(event) {
  if (event.defaultPrevented || event.isComposing) return false;
  const scope = shortcutScope(event);
  if (!scope) return false;
  for (const definition of definitions) {
    if (definition.scope !== "global" && definition.scope !== scope) continue;
    const bindings = activeBindings(definition);
    if (bindingCache.get(definition).override && event.getModifierState?.("AltGraph")) continue;
    if (!bindings.some((binding) => shortcutUtils.matchesBinding(definition, binding, event, mac, bindingCache.get(definition).override))) continue;
    if (definition.when && !definition.when(event)) continue;
    const handled = definition.command(event);
    if (handled === false) continue;
    event.preventDefault();
    event.stopPropagation();
    return true;
  }
  return false;
}

function setupShortcutRegistry() {
  if (installed) return;
  installed = true;
  window.addEventListener("keydown", (event) => {
    if (event.isComposing) return;
    if (handleShortcut(event)) return;
    if (["F5", "F9"].includes(event.key) || (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") event.preventDefault();
  }, true);
}

Object.assign(registry, { registerShortcut, shortcutDefinitions: definitions, activeShortcutBindings: activeBindings, handleShortcut, setupShortcutRegistry, shortcutUtils, shortcutPlatformMac: mac });
for (const definition of registry.pendingShortcuts.splice(0)) registerShortcut(definition);
