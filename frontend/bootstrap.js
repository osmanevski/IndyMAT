import shared from "./state.js";
import registry from "./registry.js";
import "./editor_commands.js";
import { t } from "./i18n.js";
import layoutUtils from "./layout_utils.cjs";

function divider(id, axis, fn, options = {}) {
  const n = registry.$(id);
  n.setAttribute("aria-orientation", axis === "x" ? "vertical" : "horizontal");
  n.onpointerdown = (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    n.setPointerCapture(e.pointerId);
    n.classList.add("dragging");
    document.body.classList.add(axis === "x" ? "resizing-columns" : "resizing-rows");
    options.begin?.();
    const start = axis === "x" ? e.clientX : e.clientY;
    let active = true, snapped = false;
    n.onpointermove = (ev) => {
      const size = fn((axis === "x" ? ev.clientX : ev.clientY) - start, true, ev);
      snapped = options.snap && layoutUtils.shouldSnap(options.area, size);
      if (options.snap) document.body.classList.toggle(`layout-snap-${options.area}`, !!snapped);
    };
    const finish = (cancelled) => {
      if (!active) return;
      active = false;
      n.onpointermove = null;
      n.classList.remove("dragging");
      document.body.classList.remove("resizing-columns", "resizing-rows", `layout-snap-${options.area}`);
      if (n.hasPointerCapture(e.pointerId)) n.releasePointerCapture(e.pointerId);
      if (cancelled) registry.applyLayout();
      else if (snapped) registry.setLayoutPanel(options.area, false);
      else registry.persistLayout();
    };
    n.onpointerup = () => finish(false);
    n.onpointercancel = () => finish(true);
    n.onlostpointercapture = () => finish(true);
  };
  if (options.area) n.ondblclick = (e) => {
    e.preventDefault();
    registry.resetLayoutSize(options.area);
  };
  n.onkeydown = (e) => {
    if (["ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown"].includes(e.key)) {
      e.preventDefault();
      fn(e.key === "ArrowLeft" || e.key === "ArrowUp" ? -10 : 10, false, null);
      registry.persistLayout();
    }
  };
}
async function init() {
  if (!shared.token) {
    registry.toast(t("No session connection. Start the app with start.command."));
    return;
  }
  setInterval(registry.poll, 400);
  await registry.refreshFiles();
  shared.commands = await registry.api("history");
  await registry.refreshHistory?.();
  shared.cmdIndex = shared.commands.length;
  registry.renderHistory();
  let restored = false, migrated = registry.migrateLegacyDrafts();
  try {
    let saved = registry.draftStore().scopes[shared.draftScope];
    if (saved?.tabs?.length) {
      for (let t of saved.tabs) {
        if (!registry.within(t.path, shared.draftScope)) continue;
        if (t.dirty) {
          shared.tabs.push(t);
          restored = true;
        } else {
          try {
            let d = await registry.api("file?path=" + encodeURIComponent(t.path));
            shared.tabs.push({ ...d, saved: d.content, dirty: false });
          } catch {
          }
        }
      }
      if (shared.tabs.length) registry.switchTab(shared.tabs.find((t) => t.path === saved.active) || shared.tabs[0]);
    }
  } catch {
  }
  if (!shared.tabs.length) {
    try {
      await registry.openFile(registry.joinPath(shared.defaultWorkspace, "examples/hosgeldin.m"));
    } catch {
      registry.createUntitled();
    }
  }
  if (migrated) registry.toast(t("Older drafts were linked to the starting folder; the previous save was kept."));
  else if (restored) registry.toast(t("Unsaved drafts were restored."));
  await registry.poll();
}

