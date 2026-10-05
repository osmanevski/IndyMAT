import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";
import commandWindowUtils from "./command_window_utils.cjs";

const { commandIsIncomplete, parseErrorLocations, decorationSegments } = commandWindowUtils;
const CONSOLE_BYTES = 1_000_000;
const MATCH_LIMIT = 1000;
const LOCATION_LIMIT = 64;
const outputText = new WeakMap();
const outputLocations = new WeakMap();
const renderedOutput = new WeakMap();
const searchState = { matches: [], byNode: new Map(), index: -1, rendering: false, capped: false };
const locationCache = new Map();
const locationQueue = [];
let activeResolutions = 0;
let renderTimer;
let outputObserver;
let knownOutputs = new Set();

function resizeCommand() {
  let input = registry.$("#command");
  input.style.height = "auto";
  input.style.height = Math.min(input.scrollHeight, 112) + "px";
  registry.$("#command-form").classList.toggle("multiline", input.value.includes("\n") || input.scrollHeight > 38);
}

function replaceCommand(value) {
  let input = registry.$("#command");
  input.value = value;
  input.selectionStart = input.selectionEnd = value.length;
  shared.commandCompletion = null;
  resizeCommand();
}

async function submitCommand() {
  let input = registry.$("#command"), command = input.value.trim();
  if (!command) return;
  if (shared.engine.waiting_input) throw new Error(t("Send program input using the field below."));
  let doc = command.match(/^doc\s+([A-Za-z][A-Za-z0-9_]*)$/i);
  if (doc && shared.engine.status !== "paused") {
    await registry.showDocumentation(doc[1]);
    replaceCommand("");
    return;
  }
  if (shared.engine.status === "paused") await registry.debugCommand("eval", command);
  else {
    const result = await registry.execute(command, "code", "", "", true);
    if (result?.history_recorded === false) registry.toast(t("Command ran but was not added to history because it exceeded the 64 KB limit."));
  }
  replaceCommand("");
  shared.commandRecall = null;
  await registry.refreshHistory?.();
}

function completeCommand(event) {
  event.preventDefault();
  shared.commandRecall = null;
  let input = event.target, end = input.selectionStart, start = end, prefix;
  if (shared.commandCompletion && end === shared.commandCompletion.end) {
    start = shared.commandCompletion.start;
    prefix = shared.commandCompletion.prefix;
  } else {
    let match = input.value.slice(0, end).match(/[A-Za-z_][A-Za-z_0-9]*$/);
    if (!match) return;
    start = end - match[0].length;
    prefix = match[0];
    let matches = [...new Set([...registry.words, ...shared.variables.map((variable) => variable.name)])].filter((value) => value.toLowerCase().startsWith(prefix.toLowerCase())).sort();
    if (!matches.length) return;
    shared.commandCompletion = { start, prefix, matches, index: -1, end };
  }
  shared.commandCompletion.index = (shared.commandCompletion.index + 1) % shared.commandCompletion.matches.length;
  let value = shared.commandCompletion.matches[shared.commandCompletion.index];
  input.setRangeText(value, start, end, "end");
  shared.commandCompletion.end = start + value.length;
  resizeCommand();
}

function onFirstLine(input) {
  return input.selectionStart === input.selectionEnd && input.value.lastIndexOf("\n", input.selectionStart - 1) < 0;
}

function onLastLine(input) {
  return input.selectionStart === input.selectionEnd && input.value.indexOf("\n", input.selectionEnd) < 0;
}

function recallHistory(input, direction) {
  let recall = shared.commandRecall;
  if (!recall) {
    if (direction > 0) return;
    let draft = input.value;
    recall = { draft, prefix: draft, matches: shared.commands.filter((command) => command.startsWith(draft)), index: 0 };
    if (!recall.matches.length) return;
    shared.commandRecall = recall;
  }
  if (direction < 0) recall.index = Math.min(recall.matches.length, recall.index + 1);
  else recall.index = Math.max(0, recall.index - 1);
  replaceCommand(recall.index === 0 ? recall.draft : recall.matches[recall.matches.length - recall.index]);
}

function insertNewline(input) {
  input.setRangeText("\n", input.selectionStart, input.selectionEnd, "end");
  shared.commandCompletion = null;
  shared.commandRecall = null;
  resizeCommand();
}

function enterCommand(input, forceNewline = false) {
  if (shared.engine.status === "paused" || shared.engine.waiting_input || shared.engine.status !== "idle") {
    registry.$("#command-form").requestSubmit();
    return;
  }
  if (forceNewline || commandIsIncomplete(input.value)) {
    insertNewline(input);
    return;
  }
  registry.$("#command-form").requestSubmit();
}

