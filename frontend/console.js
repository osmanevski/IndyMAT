import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";
import editorCommandUtils from "./editor_command_utils.cjs";
import { editorSourceContext, showAdaptation } from "./source_adapter.js";

const { sectionRange } = editorCommandUtils;
let lastStatus = null;

function addConsole(label, source = "") {
  let wrap = registry.el("div", "console-entry"), command = registry.el("div", "console-command", ">> " + label);
  if (source) command.title = source;
  wrap.append(command);
  let out = registry.el("pre", "console-output");
  let err = registry.el("pre", "console-output console-error");
  let time = registry.el("div", "console-time");
  wrap.append(out, err, time);
  registry.$("#console").append(wrap);
  while (registry.$("#console").children.length > 100) registry.$("#console").firstElementChild.remove();
  shared.activeOutput = { wrap, out, err, time };
  scrollConsole();
  return shared.activeOutput;
}
function applyConsoleClear(s) {
  if (!shared.activeOutput || !s.console_clear || shared.activeOutput.clearCount === s.console_clear) return;
  shared.activeOutput.clearCount = s.console_clear;
  shared.activeOutput.wrap.querySelector(".console-command")?.remove();
  registry.$("#console").replaceChildren(shared.activeOutput.wrap);
}
function scrollConsole() {
  let c = registry.$("#console");
  c.scrollTop = c.scrollHeight;
}
async function execute(code, mode = "code", argument = "", label, recordHistory = false, source = "", onAccepted, sourceContext = null) {
  registry.requireIdle();
  const generation = shared.uiGeneration;
  const payload = { code, mode, argument, history: recordHistory };
  if (sourceContext) Object.assign(payload, { adapt_editor_literals: true, source_context: sourceContext });
  let result = await registry.api("execute", payload);
  if (generation !== shared.uiGeneration) return;
  shared.publishRenders.clear();
  shared.lastJob = result.job;
  const entry = addConsole(label || code || argument, source);
  entry.job = result.job;
  entry.epoch = result.source_adapter?.epoch ?? shared.engine.epoch;
  entry.wrap.dataset.job = result.job;
  entry.wrap.dataset.epoch = String(entry.epoch);
  entry.wrap.dataset.cwd = shared.engine.cwd || shared.currentFolder || "";
  if (sourceContext) {
    entry.sourceContext = sourceContext;
    entry.wrap.sourceEntry = entry;
    const original = registry.el("pre", "console-source", code);
    entry.wrap.querySelector(".console-command").after(original);
    showAdaptation(entry, result.source_adapter);
  }
  shared.busy = true;
  setStatus({ status: "running", elapsed: 0 });
  if (onAccepted) onAccepted(result);
  if (recordHistory && code.trim() && result.history_recorded !== false) {
    if (shared.commands.at(-1) !== code) shared.commands.push(code);
    shared.cmdIndex = shared.commands.length;
    renderHistory();
  }
  await registry.poll();
  return result;
}
async function runFile() {
  registry.requireIdle();
  let t = shared.active;
  if (!t) return;
  if (t.dirty || !t.hash) {
    if (!await registry.saveActive()) return;
  }
  await execute("", "file", t.path, "run " + t.path.split("/").pop());
}
async function runFileMode(mode, label) {
  registry.requireIdle();
  let t = shared.active;
  if (!t) return;
  if (t.dirty || !t.hash) {
    if (!await registry.saveActive()) return;
  }
  const generation = shared.uiGeneration;
  let result = await registry.api(mode, { path: t.path });
  if (generation !== shared.uiGeneration) return;
  shared.publishRenders.clear();
  shared.lastJob = result.job;
  if (mode === "publish" && result.render) shared.publishRenders.set(result.job, result.render);
  addConsole(label + " " + t.path.split("/").pop());
  shared.busy = true;
  setStatus({ status: "running", elapsed: 0 });
  for (let menu of document.querySelectorAll(".run-more[open]")) menu.open = false;
  await registry.poll();
}
function runProfile() {
  return runFileMode("profile", t("Profile"));
}
function publishFile() {
  return runFileMode("publish", t("Publish"));
}
async function runSelection() {
  registry.requireIdle();
  let range = shared.editor.state.selection.main, code = shared.editor.state.sliceDoc(range.from, range.to);
  if (!code.trim()) throw new Error(t("Select code to run."));
  let count = code.replace(/\r?\n$/, "").split(/\r?\n/).length;
  const context = editorSourceContext("editor-selection", shared.editor.state.doc.toString(), range.from, range.to, range.head);
  await execute(code, "code", "", t("Selection · {count} lines", { count }), false, code, null, context);
}
async function runEditorSection(kind = "section") {
  registry.requireIdle();
  const profile = registry.getSetting("preferences", "adaptEditorLiterals") ? "matlab" : "native-octave";
  let tab = shared.active, source = shared.editor.state.doc.toString(), position = shared.editor.state.selection.main.head, throughEnd = kind === "to-end", range = sectionRange(source, position, throughEnd, profile);
  let code = source.slice(range.from, range.to), name = (tab?.path || "").split("/").pop(), label = kind === "to-end" ? t("To end · {name}:{start}–{end}", { name, start: range.startLine, end: range.endLine }) : t("Section · {name}:{start}–{end}", { name, start: range.startLine, end: range.endLine });
  let advance = kind === "advance" && range.nextFrom !== null ? () => {
    if (shared.active !== tab || shared.editor.state.doc.toString() !== source) return;
    shared.editor.dispatch({ selection: { anchor: range.nextFrom }, scrollIntoView: true });
    shared.editor.focus();
  } : null;
  const origin = kind === "to-end" ? "editor-to-end" : kind === "advance" ? "editor-advance" : "editor-section";
  const context = editorSourceContext(origin, source, range.from, range.to, position);
  await execute(code, "code", "", label, false, code, advance, context);
}
function runSection() {
  return runEditorSection("section");
}
function runAndAdvance() {
  return runEditorSection("advance");
}
function runToEnd() {
  return runEditorSection("to-end");
}
function renderHistory() {
  let list = registry.$("#history");
  list.replaceChildren();
  shared.commands.slice(-100).reverse().forEach((command) => {
    let b = registry.el("button", "history-item", command);
    b.title = t("Insert into Command Window: {command}", { command });
    b.onclick = () => {
      registry.$("#command").value = command;
      registry.$("#command").focus();
    };
    list.append(b);
  });
  registry.$("#history-count").textContent = shared.commands.length;
}
function setStatus(s) {
  let running = ["running", "stopping"].includes(s.status), paused = s.status === "paused";
  lastStatus = s;
  shared.busy = running;
  shared.starting = s.status === "starting";
  document.body.classList.toggle("busy", running || shared.starting);
  document.body.classList.toggle("debug-paused", paused);
  registry.$("#status-text").textContent = { idle: t("Ready"), running: t("Running"), paused: t("Paused at breakpoint"), stopping: t("Stopping"), starting: t("Starting engine"), dead: t("Session closed") }[s.status] || t("Disconnected");
  registry.$("#elapsed").textContent = s.elapsed ? `${s.elapsed.toFixed(2)} s` : "";
  registry.$("#stop").disabled = !running && !paused;
  for (let selector of ["#run", "#run-section", "#run-selection", "#run-profile", "#publish", "#editor-run-section", "#editor-run-advance", "#editor-run-to-end", "#editor-run-selection"]) registry.$(selector).disabled = s.status !== "idle";
  registry.$("#command").disabled = shared.starting || s.status === "dead";
  registry.$("#debug-controls").hidden = !paused;
  for (let button of document.querySelectorAll("#debug-controls button")) button.disabled = !s.debug?.ready;
  registry.$("#input-bar").hidden = !s.waiting_input;
  registry.$("#status-dot").style.background = s.status === "dead" ? "var(--danger)" : "";
  registry.updateWorkspaceActions?.(s.status);
}

