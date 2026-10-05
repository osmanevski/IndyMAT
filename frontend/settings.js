import { t, setLanguage, applyStaticTranslations, onLanguageChange } from "./i18n.js";
import shared from "./state.js";
import registry from "./registry.js";
import shortcutUtils from "./shortcut_registry_utils.cjs";
// Language names are shown in their own language and are never translated.
const LANGUAGE_NAME_TR = "T\u00FCrk\u00E7e";

let storageAvailable = true;
let storageNoticeShown = false;
let sessionLayoutFallback = false;
let activeCaptureCleanup = null;
let storage = null;
let baseline = shortcutUtils.cloneDefaults();
const pendingWrites = [];
let writeQueue = Promise.resolve();

function settingsSnapshot(value) {
  return shortcutUtils.sanitizeSettings(value, registry.shortcutDefinitions, registry.shortcutPlatformMac, t);
}

function receiveSettings(value, refreshDialog = true) {
  for (const pending of pendingWrites) shortcutUtils.applyPatch(value, pending.patch);
  const next = settingsSnapshot(value);
  const changed = shortcutUtils.settingsPatch(shared.settings, next).length > 0;
  const figureChanged = JSON.stringify(shared.settings.activeTabs) !== JSON.stringify(next.activeTabs);
  shared.settings = next;
  baseline = settingsSnapshot(shared.settings);
  syncPreferenceMirrors(next.preferences);
  if (!changed) return;
  applySettings();
  if (figureChanged) {
    shared.figureIndex = Math.max(0, shared.figures.findIndex((figure) => figure.name === next.activeTabs.figureName));
    registry.renderFigures?.();
  }
  if (refreshDialog && registry.$("#modal")?.open && registry.$(".settings-dialog")?.isConnected) openSettings();
}

function storageNotice() {
  if (storageNoticeShown) return;
  storageNoticeShown = true;
  queueMicrotask(() => registry.toast(t("Settings cannot be saved in the browser; they will stay in memory for this session.")));
}

function syncPreferenceMirrors(preferences, previous = null, missingOnly = false) {
  try {
    shortcutUtils.writePreferenceMirrors(storage, preferences, previous, missingOnly);
  } catch {
    storageNotice();
  }
}

function saveSettings() {
  if (!storageAvailable) return storageNotice();
  const next = settingsSnapshot(shared.settings);
  // Mirrors must update in this event, before the asynchronous locked commit.
  syncPreferenceMirrors(next.preferences, baseline.preferences);
  const pending = { patch: shortcutUtils.settingsPatch(baseline, next) };
  baseline = next;
  pendingWrites.push(pending);
  const commit = () => {
    if (!storageAvailable) {
      pendingWrites.splice(pendingWrites.indexOf(pending), 1);
      return;
    }
    const result = shortcutUtils.commitPatch(storage, pending.patch, registry.shortcutDefinitions, registry.shortcutPlatformMac, t);
    pendingWrites.splice(pendingWrites.indexOf(pending), 1);
    if (!result.available) {
      storageAvailable = false;
      storageNotice();
      return;
    }
    receiveSettings(result.settings, false);
  };
  // The read/merge/write is serialized across same-origin tabs where Web Locks
  // is available. Field patches also protect against stale, delayed events.
  writeQueue = writeQueue.then(() => globalThis.navigator?.locks ? navigator.locks.request(shortcutUtils.SETTINGS_KEY, commit) : commit()).catch(() => {
    storageAvailable = false;
    storageNotice();
  });
  return writeQueue;
}

function getSetting(section, name) {
  return shared.settings?.[section]?.[name];
}

function updateSetting(section, name, value, apply = true) {
  shared.settings[section][name] = value;
  saveSettings();
  if (apply) applySettings();
}

function effectiveLayout() {
  return sessionLayoutFallback ? shortcutUtils.cloneDefaults().layout : shared.settings.layout;
}

function applyPanelVisibility() {
  const panels = shared.settings.panels;
  document.body.classList.toggle("hide-current-folder", !panels.files);
  document.body.classList.toggle("hide-workspace", !panels.workspace);
  document.body.classList.toggle("hide-command-history", !panels.history);
  document.body.classList.toggle("hide-figures", !panels.figures);
  document.body.classList.toggle("hide-debugger", !panels.debugger);
  document.body.classList.toggle("hide-right-panels", !panels.workspace && !panels.history && !panels.debugger);
  registry.$("#right-panel")?.querySelector("#debugger-panel + .panel-heading")?.classList.add("workspace-heading");
  registry.applyWindowLayout?.();
  registry.syncLayoutCheckboxes?.();
}

