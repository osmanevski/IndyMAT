import { t, onLanguageChange } from "./i18n.js";

export function mountFigureWindowControls(wrap, resetView) {
  const controls = document.createElement("div");
  controls.className = "figure-window-controls";
  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "figure-window-reset";
  reset.dataset.i18n = "Reset";
  reset.dataset.i18nTitle = "Reset the figure view";
  reset.dataset.i18nAriaLabel = "Reset the figure view";
  const fullscreen = document.createElement("button");
  fullscreen.type = "button";
  fullscreen.className = "figure-window-fullscreen";
  fullscreen.dataset.i18n = "Fullscreen";
  const update = () => {
    const active = document.fullscreenElement === target();
    const source = active ? "Exit Fullscreen" : "Fullscreen";
    const label = active ? t("Exit Fullscreen") : t("Fullscreen");
    fullscreen.textContent = label;
    fullscreen.title = label;
    fullscreen.setAttribute("aria-label", label);
    fullscreen.dataset.i18n = source;
    fullscreen.dataset.i18nTitle = source;
    fullscreen.dataset.i18nAriaLabel = source;
    const supported = typeof target().requestFullscreen === "function" && document.fullscreenEnabled !== false;
    fullscreen.disabled = !supported && !active;
    fullscreen.hidden = !supported && !active;
    reset.textContent = t("Reset");
    reset.title = t("Reset the figure view");
    reset.setAttribute("aria-label", t("Reset the figure view"));
  };
  const target = () => wrap.closest(".figure-expanded") || wrap;
  const stop = (event) => event.stopPropagation();
  const toggleFullscreen = async () => {
    const host = target();
    try {
      if (document.fullscreenElement === host) await document.exitFullscreen();
      else if (host.requestFullscreen) await host.requestFullscreen();
    } catch { fullscreen.hidden = true; }
  };
  reset.addEventListener("pointerdown", stop);
  reset.addEventListener("click", (event) => { stop(event); resetView(); });
  fullscreen.addEventListener("pointerdown", stop);
  fullscreen.addEventListener("click", (event) => { stop(event); void toggleFullscreen(); });
  document.addEventListener("fullscreenchange", update);
  const unsubscribe = onLanguageChange(update);
  controls.append(reset, fullscreen);
  wrap.append(controls);
  update();
  return () => {
    unsubscribe();
    document.removeEventListener("fullscreenchange", update);
    controls.remove();
  };
}