registry.bootstrap = () => {
  registry.setupEditorCommands();
  registry.on("#run", registry.runFile);
  registry.on("#run-section", registry.runSection);
  registry.on("#run-selection", registry.runSelection);
  registry.on("#run-profile", registry.runProfile);
  registry.on("#publish", registry.publishFile);
  registry.on("#stop", () => registry.api("stop", {}));
  registry.on("#refresh-files", () => registry.refreshFiles(false));
  for (let button of document.querySelectorAll("#debug-controls button[data-debug]")) button.onclick = () => registry.safe(() => registry.debugCommand(button.dataset.debug));
  registry.on("#run-to-cursor", registry.runToCursor);
  registry.on("#clear-breakpoints", registry.clearBreakpoints);
  registry.$("#folder-form").onsubmit = (e) => {
    e.preventDefault();
    registry.safe(() => registry.navigateFolder(registry.$("#folder-address").value.trim()));
  };
  registry.on("#folder-up", () => {
    if (shared.currentFolder === shared.folderRoot) return;
    return registry.navigateFolder(shared.currentFolder.slice(0, shared.currentFolder.lastIndexOf("/")));
  });
  registry.on("#folder-back", async () => {
    if (shared.folderIndex <= 0) return;
    let index = shared.folderIndex - 1;
    await registry.navigateFolder(shared.folderHistory[index], false);
    shared.folderIndex = index;
    registry.renderFolder();
  });
  registry.on("#folder-forward", async () => {
    if (shared.folderIndex >= shared.folderHistory.length - 1) return;
    let index = shared.folderIndex + 1;
    await registry.navigateFolder(shared.folderHistory[index], false);
    shared.folderIndex = index;
    registry.renderFolder();
  });
  registry.on("#clear-console", () => {
    registry.$("#console").replaceChildren();
    shared.activeOutput = null;
  });
  registry.on("#reset", async () => {
    if (!confirm(t("Reset the Octave session? Variables and figures in memory will be cleared; your files will be kept."))) return;
    shared.uiGeneration++;
    shared.publishRenders.clear();
    await registry.api("reset", {});
    shared.lastJob = null;
    shared.lastFiguresKey = "";
    registry.poll.completed = null;
    shared.activeOutput = null;
    registry.toast(t("Starting a new Octave session."));
  });
  registry.on("#clear-workspace", registry.clearWorkspace);
  registry.on("#send-input", async () => {
    await registry.api("input", { text: registry.$("#runtime-input").value });
    registry.$("#runtime-input").value = "";
  });
  registry.$("#runtime-input").onkeydown = (e) => {
    if (e.key === "Enter") registry.$("#send-input").click();
  };
  registry.on("#plot-download", () => {
    if (shared.figures.length) registry.download(shared.figureURLs[shared.figureIndex], shared.figures[shared.figureIndex].name + ".png");
  });
  registry.on("#plot-expand", () => registry.expandFigure());
  registry.on("#plot-interactive", () => {
    shared.plotMode = "interactive";
    registry.renderFigures();
  });
  registry.on("#plot-png", () => {
    shared.plotMode = "png";
    registry.renderFigures();
  });
  registry.on("#plot-fit", () => registry.resetFigure());
  registry.on("#rotate-left", () => registry.rotate(-15));
  registry.on("#rotate-right", () => registry.rotate(15));
  registry.on("#toggle-files", () => {
    if (!shared.settings.panels.files) registry.setLayoutPanel("left", true);
    registry.$("#left-panel").classList.toggle("visible");
  });
  registry.on("#toggle-workspace", () => {
    if (!layoutUtils.panelVisible(shared.settings.panels, "right")) registry.setLayoutPanel("right", true);
    registry.$("#right-panel").classList.toggle("visible");
  });
  window.addEventListener("beforeunload", (e) => {
    if (shared.tabs.some((t) => t.dirty)) {
      e.preventDefault();
      e.returnValue = "";
    }
  });
  divider("#left-divider", "x", (delta, drag, e) => {
    const size = drag ? e.clientX - parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--gap")) : registry.$("#left-panel").clientWidth + delta;
    document.documentElement.style.setProperty("--left", Math.max(140, Math.min(360, size)) + "px");
    return size;
  }, { area: "left", snap: true });
  let rightEdge = 0;
  divider("#right-divider", "x", (delta, drag, e) => {
    const size = drag ? rightEdge - e.clientX : registry.$("#right-panel").clientWidth - delta;
    document.documentElement.style.setProperty("--right", Math.max(190, Math.min(500, size)) + "px");
    return size;
  }, { area: "right", snap: true, begin: () => { rightEdge = registry.$("#right-panel").getBoundingClientRect().right; } });
  divider("#editor-divider", "y", (delta, drag, e) => {
    let r = registry.$(".center").getBoundingClientRect(), v = drag ? (e.clientY - r.top) / r.height * 100 : registry.$(".editor-panel").clientHeight / r.height * 100 + delta / 5;
    document.documentElement.style.setProperty("--editor-height", Math.max(20, Math.min(78, v)) + "%");
    return drag ? r.bottom - e.clientY - parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--gap")) : Infinity;
  }, { area: "bottom", snap: true });
  divider("#plot-divider", "x", (delta, drag, e) => {
    let r = registry.$(".bottom-panels").getBoundingClientRect(), width = drag ? e.clientX - r.left : registry.$(".console-panel").clientWidth + delta;
    registry.$(".console-panel").style.flex = `0 0 ${Math.max(20, Math.min(80, width / r.width * 100))}%`;
  }, { area: "plot" });

  registry.safe(init);
};
Object.assign(registry, { divider, init });
registry.registerShortcut({ defaultMatch: { keys: ["s"], lowercase: true, modifiers: "primary" }, id: "global.save", label: "Save the active file", scope: "global", scopeLabel: "General", bindings: ["Mod+KeyS"], command: () => registry.safe(registry.saveActive) });
registry.registerShortcut({ defaultMatch: { keys: ["p"], lowercase: true, modifiers: "primary" }, id: "global.quick-open", label: "Go to File", scope: "global", scopeLabel: "General", bindings: ["Mod+KeyP"], command: () => registry.quickOpen() });
registry.registerShortcut({ defaultMatch: { keys: ["+", "="], modifiers: "primary" }, id: "global.font-increase", label: "Increase editor font size", scope: "global", scopeLabel: "General", bindings: ["Mod+Equal"], ignoreShift: true, command: () => registry.changeEditorFont(1) });
registry.registerShortcut({ defaultMatch: { keys: ["-"], modifiers: "primary" }, id: "global.font-decrease", label: "Decrease editor font size", scope: "global", scopeLabel: "General", bindings: ["Mod+Minus"], ignoreShift: true, command: () => registry.changeEditorFont(-1) });
registry.registerShortcut({ defaultMatch: { keys: ["0"], modifiers: "primary" }, id: "global.font-reset", label: "Reset editor font size", scope: "global", scopeLabel: "General", bindings: ["Mod+Digit0"], command: () => registry.resetEditorFont() });
registry.registerShortcut({ defaultMatch: { keys: ["F1"], modifiers: "any" }, id: "global.documentation", label: "Open function help", scope: "global", scopeLabel: "General", bindings: ["F1"], command: () => registry.safe(() => registry.showDocumentation(registry.wordUnderCursor())) });
registry.registerShortcut({ id: "global.run", label: "Run File", scope: "global", scopeLabel: "General", bindings: ["F5"], command: () => registry.safe(registry.runFile) });
registry.registerShortcut({ id: "global.run-selection", label: "Run Selected Code", scope: "global", scopeLabel: "General", bindings: ["F9"], command: () => registry.safe(registry.runSelection) });
registry.registerShortcut({ defaultMatch: { keys: ["c"], lowercase: true, modifiers: "control" }, id: "global.stop", label: "Stop the running computation", scope: "global", scopeLabel: "General", bindings: ["Ctrl+KeyC"], configurable: false, when: (event) => {
  if (!shared.busy) return false;
  const command = event.target === registry.$("#command"), editing = event.target.closest?.("#editor");
  const selected = command ? event.target.selectionStart !== event.target.selectionEnd : editing ? shared.editor.state.selection.ranges.some((range) => !range.empty) : true;
  return (command || editing) && !selected;
}, command: () => registry.safe(() => registry.api("stop", {})) });