function applyLayout() {
  // Keep the responsive stylesheet authoritative across its entire range.
  const panels = shared.settings.panels;
  const sideWidth = (panels.files ? shared.settings.layout.left : 0) + (panels.workspace || panels.history || panels.debugger ? shared.settings.layout.right : 0);
  sessionLayoutFallback = innerWidth <= 1150 || sideWidth + 314 > innerWidth;
  const consolePanel = registry.$(".console-panel");
  if (sessionLayoutFallback) {
    for (const property of ["--left", "--right", "--editor-height"]) document.documentElement.style.removeProperty(property);
    consolePanel?.style.removeProperty("flex");
    return;
  }
  const layout = effectiveLayout();
  document.documentElement.style.setProperty("--left", layout.left + "px");
  document.documentElement.style.setProperty("--right", layout.right + "px");
  document.documentElement.style.setProperty("--editor-height", layout.editorHeight + "%");
  if (consolePanel) {
    if (shared.settings.panels.figures) consolePanel.style.flex = `0 0 ${layout.consoleWidth}%`;
    else consolePanel.style.removeProperty("flex");
  }
}

function applySettings() {
  const preferences = shared.settings.preferences;
  setLanguage(preferences.language);
  shared.editorFontSize = preferences.editorFontSize;
  document.documentElement.style.setProperty("--editor-font-size", preferences.editorFontSize + "px");
  document.documentElement.style.setProperty("--console-font-size", preferences.consoleFontSize + "px");
  if (shared.editor && registry.applyTheme) registry.applyTheme(preferences.theme === "dark", false);
  else document.body.classList.toggle("dark", preferences.theme === "dark");
  shared.plotMode = shared.settings.activeTabs.plotMode;
  registry.applyAssistantLayout?.();
  applyPanelVisibility();
  applyLayout();
}

function persistLayout() {
  if (sessionLayoutFallback) return;
  const rootStyle = getComputedStyle(document.documentElement), bottom = registry.$(".bottom-panels")?.getBoundingClientRect(), consolePanel = registry.$(".console-panel");
  shared.settings.layout.left = parseFloat(rootStyle.getPropertyValue("--left")) || 220;
  shared.settings.layout.right = parseFloat(rootStyle.getPropertyValue("--right")) || 300;
  shared.settings.layout.editorHeight = parseFloat(rootStyle.getPropertyValue("--editor-height")) || 56;
  if (shared.settings.panels.bottom && shared.settings.panels.figures && bottom?.width && consolePanel) shared.settings.layout.consoleWidth = Math.max(20, Math.min(80, consolePanel.getBoundingClientRect().width / bottom.width * 100));
  saveSettings();
}

function persistActiveTab(name, value) {
  shared.settings.activeTabs[name] = value;
  saveSettings();
}

function preferenceRow(labelText, control) {
  const label = registry.el("label", "settings-field");
  label.append(registry.el("span", "", labelText), control);
  return label;
}

function selectControl(values, current, onChange) {
  const select = registry.el("select");
  for (const [value, label] of values) {
    const option = registry.el("option", "", label);
    option.value = value;
    option.selected = String(current) === value;
    select.append(option);
  }
  select.onchange = () => onChange(select.value);
  return select;
}

function numberControl(value, minimum, maximum, onChange) {
  const input = registry.el("input");
  input.type = "number";
  input.min = String(minimum);
  input.max = String(maximum);
  input.value = String(value);
  input.onchange = () => {
    const next = Math.max(minimum, Math.min(maximum, Number(input.value) || value));
    input.value = String(next);
    onChange(next);
  };
  return input;
}

function checkboxRow(labelText, checked, onChange) {
  const label = registry.el("label", "settings-check"), input = registry.el("input");
  input.type = "checkbox";
  input.checked = checked;
  input.onchange = () => onChange(input.checked);
  label.append(input, registry.el("span", "", labelText));
  return label;
}

