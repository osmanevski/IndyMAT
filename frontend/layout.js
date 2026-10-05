import shared from "./state.js";
import registry from "./registry.js";
import utils from "./layout_utils.cjs";
import { applyStaticTranslations, onLanguageChange } from "./i18n.js";

let maximized = false;
let editorHeight = null;

function applyWindowLayout() {
  const panels = shared.settings.panels;
  maximized = utils.nextMaximized(maximized, utils.panelVisible(panels, "bottom"), editorHeight !== null && editorHeight !== shared.settings.layout.editorHeight);
  editorHeight = shared.settings.layout.editorHeight;
  document.body.classList.toggle("hide-bottom-panel", !utils.panelVisible(panels, "bottom"));
  document.body.classList.toggle("panel-maximized", maximized);
  for (const area of ["left", "bottom", "right"]) registry.$(`#toggle-layout-${area}`)?.setAttribute("aria-pressed", String(utils.panelVisible(panels, area)));
  const button = registry.$("#maximize-panel");
  if (button) {
    const label = maximized ? "Restore Panel Size" : "Maximize Panel Size";
    button.dataset.i18nTitle = label;
    button.dataset.i18nAriaLabel = label;
    button.dataset.i18nTerm = label;
    button.setAttribute("aria-pressed", String(maximized));
    applyStaticTranslations(button.parentElement);
  }
  // A hidden focused region must not keep receiving keystrokes.
  const active = document.activeElement;
  const hiddenArea = active?.closest?.("#left-panel, #right-panel, .bottom-panels, .editor-panel, .editor-intel-toolbar, #editor-intel-panel, #left-divider, #right-divider, #editor-divider, #plot-divider");
  if (hiddenArea && !hiddenArea.getClientRects().length) {
    if (maximized) registry.$("#command")?.focus();
    else shared.editor?.focus();
  }
}

function syncLayoutCheckboxes() {
  for (const input of document.querySelectorAll("input[data-layout-panel]")) input.checked = shared.settings.panels[input.dataset.layoutPanel];
}

function setLayoutPanel(area, visible) {
  utils.setPanelVisible(shared.settings, area, visible);
  if (area === "bottom") maximized = false;
  registry.saveSettings();
  registry.applySettings();
  syncLayoutCheckboxes();
}

function toggleLayoutPanel(area) {
  setLayoutPanel(area, !utils.panelVisible(shared.settings.panels, area));
}

function restorePanelSize() {
  maximized = false;
  applyWindowLayout();
}

function resetLayoutSize(area) {
  restorePanelSize();
  const key = { left: "left", right: "right", bottom: "editorHeight", plot: "consoleWidth" }[area];
  shared.settings.layout[key] = utils.DEFAULT_LAYOUT[key];
  registry.saveSettings();
  registry.applyLayout();
}

function setupLayout() {
  for (const area of ["left", "bottom", "right"]) registry.on(`#toggle-layout-${area}`, () => toggleLayoutPanel(area));
  registry.on("#maximize-panel", () => {
    maximized = !maximized;
    applyWindowLayout();
  });
  onLanguageChange(applyWindowLayout);
  applyWindowLayout();
}

Object.assign(registry, { setupLayout, applyWindowLayout, toggleLayoutPanel, setLayoutPanel, resetLayoutSize, restorePanelSize, syncLayoutCheckboxes });
registry.registerShortcut({ id: "layout.primary", label: "Toggle Primary Side Bar", scope: "global", scopeLabel: "General", bindings: ["Mod+KeyB"], command: () => toggleLayoutPanel("left") });
registry.registerShortcut({ id: "layout.bottom", label: "Toggle Panel", scope: "global", scopeLabel: "General", bindings: ["Mod+KeyJ"], command: () => toggleLayoutPanel("bottom") });
registry.registerShortcut({ id: "layout.secondary", label: "Toggle Secondary Side Bar", scope: "global", scopeLabel: "General", bindings: ["Mod+Alt+KeyB"], command: () => toggleLayoutPanel("right") });
