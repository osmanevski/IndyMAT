import shared from "./state.js";
import registry from "./registry.js";
import { t } from "./i18n.js";

// GET /api/state lists the features the active engine does not provide
// (state.unsupported). Their entry points are not offered at all instead of
// being refused by the server. GNU Octave sends no list, which gates nothing.
function setUnsupported(list) {
  const features = Array.isArray(list) ? list.filter((item) => typeof item === "string") : [];
  const key = JSON.stringify(features);
  if (key === shared.unsupportedKey) return false;
  shared.unsupportedKey = key;
  shared.unsupported = new Set(features);
  applyEngineGating();
  return true;
}
function applyEngineGating() {
  const off = registry.unsupported;
  const hide = (selector, hidden) => {
    for (const node of document.querySelectorAll(selector)) node.hidden = hidden;
  };
  hide("#run-profile", off("profiler"));
  hide("#publish", off("publish"));
  hide(".run-more", off("profiler") && off("publish"));
  hide("#run-to-cursor", off("run_to_cursor"));
  hide('[data-action="packages"]', off("packages"));
  document.body.classList.toggle("breakpoints-unsupported", off("breakpoints"));
  shared.debugPanelKey = "";
  registry.renderDebuggerPanel?.(shared.engine);
  registry.updateWorkspaceActions?.();
  registry.updateRotateControls?.();
  registry.refreshLintGating?.();
}
// `state.engine` exists only on the experimental Rust engine.
function syncEngine(state) {
  document.body.classList.toggle("engine-rust", !!state.engine);
  setUnsupported(state.unsupported);
  if (state.engine_notice && state.engine_notice !== shared.engineNotice) registry.toast(state.engine_notice);
  shared.engineNotice = state.engine_notice || "";
}
// The compute engine lives in the server's session, not in browser settings: GET/POST /api/engine.
function appendEngineSection(section, preferenceRow, selectControl) {
  const active = shared.engine.engine ? "rust" : "octave";
  const note = registry.el("p", "engine-note", t("GNU Octave is the default and recommended engine. The Rust engine is in development: some features are not available yet."));
  const select = selectControl([["octave", "GNU Octave"], ["rust", t("Rust engine (experimental)")]], active, (value) => registry.safe(async () => {
    try {
      const info = await registry.api("engine");
      if (!info.available[value]) {
        note.textContent = info.problems[value];
        throw new Error(info.problems[value]);
      }
      if (!confirm(t("Switch the compute engine and restart the session? Variables and figures in memory will be cleared; your files will be kept."))) {
        select.value = active;
        return;
      }
      shared.uiGeneration++;
      shared.publishRenders.clear();
      await registry.api("engine", { engine: value, confirm: true });
      // A poll sent during the switch answers for the kernel that was closing.
      shared.uiGeneration++;
      shared.lastJob = null;
      shared.lastFiguresKey = "";
      registry.poll.completed = null;
      shared.activeOutput = null;
      registry.$("#modal").close();
      registry.toast(value === "rust" ? t("Starting a new session on the Rust engine.") : t("Starting a new Octave session."));
    } catch (error) {
      select.value = active;
      throw error;
    }
  }));
  select.dataset.setting = "computeEngine";
  section.append(registry.el("h3", "", t("Compute Engine")), preferenceRow(t("Engine"), select), note);
}

Object.assign(registry, { setUnsupported, applyEngineGating, syncEngine, appendEngineSection });