function renderShortcutTable(root, query = "") {
  root.replaceChildren();
  const normalized = query.toLocaleLowerCase("tr");
  for (const definition of registry.shortcutDefinitions.filter((item) => item.configurable !== false && t(item.label).toLocaleLowerCase("tr").includes(normalized))) {
    const row = registry.el("div", "shortcut-row"), label = registry.el("div", "shortcut-name", t(definition.label)), scope = registry.el("div", "shortcut-scope", t(definition.scopeLabel || definition.scope)), bindings = registry.el("div", "shortcut-binding"), capture = registry.el("button", "", t("Change")), restore = registry.el("button", "", t("Default"));
    const values = shared.settings.shortcuts[definition.id] || definition.bindings.map((binding) => ({ binding, label: "" }));
    bindings.textContent = values.map((item) => registry.shortcutUtils.displayBinding(item.binding, registry.shortcutPlatformMac, item.label, t)).join(" · ");
    capture.setAttribute("aria-label", t("Change shortcut for {label}", { label: t(definition.label) }));
    capture.onclick = () => {
      activeCaptureCleanup?.();
      capture.textContent = t("Press keys…");
      capture.classList.add("capturing");
      const modal = registry.$("#modal");
      let listen;
      const cleanup = () => {
        window.removeEventListener("keydown", listen, true);
        modal.removeEventListener("close", cleanup);
        capture.classList.remove("capturing");
        capture.textContent = t("Change");
        if (activeCaptureCleanup === cleanup) activeCaptureCleanup = null;
      };
      activeCaptureCleanup = cleanup;
      modal.addEventListener("close", cleanup, { once: true });
      listen = (event) => {
        event.preventDefault();
        event.stopPropagation();
        if (["Shift", "Control", "Alt", "Meta"].includes(event.key)) return;
        cleanup();
        if (event.key === "Escape") return;
        const binding = registry.shortcutUtils.canonicalEvent(event, registry.shortcutPlatformMac, { ignoreShift: definition.ignoreShift });
        const reserved = registry.shortcutUtils.reservedReason(binding, definition, event, registry.shortcutPlatformMac, t);
        const conflict = registry.shortcutUtils.findConflict(registry.shortcutDefinitions, shared.settings.shortcuts, definition.id, binding, registry.shortcutPlatformMac);
        if (reserved) return registry.toast(reserved);
        if (conflict) return registry.toast(t("This key is used for “{label}”; change that shortcut first.", { label: t(conflict.label) }));
        shared.settings.shortcuts[definition.id] = [{ binding, label: registry.shortcutUtils.captureLabel(event, registry.shortcutPlatformMac) }];
        saveSettings();
        renderShortcutTable(root, query);
      };
      window.addEventListener("keydown", listen, true);
    };
    restore.disabled = !shared.settings.shortcuts[definition.id];
    restore.onclick = () => {
      delete shared.settings.shortcuts[definition.id];
      saveSettings();
      renderShortcutTable(root, query);
    };
    row.append(label, scope, bindings, capture, restore);
    root.append(row);
  }
  if (!root.children.length) root.append(registry.el("div", "settings-empty", t("No matching shortcuts.")));
}

