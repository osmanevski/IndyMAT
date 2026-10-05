import shared from "./state.js";
import registry from "./registry.js";
import "./editor_commands.js";
import { t } from "./i18n.js";

function divider(id, axis, fn) {
  const n = registry.$(id);
  n.onpointerdown = (e) => {
    n.setPointerCapture(e.pointerId);
    n.classList.add("dragging");
    const start = axis === "x" ? e.clientX : e.clientY;
    n.onpointermove = (ev) => fn((axis === "x" ? ev.clientX : ev.clientY) - start, true, ev);
    n.onpointerup = () => {
      n.onpointermove = null;
      n.classList.remove("dragging");
      registry.persistLayout();
    };
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
  registry.on("#plot-expand", () => {
    if (!shared.figures.length) return;
    let img = registry.el("img");
    img.src = shared.figureURLs[shared.figureIndex];
    img.alt = shared.figures[shared.figureIndex].name;
    registry.modal(shared.figures[shared.figureIndex].name, img);
  });
  registry.on("#plot-interactive", () => {
    shared.plotMode = "interactive";
    registry.renderFigures();
  });
  registry.on("#plot-png", () => {
    shared.plotMode = "png";
    registry.renderFigures();
  });
  registry.on("#plot-fit", () => {
    if (shared.interactivePlot) shared.interactivePlot.reset();
    else registry.$("#plot-area").classList.remove("zoomed");
  });
  registry.on("#rotate-left", () => registry.rotate(-15));
  registry.on("#rotate-right", () => registry.rotate(15));
  registry.on("#toggle-files", () => registry.$("#left-panel").classList.toggle("visible"));
  registry.on("#toggle-workspace", () => registry.$("#right-panel").classList.toggle("visible"));
  window.addEventListener("beforeunload", (e) => {
    if (shared.tabs.some((t) => t.dirty)) {
      e.preventDefault();
      e.returnValue = "";
    }
  });
  divider("#left-divider", "x", (delta, drag, e) => document.documentElement.style.setProperty("--left", Math.max(140, Math.min(360, drag ? e.clientX - 6 : registry.$("#left-panel").clientWidth + delta)) + "px"));
  divider("#right-divider", "x", (delta, drag, e) => document.documentElement.style.setProperty("--right", Math.max(190, Math.min(500, drag ? innerWidth - e.clientX - 6 : registry.$("#right-panel").clientWidth - delta)) + "px"));
  divider("#editor-divider", "y", (delta, drag, e) => {
    let r = registry.$(".center").getBoundingClientRect(), v = drag ? (e.clientY - r.top) / r.height * 100 : registry.$(".editor-panel").clientHeight / r.height * 100 + delta / 5;
    document.documentElement.style.setProperty("--editor-height", Math.max(20, Math.min(78, v)) + "%");
  });
  divider("#plot-divider", "x", (delta, drag, e) => {
    let r = registry.$(".bottom-panels").getBoundingClientRect(), width = drag ? e.clientX - r.left : registry.$(".console-panel").clientWidth + delta;
    registry.$(".console-panel").style.flex = `0 0 ${Math.max(20, Math.min(80, width / r.width * 100))}%`;
  });

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
