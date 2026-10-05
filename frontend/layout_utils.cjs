const DEFAULT_LAYOUT = Object.freeze({ left: 220, right: 300, editorHeight: 56, consoleWidth: 50 });
const RIGHT_PANELS = Object.freeze(["workspace", "history", "debugger"]);
const MINIMUM_SIZE = Object.freeze({ left: 140, right: 190, bottom: 100 });

function bounded(value, fallback, minimum, maximum) {
  const number = Number(value);
  return Number.isFinite(number) ? Math.max(minimum, Math.min(maximum, number)) : fallback;
}

function sanitizeLayout(value) {
  const layout = {
    left: bounded(value?.left, 220, 140, 360),
    right: bounded(value?.right, 300, 190, 500),
    editorHeight: bounded(value?.editorHeight, 56, 20, 78),
    consoleWidth: bounded(value?.consoleWidth, 50, 20, 80)
  };
  // Optional until the secondary sidebar is first hidden; old v1 stores need no rewrite.
  if (Number.isInteger(value?.rightPanels) && value.rightPanels >= 1 && value.rightPanels <= 7) layout.rightPanels = value.rightPanels;
  return layout;
}

function panelVisible(panels, area) {
  if (area === "left") return panels.files;
  if (area === "right") return RIGHT_PANELS.some((name) => panels[name]);
  return panels.bottom !== false;
}

function setPanelVisible(settings, area, visible) {
  if (area === "left") settings.panels.files = visible;
  else if (area === "bottom") settings.panels.bottom = visible;
  else if (area === "right") {
    if (!visible && panelVisible(settings.panels, "right")) settings.layout.rightPanels = RIGHT_PANELS.reduce((mask, name, index) => mask | (settings.panels[name] ? 1 << index : 0), 0);
    const mask = settings.layout.rightPanels || 7;
    RIGHT_PANELS.forEach((name, index) => { settings.panels[name] = visible && !!(mask & 1 << index); });
  }
  return settings;
}

function shouldSnap(area, size) {
  return Number.isFinite(size) && size < MINIMUM_SIZE[area] / 2;
}

function nextMaximized(current, bottomVisible, editorChanged = false) {
  return current && bottomVisible && !editorChanged;
}

module.exports = { DEFAULT_LAYOUT, RIGHT_PANELS, MINIMUM_SIZE, sanitizeLayout, panelVisible, setPanelVisible, shouldSnap, nextMaximized };