function openSettings() {
  activeCaptureCleanup?.();
  const wrap = registry.el("div", "settings-dialog"), preferences = registry.el("section"), panels = registry.el("section"), shortcuts = registry.el("section"), actions = registry.el("div", "settings-actions");
  preferences.append(registry.el("h3", "", t("Appearance and Editor")));
  const languageRow = preferenceRow(t("Language"), selectControl([["system", "System"], ["en", "English"], ["tr", LANGUAGE_NAME_TR]], getSetting("preferences", "language"), (value) => updateSetting("preferences", "language", value)));
  languageRow.querySelector("span").setAttribute("data-i18n", "Language");
  languageRow.querySelector("select").setAttribute("data-i18n-aria-label", "Language");
  preferences.append(languageRow);
  preferences.append(preferenceRow(t("Theme"), selectControl([["dark", t("Dark")], ["light", t("Light")]], getSetting("preferences", "theme"), (value) => {
    updateSetting("preferences", "theme", value);
  })));
  preferences.append(preferenceRow(t("Editor font size"), numberControl(getSetting("preferences", "editorFontSize"), 10, 24, (value) => {
    updateSetting("preferences", "editorFontSize", value);
  })));
  preferences.append(preferenceRow(t("Command Window font size"), numberControl(getSetting("preferences", "consoleFontSize"), 10, 24, (value) => updateSetting("preferences", "consoleFontSize", value))));
  preferences.append(preferenceRow(t("Indent width"), numberControl(getSetting("preferences", "indentWidth"), 1, 8, (value) => updateSetting("preferences", "indentWidth", value, false))));
  preferences.append(preferenceRow(t("Smart Indent"), selectControl([["spaces", t("Spaces")], ["tabs", t("Tabs")]], getSetting("preferences", "useTabs") ? "tabs" : "spaces", (value) => updateSetting("preferences", "useTabs", value === "tabs", false))));
  panels.append(registry.el("h3", "", t("Panels")));
  for (const [name, label] of [["files", "Current Folder"], ["workspace", "Workspace"], ["history", "Command History"], ["figures", "Figures"], ["debugger", "Debugger (when needed)"], ["bottom", "Bottom panel"]]) {
    const row = checkboxRow(t(label), shared.settings.panels[name], (value) => updateSetting("panels", name, value));
    row.querySelector("input").dataset.layoutPanel = name;
    panels.append(row);
  }
  panels.append(checkboxRow(t("Assistant"), shared.settings.assistant.open, (value) => updateSetting("assistant", "open", value)));
  const resetLayout = registry.el("button", "", t("Reset Layout"));
  resetLayout.onclick = () => {
    registry.restorePanelSize?.();
    shared.settings.assistant = shortcutUtils.cloneDefaults().assistant;
    shared.settings.layout = shortcutUtils.cloneDefaults().layout;
    shared.settings.panels = shortcutUtils.cloneDefaults().panels;
    shared.settings.activeTabs = shortcutUtils.cloneDefaults().activeTabs;
    shared.figureIndex = 0;
    sessionLayoutFallback = false;
    saveSettings();
    applySettings();
    registry.renderFigures?.();
    registry.toast(t("Layout reset to defaults."));
    openSettings();
  };
  panels.append(resetLayout);
  shortcuts.append(registry.el("h3", "", t("Keyboard Shortcuts")));
  const search = registry.el("input", "shortcut-search"), table = registry.el("div", "shortcut-table");
  search.type = "search";
  search.placeholder = t("Search shortcuts");
  search.setAttribute("aria-label", t("Search shortcuts"));
  search.oninput = () => renderShortcutTable(table, search.value);
  shortcuts.append(search, table);
  renderShortcutTable(table);
  const restoreAll = registry.el("button", "danger-button", t("Restore All Settings to Defaults"));
  restoreAll.onclick = () => {
    if (!confirm(t("Restore all appearance, layout, and shortcut settings to their defaults?"))) return;
    shared.settings = shortcutUtils.cloneDefaults();
    registry.restorePanelSize?.();
    shared.figureIndex = 0;
    sessionLayoutFallback = false;
    saveSettings();
    applySettings();
    registry.renderFigures?.();
    registry.$("#modal").close();
    registry.toast(t("All settings reset to defaults."));
  };
  actions.append(restoreAll);
  wrap.append(preferences, panels, shortcuts, actions);
  registry.modal(t("Settings"), wrap);
  applyStaticTranslations(wrap);
}

function setupSettings() {
  try { storage = localStorage; } catch { storage = null; }
  const loaded = shortcutUtils.loadSettings(storage, registry.shortcutDefinitions, registry.shortcutPlatformMac, t);
  shared.settings = loaded.settings;
  baseline = settingsSnapshot(shared.settings);
  storageAvailable = loaded.available;
  applySettings();
  if (loaded.available) syncPreferenceMirrors(shared.settings.preferences, null, true);
  if (!loaded.available) storageNotice();
  if (loaded.migrated || loaded.missing) saveSettings();
  registry.on("#settings", openSettings);
  onLanguageChange(() => { if (registry.$("#modal")?.open && registry.$(".settings-dialog")?.isConnected) openSettings(); });
  window.addEventListener("resize", applyLayout);
  window.addEventListener("storage", (event) => {
    if (!storageAvailable) return;
    if (event.storageArea !== storage || event.key !== null && event.key !== shortcutUtils.SETTINGS_KEY) return;
    const latest = shortcutUtils.loadSettings(storage, registry.shortcutDefinitions, registry.shortcutPlatformMac, t);
    if (latest.available) receiveSettings(latest.settings);
  });
  document.addEventListener("click", (event) => {
    if (event.target.closest?.("#plot-interactive")) persistActiveTab("plotMode", "interactive");
    if (event.target.closest?.("#plot-png")) persistActiveTab("plotMode", "png");
  });
}

Object.assign(registry, { setupSettings, openSettings, saveSettings, getSetting, updateSetting, applySettings, applyPanelVisibility, applyLayout, persistLayout, persistActiveTab });
