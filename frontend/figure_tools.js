import tools from "./figure_tool_utils.cjs";
import { t, onLanguageChange, applyStaticTranslations } from "./i18n.js";

let mounted = null;

export function figureReasonText(code, args = {}) {
  // Validator errors need not carry the producer's optional detail fields.
  const details = { budget: "?", actual: "?", limit: 8388608, type: "?", marker: "?", property: "?", ...args };
  switch (code) {
    case "no_axes": return t("Only the PNG view is available: no data axes.");
    case "budget_exceeded": return t("Only the PNG view is available: {budget} exceeds the limit ({actual} > {limit}).", details);
    case "invalid_data": return t("Only the PNG view is available: invalid figure data.");
    case "unsupported_object": return t("Only the PNG view is available: unsupported object ({type}).", details);
    case "unsupported_group": return t("Only the PNG view is available: unsupported graphics group ({type}).", details);
    case "unsupported_marker": return t("Only the PNG view is available: unsupported marker ({marker}).", details);
    case "unsupported_color": return t("Only the PNG view is available: unsupported color.");
    case "scatter_colors": return t("Only the PNG view is available: per-point scatter colors.");
    case "transparency": return t("Only the PNG view is available: transparency.");
    case "lighting": return t("Only the PNG view is available: lighting.");
    case "interpolated_color": return t("Only the PNG view is available: interpolated colors.");
    case "unsupported_surface": return t("Only the PNG view is available: unsupported surface configuration.");
    case "log_3d": return t("Only the PNG view is available: logarithmic 3D axes.");
    case "perspective": return t("Only the PNG view is available: perspective projection.");
    case "manual_camera": return t("Only the PNG view is available: manual camera ({property}).", details);
    case "unsupported_units": return t("Only the PNG view is available: unsupported axes units or parent.");
    case "unsupported_colorbar": return t("Only the PNG view is available: unsupported colorbar configuration.");
    case "json_budget": return t("Only the PNG view is available: figure JSON exceeds {limit} bytes.", details);
    case "unknown_version": return t("Only the PNG view is available: unknown figure data version.");
    case "invalid_index": return t("Only the PNG view is available: invalid figure data indices.");
    case "webgl_unavailable": return t("Only the PNG view is available: WebGL2 is unavailable.");
    case "shader_failure": return t("Only the PNG view is available: the 3D renderer could not start.");
    case "context_lost": return t("Only the PNG view is available: the graphics context was lost.");
    default: return t("Only the PNG view is available.");
  }
}

export function figureReductionText(original, rendered) {
  return t("Interactive data reduced from {original} to {rendered} points.", { original, rendered });
}

export function figureCanvasDescription() {
  return t("Interactive 3D figure. Use Figure tools to rotate, zoom, pan or pin data tips. Arrow keys rotate, Shift and arrows pan, plus and minus zoom, Home resets, and Escape cancels or clears tips.");
}

function instructions(mode) {
  switch (mode) {
    case "zoom": return t("Drag vertically or use the wheel to zoom. Shift-drag or middle-drag pans.");
    case "pan": return t("Drag to pan. Use the wheel to zoom.");
    case "tips": return t("Click a visible sample or surface vertex to pin a data tip. Escape clears tips. Shift-drag or middle-drag pans.");
    default: return t("Drag to rotate. Use the wheel to zoom. Shift-drag or middle-drag pans.");
  }
}

function tipLabels() {
  return { series: t("Series"), index: t("Index"), row: t("Row"), column: t("Column"), color: t("Color value") };
}