function commandKeydown(event) {
  if (event.defaultPrevented || event.isComposing) return;
  let input = event.target;
  if (event.key === "Tab") return completeCommand(event);
  if (event.key === "ArrowUp" && onFirstLine(input)) {
    event.preventDefault();
    shared.commandCompletion = null;
    recallHistory(input, -1);
    return;
  }
  if (event.key === "ArrowDown" && onLastLine(input) && shared.commandRecall) {
    event.preventDefault();
    shared.commandCompletion = null;
    recallHistory(input, 1);
    return;
  }
  if (event.key !== "Enter" || event.isComposing) return;
  event.preventDefault();
  enterCommand(input, event.shiftKey);
}

// Rebindings choose the existing command, not a second implementation of its
// editing/engine-state rules. Escape and ordinary caret keys remain native.
function commandShortcut(event, key, shiftKey = event.shiftKey) {
  let handled = false;
  commandKeydown({
    key,
    shiftKey,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    altKey: event.altKey,
    repeat: event.repeat,
    target: event.target,
    isComposing: event.isComposing,
    preventDefault() {
      handled = true;
      event.preventDefault();
    },
    stopPropagation() {
      event.stopPropagation();
    }
  });
  return handled;
}

async function openErrorLocation(path, line) {
  let location = await registry.api("error-location", { path, line });
  await registry.openFile(location.path);
  registry.revealLine(location.line);
}

function outputNodes() {
  return [...document.querySelectorAll("#console .console-output")];
}

function enforceConsoleBudget() {
  const root = registry.$("#console");
  if (!root) return;
  const encoder = new TextEncoder();
  const entries = [...root.children];
  const sizes = entries.map((entry) => encoder.encode(entry.textContent).length);
  let total = sizes.reduce((sum, size) => sum + size, 0);
  while (total > CONSOLE_BYTES && entries.length > 1) {
    total -= sizes.shift();
    entries.shift().remove();
  }
  if (total > CONSOLE_BYTES && entries.length) {
    let remaining = CONSOLE_BYTES;
    const fields = [...entries[0].querySelectorAll(".console-command,.console-output,.console-time")];
    for (const field of fields.reverse()) {
      const bytes = encoder.encode(field.textContent);
      if (bytes.length > remaining) {
        const marker = remaining > 40 ? t("[Earlier output truncated]") + "\n" : "";
        let start = bytes.length - Math.max(0, remaining - encoder.encode(marker).length);
        while (start < bytes.length && (bytes[start] & 192) === 128) start++;
        field.textContent = marker + new TextDecoder().decode(bytes.subarray(start));
        outputText.set(field, field.textContent);
        renderedOutput.delete(field);
      }
      remaining -= encoder.encode(field.textContent).length;
    }
  }
  // Source tooltips can otherwise retain multi-megabyte submitted commands.
  for (const entry of entries) {
    const command = entry.querySelector?.(".console-command");
    if (command?.title?.length > 16000) command.removeAttribute("title");
  }
}

function collectSearchMatches(query) {
  searchState.matches = [];
  searchState.byNode = new Map();
  searchState.capped = false;
  if (!query) return;
  let needle = query.toLocaleLowerCase("tr");
  for (let node of outputNodes()) {
    let raw = outputText.get(node) ?? node.textContent;
    let haystack = raw.toLocaleLowerCase("tr"), from = 0;
    const matches = [];
    searchState.byNode.set(node, matches);
    while ((from = haystack.indexOf(needle, from)) >= 0) {
      if (searchState.matches.length === MATCH_LIMIT) {
        searchState.capped = true;
        return;
      }
      const match = { node, start: from, end: from + query.length };
      searchState.matches.push(match);
      matches.push(match);
      from += Math.max(1, query.length);
    }
  }
}

function scheduleOutputSearch(delay = 60) {
  clearTimeout(renderTimer);
  renderTimer = setTimeout(renderOutputSearch, delay);
}

function pumpLocations() {
  while (activeResolutions < 4 && locationQueue.length) {
    const task = locationQueue.shift();
    if (!task.current()) {
      locationCache.delete(task.key);
      task.resolve(null);
      continue;
    }
    activeResolutions++;
    registry.api("error-location", task.payload).then(task.resolve, () => task.resolve(null)).finally(() => {
      activeResolutions--;
      setTimeout(pumpLocations, 25);
    });
  }
}