registry.setupCommandWindow = () => {
  shared.commandCompletion = null;
  registry.$("#command-form").onsubmit = (e) => {
    e.preventDefault();
    registry.safe(async () => {
      let command = registry.$("#command").value.trim();
      if (!command) return;
      let doc = command.match(/^doc\s+([A-Za-z][A-Za-z0-9_]*)$/i);
      if (doc) {
        await registry.showDocumentation(doc[1]);
        registry.$("#command").value = "";
        return;
      }
      if (shared.engine.status === "paused") {
        await registry.debugCommand("eval", command);
      } else await execute(command, "code", "", "", true);
      registry.$("#command").value = "";
    });
  };

  registry.$("#command").oninput = () => shared.commandCompletion = null;
  registry.$("#command").onkeydown = (e) => {
    if (e.key === "Tab") {
      e.preventDefault();
      let input = e.target, end = input.selectionStart, start = end, prefix;
      if (shared.commandCompletion && end === shared.commandCompletion.end) {
        start = shared.commandCompletion.start;
        prefix = shared.commandCompletion.prefix;
      } else {
        let match = input.value.slice(0, end).match(/[A-Za-z_][A-Za-z_0-9]*$/);
        if (!match) return;
        start = end - match[0].length;
        prefix = match[0];
        let matches = [.../* @__PURE__ */ new Set([...registry.words, ...shared.variables.map((v) => v.name)])].filter((v) => v.toLowerCase().startsWith(prefix.toLowerCase())).sort();
        if (!matches.length) return;
        shared.commandCompletion = { start, prefix, matches, index: -1, end };
      }
      shared.commandCompletion.index = (shared.commandCompletion.index + 1) % shared.commandCompletion.matches.length;
      let value = shared.commandCompletion.matches[shared.commandCompletion.index];
      input.setRangeText(value, start, end, "end");
      shared.commandCompletion.end = start + value.length;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      shared.commandCompletion = null;
      shared.cmdIndex = Math.max(0, shared.cmdIndex - 1);
      e.target.value = shared.commands[shared.cmdIndex] || "";
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      shared.commandCompletion = null;
      shared.cmdIndex = Math.min(shared.commands.length, shared.cmdIndex + 1);
      e.target.value = shared.commands[shared.cmdIndex] || "";
    }
  };

};
Object.assign(registry, { addConsole, applyConsoleClear, scrollConsole, execute, runFile, runFileMode, runProfile, publishFile, runSelection, runSection, runAndAdvance, runToEnd, renderHistory, setStatus });

onLanguageChange(() => {
  if (lastStatus) setStatus(lastStatus);
  renderHistory();
});