// container is the positioned viewer host, whose origin is used by pick/zoom.
// target = {orbit(dx,dy), rotateAzimuth(deg), pan(dx,dy), zoom(factor,anchor),
//           reset(), pick(x,y)->hit|null, requestRender()}. All calls are local.
export function mountFigureTools(container, target) {
  for (const method of ["orbit", "rotateAzimuth", "pan", "zoom", "reset", "pick", "requestRender"]) {
    if (typeof target?.[method] !== "function") throw new TypeError("Missing figure target method: " + method);
  }
  unmountFigureTools();
  const doc = container.ownerDocument;
  const controls = doc.querySelector("#figure-tools");
  const selector = controls?.querySelector("#figure-tool-select");
  const radios = [...(controls?.querySelectorAll("input[data-figure-mode]") || [])];
  const help = controls?.querySelector("#figure-tool-instructions");
  const tips = doc.createElement("div");
  tips.className = "figure-pinned-tips";
  tips.setAttribute("aria-live", "polite");
  container.append(tips);
  const saved = new Map(["tabindex", "aria-label", "role", "data-figure-mode"].map((name) => [name, container.getAttribute(name)]));
  container.tabIndex = 0;
  container.setAttribute("role", "group");
  container.classList.add("figure-tool-viewer");
  let state = tools.initialState();
  let renderedPins = null;
  let active = true;
  const listeners = [];
  const listen = (node, name, fn, options) => {
    node.addEventListener(name, fn, options);
    listeners.push(() => node.removeEventListener(name, fn, options));
  };
  const release = (pointerId) => {
    if (container.hasPointerCapture(pointerId)) container.releasePointerCapture(pointerId);
  };
  const point = (event) => {
    const bounds = container.getBoundingClientRect();
    return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
  };
  const center = () => ({ x: container.clientWidth / 2, y: container.clientHeight / 2 });
  const refresh = (refreshTips = false) => {
    container.setAttribute("aria-label", figureCanvasDescription());
    container.dataset.figureMode = state.gesture?.mode || state.mode;
    if (controls) {
      controls.hidden = false;
      applyStaticTranslations(controls);
    }
    if (selector) selector.value = state.mode;
    for (const radio of radios) radio.checked = radio.dataset.figureMode === state.mode;
    if (help) help.textContent = instructions(state.mode);
    if (!refreshTips && renderedPins === state.pins) return;
    renderedPins = state.pins;
    tips.replaceChildren();
    for (const pin of state.pins) {
      const tip = doc.createElement("div");
      tip.className = "figure-pinned-tip";
      tip.textContent = tools.tipText(pin.hit, tipLabels());
      tips.append(tip);
      tip.style.left = Math.max(2, Math.min(container.clientWidth - tip.offsetWidth - 2, pin.anchor.x + 9)) + "px";
      tip.style.top = Math.max(2, Math.min(container.clientHeight - tip.offsetHeight - 2, pin.anchor.y + 9)) + "px";
    }
  };
  const dispatch = (event) => {
    if (!active) return;
    const result = tools.transition(state, event);
    state = result.state;
    let redraw = false;
    for (const action of result.actions) {
      switch (action.type) {
        case "capture":
          container.setPointerCapture(action.pointerId);
          break;
        case "release":
          release(action.pointerId);
          break;
        case "orbit":
          target.orbit(action.dx, action.dy);
          redraw = true;
          break;
        case "rotateAzimuth":
          target.rotateAzimuth(action.deg);
          redraw = true;
          break;
        case "pan":
          target.pan(action.dx, action.dy);
          redraw = true;
          break;
        case "zoom":
          target.zoom(action.factor, action.anchor);
          redraw = true;
          break;
        case "reset":
          target.reset();
          redraw = true;
          break;
        case "pick": {
          const hit = target.pick(action.x, action.y);
          state = tools.transition(state, { type: "pin", hit, anchor: { x: action.x, y: action.y } }).state;
          break;
        }
      }
    }
    refresh();
    if (redraw) target.requestRender();
  };
  for (const radio of radios) listen(radio, "change", () => {
    if (radio.checked) dispatch({ type: "mode", mode: radio.dataset.figureMode });
  });
  if (selector) listen(selector, "change", () => dispatch({ type: "mode", mode: selector.value }));
  listen(container, "pointerdown", (event) => {
    if (state.gesture || ![0, 1].includes(event.button)) return;
    event.preventDefault();
    container.focus({ preventScroll: true });
    dispatch({ type: "down", ...point(event), pointerId: event.pointerId, button: event.button, shiftKey: event.shiftKey });
  });
  for (const [name, type] of [["pointermove", "move"], ["pointerup", "up"], ["pointercancel", "cancel"], ["lostpointercapture", "cancel"]]) {
    listen(container, name, (event) => {
      if (state.gesture?.pointerId !== event.pointerId) return;
      event.preventDefault();
      dispatch({ type, ...point(event), pointerId: event.pointerId, shiftKey: event.shiftKey });
    });
  }
  listen(container, "wheel", (event) => {
    event.preventDefault();
    const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? container.clientHeight : 1;
    dispatch({ type: "wheel", delta: event.deltaY * unit, anchor: point(event) });
  }, { passive: false });
  listen(container, "keydown", (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "+", "=", "-", "_", "Home", "Escape"].includes(event.key)) return;
    event.preventDefault();
    event.stopPropagation();
    dispatch({ type: "key", key: event.key, shiftKey: event.shiftKey, anchor: center() });
  });
  const unsubscribe = onLanguageChange(() => refresh(true));
  const resize = typeof ResizeObserver === "function" ? new ResizeObserver(() => refresh(true)) : null;
  resize?.observe(container);
  mounted = {
    reset: () => dispatch({ type: "reset" }),
    setMode: (mode) => dispatch({ type: "mode", mode }),
    getState: () => structuredClone(state),
    dispose: () => {
      active = false;
      listeners.forEach((remove) => remove());
      unsubscribe();
      resize?.disconnect();
      if (state.gesture) release(state.gesture.pointerId);
      tips.remove();
      if (controls) controls.hidden = true;
      container.classList.remove("figure-tool-viewer");
      for (const [name, value] of saved) {
        if (value === null) container.removeAttribute(name);
        else container.setAttribute(name, value);
      }
    }
  };
  refresh();
  return { reset: mounted.reset, setMode: mounted.setMode, getState: mounted.getState };
}

export function unmountFigureTools() {
  mounted?.dispose();
  mounted = null;
}