function resolveLocation(candidate, current) {
  const key = `${shared.engine?.cwd || shared.currentFolder || ""}\0${candidate.path}\0${candidate.line}`;
  if (locationCache.has(key)) return locationCache.get(key);
  if (locationQueue.length >= LOCATION_LIMIT) return Promise.resolve(null);
  const promise = new Promise((resolve) => locationQueue.push({ key, resolve, current, payload: { path: candidate.path, line: candidate.line } }));
  locationCache.set(key, promise);
  while (locationCache.size > 128) locationCache.delete(locationCache.keys().next().value);
  pumpLocations();
  return promise;
}

function resolvedOutputLocations(node, raw, limit) {
  if (!limit) return [];
  let cached = outputLocations.get(node);
  if (cached?.raw === raw) return cached.locations.slice(0, limit);
  cached = { raw, locations: [] };
  outputLocations.set(node, cached);
  const candidates = parseErrorLocations(raw, limit);
  cached.candidateCount = candidates.length;
  if (candidates.length) {
    // Both stdout/stderr and caught errors can carry locations. Resolve once
    // per text revision, and retain absolute paths for later clicks/cwd changes.
    Promise.all(candidates.map(async (candidate) => {
      try {
        let resolved = await resolveLocation(candidate, () => node.isConnected && outputLocations.get(node) === cached);
        if (!resolved) return null;
        return { ...candidate, path: resolved.path, line: resolved.line };
      } catch {
        return null;
      }
    })).then((locations) => {
      if (!node.isConnected || outputLocations.get(node) !== cached || node.textContent !== raw) return;
      cached.locations = locations.filter(Boolean);
      scheduleOutputSearch();
    });
  }
  return cached.locations;
}

function renderOutput(node, query, locations) {
  let raw = outputText.get(node) ?? node.textContent;
  outputText.set(node, raw);
  let matches = searchState.byNode.get(node) || [];
  const active = searchState.matches[searchState.index];
  const key = `${query}\0${matches.length}\0${active?.node === node ? active.start : -1}\0${locations.map((item) => `${item.start}:${item.path}:${item.line}`).join("\0")}`;
  const previous = renderedOutput.get(node);
  if (previous?.raw === raw && previous.key === key) return;
  let fragment = document.createDocumentFragment();
  for (const { start, end, match, location } of decorationSegments(raw.length, matches, locations)) {
    let text = document.createTextNode(raw.slice(start, end));
    let child = text;
    if (match) {
      let mark = registry.el("mark", "console-match" + (searchState.matches[searchState.index] === match ? " current" : ""));
      mark.append(child);
      child = mark;
    }
    if (location) {
      let button = registry.el("button", "console-location");
      button.type = "button";
      button.dataset.path = location.path;
      button.dataset.line = String(location.line);
      button.title = t("Open {path} at line {line}", { path: location.path, line: location.line });
      button.onclick = () => registry.safe(() => openErrorLocation(location.path, location.line));
      button.append(child);
      child = button;
    }
    fragment.append(child);
  }
  searchState.rendering = true;
  node.replaceChildren(fragment);
  renderedOutput.set(node, { raw, key });
  searchState.rendering = false;
}

function renderOutputSearch() {
  clearTimeout(renderTimer);
  outputObserver?.disconnect();
  enforceConsoleBudget();
  let query = registry.$("#console-search").value;
  collectSearchMatches(query);
  if (!searchState.matches.length) searchState.index = -1;
  else searchState.index = Math.max(0, Math.min(searchState.index, searchState.matches.length - 1));
  let remaining = LOCATION_LIMIT;
  const locations = new Map();
  const nodes = outputNodes();
  knownOutputs = new Set(nodes);
  for (const node of [...nodes].reverse()) {
    const raw = outputText.get(node) ?? node.textContent;
    locations.set(node, resolvedOutputLocations(node, raw, remaining));
    const cached = outputLocations.get(node);
    remaining -= Math.min(remaining, cached?.candidateCount || 0);
  }
  for (let node of nodes) renderOutput(node, query, locations.get(node));
  registry.$("#console-match-count").textContent = searchState.matches.length ? `${searchState.index + 1} / ${searchState.matches.length}${searchState.capped ? "+" : ""}` : "0";
  registry.$("#console-match-count").title = searchState.capped ? t("Showing the first 1000 matches. Narrow your search to see fewer.") : "";
  registry.$("#console-search-previous").disabled = !searchState.matches.length;
  registry.$("#console-search-next").disabled = !searchState.matches.length;
  outputObserver?.observe(registry.$("#console"), { childList: true, subtree: true, characterData: true });
}

function moveOutputMatch(delta) {
  if (!searchState.matches.length) return;
  searchState.index = (searchState.index + delta + searchState.matches.length) % searchState.matches.length;
  renderOutputSearch();
  searchState.matches[searchState.index]?.node.scrollIntoView({ block: "center" });
}

function observeConsoleOutput() {
  let observer = new MutationObserver(() => {
    if (searchState.rendering) return;
    let nodes = outputNodes(), changed = nodes.length !== knownOutputs.size || nodes.some((node) => !knownOutputs.has(node));
    knownOutputs = new Set(nodes);
    for (let node of nodes) {
      if (!outputText.has(node) || node.textContent !== outputText.get(node)) {
        outputText.set(node, node.textContent);
        changed = true;
      }
    }
    if (changed) scheduleOutputSearch();
  });
  observer.observe(registry.$("#console"), { childList: true, subtree: true, characterData: true });
  outputObserver = observer;
}

registry.setupCommandWindow = () => {
  shared.commandCompletion = null;
  shared.commandRecall = null;
  let form = registry.$("#command-form"), input = registry.$("#command");
  form.onsubmit = (event) => {
    event.preventDefault();
    registry.safe(submitCommand);
  };
  input.oninput = () => {
    shared.commandCompletion = null;
    shared.commandRecall = null;
    resizeCommand();
  };
  input.onkeydown = registry.registerShortcut ? null : commandKeydown;
  registry.$("#console-search").oninput = () => {
    searchState.index = 0;
    scheduleOutputSearch(100);
  };
  registry.$("#console-search").onkeydown = (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      moveOutputMatch(event.shiftKey ? -1 : 1);
    }
  };
  registry.on("#console-search-previous", () => moveOutputMatch(-1));
  registry.on("#console-search-next", () => moveOutputMatch(1));
  observeConsoleOutput();
  resizeCommand();
};

Object.assign(registry, { openErrorLocation, renderOutputSearch, resizeCommand, submitCommand });
registry.registerShortcut?.({ defaultMatch: { keys: ["Tab"], modifiers: "any" }, id: "command.complete", label: t("Complete Command"), scope: "command-line", scopeLabel: t("Command Line"), bindings: ["Tab"], command: (event) => commandShortcut(event, "Tab") });
registry.registerShortcut?.({ defaultMatch: { keys: ["ArrowUp"], modifiers: "any" }, id: "command.previous", label: t("Recall Previous Command"), scope: "command-line", scopeLabel: t("Command Line"), bindings: ["ArrowUp"], when: (event) => onFirstLine(event.target), command: (event) => commandShortcut(event, "ArrowUp") });
registry.registerShortcut?.({ defaultMatch: { keys: ["ArrowDown"], modifiers: "any" }, id: "command.next", label: t("Recall Next Command"), scope: "command-line", scopeLabel: t("Command Line"), bindings: ["ArrowDown"], when: (event) => onLastLine(event.target) && !!shared.commandRecall, command: (event) => commandShortcut(event, "ArrowDown") });
registry.registerShortcut?.({ defaultMatch: { keys: ["Enter"], modifiers: "no-shift" }, id: "command.submit", label: t("Run Command"), scope: "command-line", scopeLabel: t("Command Line"), bindings: ["Enter"], command: (event) => commandShortcut(event, "Enter", false) });
registry.registerShortcut?.({ defaultMatch: { keys: ["Enter"], modifiers: "shift" }, id: "command.newline", label: t("Insert New Line in Command"), scope: "command-line", scopeLabel: t("Command Line"), bindings: ["Shift+Enter"], command: (event) => commandShortcut(event, "Enter", true) });

onLanguageChange(() => {
  const labels = {
    "command.complete": "Complete Command",
    "command.previous": "Recall Previous Command",
    "command.next": "Recall Next Command",
    "command.submit": "Run Command",
    "command.newline": "Insert New Line in Command"
  };
  for (const definition of registry.shortcutDefinitions || []) {
    if (labels[definition.id]) definition.label = t(labels[definition.id]);
    if (definition.scope === "command-line") definition.scopeLabel = t("Command Line");
  }
  for (const button of document.querySelectorAll(".console-location")) button.title = t("Open {path} at line {line}", { path: button.dataset.path, line: button.dataset.line });
  registry.renderOutputSearch?.();
});
