import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";
import utils from "./assistant_utils.cjs";
import modelUtils from "./assistant_models_utils.cjs";

shared.assistant = { conversations: [], active: null, providers: [], loaded: false };
const model = shared.assistant;
const catalogs = new Map();
const catalogRequests = new Map();
let controls;
let lifecycleInstalled = false;
const sessionJobs = new Map();
const $ = (selector) => registry.$(selector);
const el = (tag, cls, text) => registry.el(tag, cls, text);

const ICONS = {
  plus: "M8 3v10M3 8h10",
  history: "M2.5 8a5.5 5.5 0 1 0 1.7-4M2.5 3v2.6h2.6M8 5v3.2l2 1.3",
  trash: "M3.5 4.5h9M6.5 4.5V3h3v1.5M5 4.5l.5 8h5l.5-8",
  close: "M4 4l8 8M12 4l-8 8",
  send: "M8 12.5V3.5M4 7.5l4-4 4 4",
  stop: "M5 5h6v6H5z",
  file: "M4.5 2.5h4.5l2.5 2.5v8.5h-7zM9 2.5V5h2.5"
};
function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", ICONS[name]);
  svg.append(path);
  return svg;
}
function iconButton(name, label) {
  const button = el("button", "assistant-icon");
  button.type = "button";
  button.title = label;
  button.setAttribute("aria-label", label);
  button.append(icon(name));
  return button;
}
function prose(text) {
  // Light formatting from tokens; everything is a text node, nothing from the model becomes markup.
  const wrap = el("div", "assistant-prose");
  const inline = (parent, value) => {
    for (const token of utils.inlineTokens(value)) parent.append(token.type === "text" ? document.createTextNode(token.text) : el(token.type === "code" ? "code" : "strong", "", token.text));
  };
  for (const block of utils.proseBlocks(text)) {
    if (block.type === "list") {
      const list = el("ul");
      for (const item of block.items) inline(list.appendChild(el("li")), item);
      wrap.append(list);
    } else inline(wrap.appendChild(el(block.type === "heading" ? "h4" : "p")), block.text);
  }
  return wrap;
}
function applyAssistantLayout() {
  const settings = shared.settings?.assistant;
  if (!settings) return;
  $("#assistant-panel").hidden = !settings.open;
  $("#assistant-divider").hidden = !settings.open;
  $("#assistant-panel").style.width = settings.width + "px";
  $("#toggle-assistant").setAttribute("aria-expanded", String(settings.open));
  document.body.classList.toggle("assistant-open", !!settings.open);
}
function toggleAssistant() {
  shared.settings.assistant.open = !shared.settings.assistant.open;
  registry.saveSettings();
  applyAssistantLayout();
  if (shared.settings.assistant.open) {
    registry.safe(loadProviders);
    controls.input.focus();
  }
}
async function loadProviders() {
  try {
    model.providers = await registry.api("assistant/providers");
  } catch (error) {
    // An older running server has no assistant routes; say so instead of "checking" forever.
    model.providers = ["claude", "codex", "agy"].map((id) => ({ id, available: false, reason: t("Restart IndyMAT to use the assistant.") }));
    model.loaded = true;
    renderProviderOptions();
    throw error;
  }
  model.loaded = true;
  renderProviderOptions();
}
function effortLabel(effort) {
  return { low: t("Low"), medium: t("Medium"), high: t("High"), xhigh: t("Extra high"), max: t("Maximum"), ultra: t("Ultra") }[effort] || effort;
}
function renderModelOptions() {
  const provider = controls.provider.value;
  const catalog = catalogs.get(provider);
  const conversation = model.active;
  const stored = shared.settings.assistant.models?.[provider] || { model: "", effort: "" };
  const choice = conversation?.provider === provider && conversation.model !== undefined ? { model: conversation.model, effort: conversation.effort } : stored;
  const available = catalog || { models: [{ id: "", label: "Default", efforts: [] }], efforts_separate: provider !== "agy" };
  const selected = modelUtils.modelChoice(available, choice.model, choice.effort);
  controls.model.replaceChildren();
  for (const item of available.models) {
    const option = el("option", "", item.id ? item.label : t("Model: default"));
    option.value = item.id;
    controls.model.append(option);
  }
  // Keep the fixed label visible if a language rebuild happens during discovery.
  if (conversation?.id && choice.model && !available.models.some((item) => item.id === choice.model)) {
    const option = el("option", "", conversation.modelLabel || choice.model);
    option.value = choice.model;
    controls.model.append(option);
    selected.model = choice.model;
  }
  controls.model.value = selected.model;
  controls.model.title = catalog?.note || (catalog ? controls.model.selectedOptions[0]?.textContent : t("Loading models…"));
  controls.model.setAttribute("aria-busy", String(!catalog));
  controls.effort.replaceChildren(el("option", "", t("Effort: default")));
  controls.effort.options[0].value = "";
  for (const effort of available.models.find((item) => item.id === selected.model)?.efforts || []) {
    const option = el("option", "", effortLabel(effort));
    option.value = effort;
    controls.effort.append(option);
  }
  controls.effort.value = selected.effort;
  controls.effort.hidden = !available.efforts_separate;
  controls.effort.title = catalog?.note || t("Reasoning effort");
  const running = !!conversation?.running;
  controls.model.disabled = !catalog || running || !!conversation?.id;
  controls.effort.disabled = !catalog || running || !!conversation?.id && provider !== "codex";
}
async function loadModels() {
  const provider = controls.provider.value;
  if (!catalogs.has(provider)) {
    if (!catalogRequests.has(provider)) catalogRequests.set(provider, registry.api("assistant/models?provider=" + encodeURIComponent(provider)).catch(() => ({ models: [{ id: "", label: "Default", efforts: [] }], efforts_separate: provider !== "agy", note: t("Could not load models from the local program. Default uses its own setting; try again after ten minutes.") })));
    const catalog = await catalogRequests.get(provider);
    catalogs.set(provider, catalog);
  }
  if (controls.provider.value === provider) {
    renderRunning();
    renderList();
  }
}
function saveModelChoice() {
  const provider = controls.provider.value;
  const choice = { model: controls.model.value, effort: controls.effort.hidden ? "" : controls.effort.value };
  shared.settings.assistant.models ||= {};
  shared.settings.assistant.models[provider] = choice;
  if (model.active) Object.assign(model.active, choice);
  registry.saveSettings();
  renderModelOptions();
  renderList();
}
function newConversation() {
  if (model.conversations.length >= 24) return registry.toast(t("Assistant conversation limit reached for this app run."));
  if (model.active) model.active.draft = controls.input.value;
  const provider = model.providers.find((item) => item.available && item.id === controls.provider.value)?.id || model.providers.find((item) => item.available)?.id || "claude";
  const conversation = { id: null, provider, mode: shared.settings.assistant.mode || "ask", sessionAccess: shared.settings.assistant.sessionAccess || "none", messages: [], after: 0, running: false, draft: "" };
  model.conversations.push(conversation);
  model.active = conversation;
  renderConversation();
}
function renderProviderOptions() {
  controls.provider.replaceChildren();
  for (const id of ["claude", "codex", "agy"]) {
    const provider = model.providers.find((item) => item.id === id);
    const name = { claude: "Claude", codex: "Codex", agy: "Antigravity" }[id];
    const option = el("option", "", name + (provider?.available ? provider.version ? " — " + provider.version : "" : " — " + (provider?.reason || t("Checking availability…"))));
    option.value = id;
    option.disabled = !provider?.available;
    controls.provider.append(option);
  }
  if (model.active && !model.active.id && !model.providers.some((item) => item.id === model.active.provider && item.available)) model.active.provider = model.providers.find((item) => item.available)?.id || "claude";
  controls.provider.value = model.active?.provider || "claude";
  renderRunning();
  registry.safe(loadModels);
}
function renderTabs() {
  for (const option of controls.provider.options) {
    const tab = controls.tabs[option.value];
    tab.disabled = option.disabled || controls.provider.disabled && controls.provider.value !== option.value;
    tab.title = option.textContent;
    tab.setAttribute("aria-selected", String(controls.provider.value === option.value));
  }
}
function renderList() {
  controls.list.replaceChildren();
  model.conversations.forEach((conversation, index) => {
    const button = el("button", conversation === model.active ? "active" : "", t("Conversation {number}", { number: index + 1 }) + " · " + { claude: "Claude", codex: "Codex", agy: "Antigravity" }[conversation.provider] + (conversation.running ? " · " + t("Running…") : ""));
    button.onclick = () => {
      if (model.active) model.active.draft = controls.input.value;
      model.active = conversation;
      renderConversation();
    };
    controls.list.append(button);
  });
  const active = model.active;
  const first = active?.messages.find((message) => message.role === "user")?.text || "";
  controls.title.textContent = first ? first.split("\n")[0].slice(0, 60) : t("New conversation");
  // Prefer the model the program reported over the chosen label: it is what actually runs.
  const shownModel = active?.actualModel || (active?.id && active.model ? active.modelLabel || active.model : "");
  if (shownModel) controls.title.append(el("span", "assistant-model-suffix", " · " + shownModel));
  controls.history.hidden = model.conversations.length < 2;
}
function renderRunning() {
  const conversation = model.active;
  const running = !!conversation?.running;
  controls.provider.disabled = running || !!conversation?.id;
  renderModelOptions();
  controls.mode.disabled = running || !!conversation?.id;
  const ask = controls.mode.querySelector('option[value="ask"]');
  const approvalsAvailable = ["claude", "codex"].includes(controls.provider.value);
  ask.disabled = !approvalsAvailable;
  ask.title = approvalsAvailable ? "" : t("Approvals for this program arrive in a later step");
  controls.modeNote.textContent = approvalsAvailable ? "" : t("Approvals for this program arrive in a later step");
  if (!approvalsAvailable && !conversation?.id && (conversation?.mode || controls.mode.value) === "ask") {
    if (conversation) conversation.mode = "read-only";
    controls.mode.value = "read-only";
  }
  const agy = controls.provider.value === "agy";
  if (agy && conversation && !conversation.id) conversation.sessionAccess = "none";
  controls.session.value = conversation?.sessionAccess || (agy ? "none" : shared.settings.assistant.sessionAccess || "none");
  controls.session.disabled = agy || running || !!conversation?.id;
  controls.sessionNote.textContent = agy ? t("Antigravity needs a one-time MCP registration; not available yet") : controls.session.value === "run" ? t("Code the assistant runs appears in the Command Window and changes your variables.") : "";
  controls.remove.disabled = !conversation || running && (conversation.provider !== "codex" || !conversation.id);
  controls.send.disabled = running && (conversation?.provider !== "codex" || !conversation?.id) || !catalogs.has(controls.provider.value) || !model.providers.some((item) => item.id === controls.provider.value && item.available);
  controls.stop.hidden = !running;
  controls.stop.disabled = !conversation?.id;
  controls.status.textContent = running ? utils.pendingApprovals(conversation?.messages || []).length ? t("Waiting for your approval") : t("Running…") : conversation?.end ? t({ stopped: "Stopped", failed: "Assistant turn failed.", completed: "Turn completed" }[conversation.end]) : "";
  controls.panel.setAttribute("aria-busy", String(running && !utils.pendingApprovals(conversation?.messages || []).length));
  controls.send.hidden = running && !(conversation?.provider === "codex" && conversation?.id);
  renderTabs();
  renderList();
}
function codeBlock(part) {
  const wrap = el("div", "assistant-code");
  const actions = el("div", "assistant-code-actions");
  const copy = el("button", "", t("Copy"));
  const insert = el("button", "", t("Insert into Editor"));
  copy.onclick = () => registry.safe(async () => {
    await navigator.clipboard.writeText(part.text);
    registry.toast(t("Code copied."));
  });
  insert.onclick = () => {
    const view = shared.editor;
    if (!view || !shared.active) return;
    const cursor = view.state.selection.main.head;
    view.dispatch({ changes: { from: cursor, insert: part.text }, selection: { anchor: cursor + part.text.length }, scrollIntoView: true });
    view.focus();
  };
  actions.append(el("span", "", part.language), copy, insert);
  wrap.append(actions, el("pre", "", part.text));
  return wrap;
}
function approvalLabel(message) {
  if (message.label === "Read file") return t("Read file");
  if (message.label === "Edit file") return t("Edit file");
  if (message.label === "Write file") return t("Write file");
  if (message.label === "Shell") return t("Shell");
  if (message.label === "File search") return t("File search");
  return message.tool || t("Tool activity");
}
function approvalCard(message, conversation) {
  const card = el("div", "assistant-approval" + (message.decision ? " resolved" : ""));
  card.dataset.approvalId = message.id;
  const title = approvalLabel(message) + " · " + message.summary;
  if (message.decision) {
    card.textContent = (message.decision === "deny" ? t("Denied: {action}", { action: title }) : t("Allowed: {action}", { action: title }));
    return card;
  }
  card.setAttribute("role", "group");
  card.setAttribute("aria-label", t("Approval required"));
  card.append(el("strong", "", approvalLabel(message)), el("div", "assistant-approval-summary", message.summary));
  if (message.detail.warning) card.append(el("div", "assistant-note", message.detail.warning));
  const diff = el("pre", "assistant-approval-diff");
  for (const line of utils.approvalLines(message.detail.text)) diff.append(el("span", "assistant-diff-" + line.kind, line.text + "\n"));
  card.append(diff);
  if (message.detail.truncated) card.append(el("div", "assistant-note", t("Approval preview was truncated at 64 KB.")));
  const reason = el("input", "assistant-approval-reason");
  reason.maxLength = 250;
  reason.setAttribute("aria-label", t("Reason for denial (optional)"));
  reason.placeholder = t("Reason for denial (optional)");
  const actions = el("div", "assistant-approval-actions");
  for (const [decision, label] of [["allow", t("Allow")], ["allow-conversation", t("Allow for this conversation")], ["deny", t("Deny")]]) {
    const button = el("button", "", label);
    button.type = "button";
    button.dataset.decision = decision;
    button.onclick = () => registry.safe(async () => {
      for (const item of actions.querySelectorAll("button")) item.disabled = true;
      try {
        const result = await registry.api("assistant/approve", { session: conversation.id, id: message.id, decision, ...(decision === "deny" && reason.value ? { message: reason.value } : {}) });
        conversation.messages = utils.reduceTranscript(conversation.messages, { type: "approval-resolved", id: message.id, decision: result.decision, turn: message.turn });
        if (result.decision !== "deny") conversation.fileActivity = true;
        if (model.active === conversation) {
          renderTranscript();
          renderRunning();
        }
      } catch (error) {
        for (const item of actions.querySelectorAll("button")) item.disabled = false;
        throw error;
      }
    });
    actions.append(button);
  }
  card.append(reason, actions);
  return card;
}
function renderTranscript() {
  const root = controls.transcript;
  const atBottom = root.scrollHeight - root.scrollTop - root.clientHeight < 60;
  const cards = new Map([...root.querySelectorAll(".assistant-approval:not(.resolved)")].map((card) => [card.dataset.approvalId, card]));
  const focused = root.contains(document.activeElement) ? document.activeElement : null;
  root.replaceChildren();
  const messages = model.active?.messages || [];
  if (!messages.length) root.append(el("p", "assistant-empty", t(controls.session.value !== "none" ? "Ask an installed coding agent about files and the session access you granted." : "Ask an installed coding agent about files in the Current Folder. Agents cannot access or execute commands in your Octave session.")));
  for (const message of utils.groupActivity(messages)) {
    if (message.role === "approval") {
      root.append(!message.decision && cards.has(message.id) ? cards.get(message.id) : approvalCard(message, model.active));
      continue;
    }
    if (message.role === "raw" || message.role === "reasoning") {
      const detail = el("details", "assistant-activity assistant-step");
      detail.append(el("summary", "", t(message.role === "raw" ? "Provider event" : "Reasoning summary")), el("pre", "", message.text));
      root.append(detail);
      continue;
    }
    const entry = el("div", "assistant-message assistant-" + message.role);
    if (message.role === "user") {
      entry.append(el("strong", "assistant-role", t("You") + (message.steered ? " · " + t("Sent during this turn") : "")), el("div", "assistant-user-text", message.text));
    } else if (message.role === "assistant") {
      entry.append(el("strong", "assistant-role", t("Assistant")));
      for (const part of utils.splitCodeBlocks(message.text)) entry.append(part.type === "code" ? codeBlock(part) : prose(part.text));
    } else if (message.role === "state") {
      entry.classList.add("assistant-state-" + (message.text || "completed"));
      entry.textContent = t({ stopped: "Stopped", completed: "Turn completed", failed: "Assistant turn failed." }[message.text] || "Turn completed");
    } else if (message.role === "tool" && message.label) {
      entry.className = "assistant-step assistant-step-" + message.kind;
      entry.append(el("strong", "", t(message.label)), el("span", "assistant-step-target", message.target));
      const counts = (message.count > 1 ? "×" + message.count + "  " : "") + (message.counted ? "+" + message.added + " −" + message.removed : "");
      if (counts.trim()) entry.append(el("span", "assistant-step-counts", counts.trim()));
    } else if (message.role === "tool") {
      entry.className = "assistant-step";
      entry.append(el("strong", "", message.name || t("Tool activity")), el("span", "assistant-step-target", message.source ? t(message.text) : message.text));
    } else {
      entry.textContent = (message.role === "file" ? t("File changes") + ": " : "") + (message.source ? t(message.text) : message.text);
    }
    root.append(entry);
  }
  if (focused?.isConnected) focused.focus({ preventScroll: true });
  if (atBottom) root.scrollTop = root.scrollHeight;
}
function renderConversation() {
  renderProviderOptions();
  controls.mode.value = model.active?.mode || shared.settings.assistant.mode || "ask";
  renderRunning();
  controls.input.value = model.active?.draft || "";
  renderTranscript();
  updateAssistantAttachment();
}
function updateAssistantAttachment() {
  if (!controls) return;
  const tab = shared.active;
  controls.chip.textContent = tab ? tab.path.split("/").pop() + (tab.dirty ? " — " + t("Unsaved changes") : "") : t("No active editor file");
  controls.chip.title = tab?.path || "";
  controls.note.textContent = tab?.dirty && controls.attach.checked && !controls.unsaved.checked ? t("File has unsaved changes; only its path will be attached.") : "";
  controls.unsaved.parentElement.hidden = !tab?.dirty;
}
function attachedContext() {
  if (!controls.attach.checked) return {};
  const tab = shared.active;
  const prefix = shared.currentFolder.replace(/\/$/, "") + "/";
  if (!tab || !tab.path.startsWith(prefix)) throw new Error(t("The active file must be inside the Current Folder to attach it."));
  const context = { path: tab.path.slice(prefix.length), dirty: !!tab.dirty, include_unsaved: controls.unsaved.checked };
  if (!tab.dirty || controls.unsaved.checked) {
    const selection = shared.editor.state.selection.main;
    if (!selection.empty) context.selection = shared.editor.state.doc.sliceString(selection.from, selection.to);
    if (tab.dirty) context.content = shared.editor.state.doc.toString();
  }
  return context;
}
function editorState() {
  // Sent with every turn: which file the user has in front of them. Paths only; the server re-derives them.
  return { active: shared.active?.path || null, dirty: !!shared.active?.dirty, open: shared.tabs.map((tab) => tab.path).slice(0, 30) };
}
async function sendAssistant() {
  const selectedProvider = controls.provider.value;
  const selectedConversation = model.active;
  if (!catalogs.has(selectedProvider)) await loadModels();
  if (controls.provider.value !== selectedProvider || model.active !== selectedConversation) return;
  const prompt = controls.input.value;
  if (!prompt.trim() || model.active?.running && (model.active.provider !== "codex" || !model.active.id)) return;
  const context = attachedContext();
  const provider = controls.provider.value;
  const mode = controls.mode.value;
  const sessionAccess = controls.session.value;
  const selectedModel = controls.model.value;
  const effort = controls.effort.hidden ? "" : controls.effort.value;
  if (!model.active) newConversation();
  const conversation = model.active;
  conversation.provider = provider;
  conversation.mode = mode;
  conversation.sessionAccess = sessionAccess;
  conversation.model = selectedModel;
  conversation.effort = effort;
  conversation.modelLabel = catalogs.get(provider)?.models.find((item) => item.id === selectedModel)?.label || selectedModel;
  const steering = conversation.running;
  conversation.running = true;
  conversation.end = null;
  renderRunning();
  // Clear the box now, not when the server answers: text typed meanwhile (a steering message) must survive.
  controls.input.value = "";
  conversation.draft = "";
  try {
    const result = await registry.api("assistant/start", { provider: conversation.provider, mode: conversation.mode, session_access: conversation.sessionAccess, model: conversation.model, effort: conversation.effort, prompt, context, ide: editorState(), ...(conversation.id ? { conversation: conversation.id } : {}) });
    conversation.id = result.session;
    conversation.turn = result.turn;
    conversation.running = true;
    conversation.messages = utils.reduceTranscript(conversation.messages, { type: "user", text: prompt, turn: result.turn, steered: !!result.steered });
    if (model.active === conversation) {
      renderRunning();
      renderTranscript();
    }
    if (!conversation.polling) pollAssistant(conversation);
  } catch (error) {
    if (!steering) conversation.running = false;
    if (model.active === conversation && !controls.input.value) controls.input.value = prompt;
    renderRunning();
    throw error;
  }
}
async function pollAssistant(conversation) {
  conversation.polling = true;
  try {
    const result = await registry.api("assistant/events?session=" + encodeURIComponent(conversation.id) + "&after=" + conversation.after);
    if (!model.conversations.includes(conversation)) {
      conversation.polling = false;
      return;
    }
    for (const event of result.events) {
      if (event.type === "model") {
        conversation.actualModel = event.text;
        continue;
      }
      conversation.messages = utils.reduceTranscript(conversation.messages, event);
      if (event.type === "turn-end") conversation.end = event.state;
    }
    for (const event of result.approvals || []) conversation.messages = utils.reduceTranscript(conversation.messages, event);
    conversation.after = result.after;
    conversation.running = result.running;
    conversation.pollError = false;
    if (model.active === conversation) {
      if (result.events.length || (result.approvals || []).some((event) => !controls.transcript.querySelector(`[data-approval-id="${event.id}"]`))) renderTranscript();
      renderRunning();
    } else renderList();
    // Follow the agent's file changes while it works, not only when the turn ends.
    const edited = result.events.some((event) => event.type === "approval-resolved" && event.decision !== "deny" || event.type === "file" || event.type === "tool" && utils.changesFiles(event));
    if (edited) conversation.fileActivity = true;
    const due = conversation.fileActivity && conversation.mode !== "read-only" && Date.now() - (conversation.refreshedAt || 0) > 900;
    if (!result.running || due) {
      conversation.refreshedAt = Date.now();
      await refreshAssistantFiles();
    }
  } catch (error) {
    if (!conversation.pollError) registry.toast(error.message);
    conversation.pollError = true;
  }
  if (conversation.running) setTimeout(() => pollAssistant(conversation), 250);
  else conversation.polling = false;
}
async function refreshAssistantFiles() {
  await registry.refreshFiles();
  for (const tab of [...shared.tabs]) {
    if (!tab.hash) continue;
    const path = tab.path;
    const hash = tab.hash;
    try {
      const data = await registry.api("file?path=" + encodeURIComponent(path));
      if (!shared.tabs.includes(tab) || tab.path !== path || tab.hash !== hash) continue;
      if (data.hash === hash) {
        tab.assistantDiskChanged = false;
        continue;
      }
      if (tab.dirty || shared.active === tab && shared.editor.state.doc.toString() !== tab.saved) {
        tab.assistantDiskChanged = true;
        continue;
      }
      tab.content = data.content;
      tab.saved = data.content;
      tab.hash = data.hash;
      tab.state = registry.makeState(data.content);
      tab.assistantDiskChanged = false;
      if (shared.active === tab) {
        shared.active = null;
        shared.editor.setState(tab.state);
        shared.active = tab;
        registry.refreshDebugEditor();
        registry.updateCursor();
      }
    } catch {
      // Deleted/unreadable files retain their tab and existing save precondition.
      if (shared.tabs.includes(tab) && tab.path === path && tab.hash === hash) tab.assistantDiskChanged = true;
    }
  }
  registry.renderTabs();
  registry.persistDrafts();
}
function renderAssistantDiskMarks() {
  const tabs = [...document.querySelectorAll("#tabs .editor-tab")];
  shared.tabs.forEach((tab, index) => {
    if (!tab.assistantDiskChanged || !tabs[index]) return;
    tabs[index].classList.add("assistant-disk-changed");
    tabs[index].title += " — " + t("Changed on disk");
    tabs[index].append(el("span", "assistant-disk-note", t("Changed on disk")));
  });
  updateAssistantAttachment();
}

function syncAssistantJobs(state, commandJob = shared.lastJob) {
  for (const job of state.assistant_jobs || []) {
    const key = job.epoch + ":" + job.job;
    let output = sessionJobs.get(key);
    if (!output) {
      const previous = shared.activeOutput;
      output = registry.addConsole("% Assistant (" + { claude: "Claude", codex: "Codex" }[job.provider] + ")\n" + job.code);
      shared.activeOutput = previous;
      sessionJobs.set(key, output);
      while (sessionJobs.size > 100) sessionJobs.delete(sessionJobs.keys().next().value);
    }
    if (job.job === state.job && shared.lastJob === commandJob) {
      shared.lastJob = job.job;
      shared.activeOutput = output;
    }
    output.out.textContent = job.output || (job.job === state.job ? state.output || "" : "");
    output.err.textContent = job.error || (job.job === state.job ? state.error || "" : "");
    if (job.status === "idle" || job.status === "dead") output.time.textContent = (job.elapsed || 0).toFixed(3) + " s";
  }
}

function setupAssistant() {
  const panel = $("#assistant-panel");
  controls = { panel };
  const heading = el("div", "assistant-header");
  const close = iconButton("close", t("Close"));
  const create = iconButton("plus", t("New conversation"));
  close.onclick = toggleAssistant;
  create.onclick = newConversation;
  controls.remove = iconButton("trash", t("Remove conversation"));
  controls.remove.id = "assistant-remove";
  controls.remove.onclick = () => registry.safe(async () => {
    const conversation = model.active;
    if (!conversation || conversation.running && (conversation.provider !== "codex" || !conversation.id)) return;
    if (conversation.id) await registry.api("assistant/remove", { session: conversation.id });
    conversation.running = false;
    model.conversations = model.conversations.filter((item) => item !== conversation);
    model.active = model.conversations.at(-1) || null;
    renderConversation();
  });
  controls.history = iconButton("history", t("Conversations"));
  controls.history.onclick = () => controls.list.classList.toggle("open");
  controls.tabs = {};
  const tabs = el("div", "assistant-tabs");
  tabs.setAttribute("role", "tablist");
  for (const [id, name] of [["claude", "Claude"], ["codex", "Codex"], ["agy", "Antigravity"]]) {
    const tab = el("button", "", name);
    tab.type = "button";
    tab.id = "assistant-tab-" + id;
    tab.setAttribute("role", "tab");
    tab.onclick = () => {
      if (controls.provider.disabled) return;
      controls.provider.value = id;
      controls.provider.onchange();
    };
    controls.tabs[id] = tab;
    tabs.append(tab);
  }
  const tools = el("div", "assistant-header-tools");
  tools.append(controls.history, create, controls.remove, close);
  heading.append(tabs, tools);
  controls.title = el("div", "assistant-title");
  controls.provider = el("select");
  controls.provider.id = "assistant-provider";
  controls.provider.setAttribute("aria-label", t("Assistant provider"));
  controls.provider.onchange = () => {
    if (model.active && !model.active.id) model.active.provider = controls.provider.value;
    if (model.active && !model.active.id) {
      delete model.active.model;
      delete model.active.effort;
    }
    renderRunning();
    renderTranscript();
    registry.safe(loadModels);
  };
  controls.mode = el("select");
  controls.mode.id = "assistant-mode";
  controls.mode.setAttribute("aria-label", t("Assistant access mode"));
  for (const [value, label] of [["ask", t("Ask before changes")], ["read-only", t("Read only")], ["edit", t("Edit files in current folder")]]) {
    const option = el("option", "", label);
    option.value = value;
    controls.mode.append(option);
  }
  controls.mode.onchange = () => {
    if (model.active && !model.active.id) model.active.mode = controls.mode.value;
    shared.settings.assistant.mode = controls.mode.value;
    registry.saveSettings();
  };
  controls.modeNote = el("div", "assistant-mode-note");
  const selectors = el("div", "assistant-selectors");
  controls.session = el("select");
  controls.session.id = "assistant-session-access";
  controls.session.setAttribute("aria-label", t("Session access"));
  for (const [value, label] of [["none", "Session: none"], ["inspect", "See variables and figures"], ["run", "Run code in my session"]]) {
    const option = el("option", "", t(label));
    option.value = value;
    controls.session.append(option);
  }
  controls.session.onchange = () => {
    if (model.active && !model.active.id) model.active.sessionAccess = controls.session.value;
    shared.settings.assistant.sessionAccess = controls.session.value;
    registry.saveSettings();
    renderRunning();
    renderTranscript();
  };
  const sessionLabel = el("label", "assistant-session-label", t("Session access"));
  sessionLabel.htmlFor = controls.session.id;
  controls.sessionNote = el("div", "assistant-session-note");
  controls.model = el("select");
  controls.model.id = "assistant-model";
  controls.model.setAttribute("aria-label", t("Assistant model"));
  controls.model.onchange = () => {
    if (model.active) model.active.model = controls.model.value;
    shared.settings.assistant.models ||= {};
    shared.settings.assistant.models[controls.provider.value] = { model: controls.model.value, effort: controls.effort.value };
    renderModelOptions();
    saveModelChoice();
  };
  controls.effort = el("select");
  controls.effort.id = "assistant-effort";
  controls.effort.setAttribute("aria-label", t("Reasoning effort"));
  controls.effort.onchange = saveModelChoice;
  controls.provider.className = "assistant-native-provider";
  for (const select of [controls.mode, controls.session, controls.model, controls.effort]) select.title = select.getAttribute("aria-label") || "";
  selectors.append(controls.provider, controls.mode, sessionLabel, controls.session, controls.model, controls.effort);
  controls.list = el("div", "assistant-conversations");
  controls.list.setAttribute("aria-label", t("Conversations"));
  controls.transcript = el("div", "assistant-transcript");
  controls.transcript.id = "assistant-transcript";
  const composer = el("form", "assistant-composer");
  const checkbox = (name, id) => {
    const label = el("label");
    const input = el("input");
    input.type = "checkbox";
    input.id = id;
    input.onchange = updateAssistantAttachment;
    label.append(input, el("span", "", t(name)));
    composer.append(label);
    return input;
  };
  controls.attach = checkbox("Attach current file", "assistant-attach");
  controls.chip = el("div", "assistant-attachment");
  composer.append(controls.chip);
  controls.unsaved = checkbox("Include unsaved changes", "assistant-unsaved");
  controls.note = el("div", "assistant-note");
  controls.input = el("textarea");
  controls.input.id = "assistant-input";
  controls.input.rows = 2;
  controls.input.maxLength = 128000;
  controls.input.setAttribute("aria-label", t("Message to assistant"));
  controls.input.placeholder = t("Enter sends; Shift+Enter adds a new line");
  controls.input.oninput = () => { if (model.active) model.active.draft = controls.input.value; };
  controls.input.onkeydown = (event) => {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      registry.safe(sendAssistant);
    }
  };
  controls.status = el("span", "assistant-status");
  controls.status.setAttribute("role", "status");
  controls.send = iconButton("send", t("Send"));
  controls.send.type = "submit";
  controls.send.id = "assistant-send";
  controls.send.classList.add("assistant-send");
  controls.stop = iconButton("stop", t("Stop"));
  controls.stop.id = "assistant-stop";
  controls.stop.classList.add("assistant-stop");
  controls.stop.onclick = () => registry.safe(() => registry.api("assistant/stop", { session: model.active.id }));
  const actions = el("div", "assistant-actions");
  actions.append(selectors, controls.stop, controls.send);
  composer.onsubmit = (event) => {
    event.preventDefault();
    registry.safe(sendAssistant);
  };
  const box = el("div", "assistant-box");
  const context = el("div", "assistant-context");
  context.append(...composer.children);
  box.append(context, controls.input, actions);
  // The file name lives inside its own chip; the second chip only matters while the file has unsaved text.
  controls.attach.parentElement.title = t("Attach current file");
  controls.attach.parentElement.classList.add("assistant-file-chip");
  controls.attach.parentElement.append(controls.chip);
  controls.unsaved.parentElement.classList.add("assistant-unsaved-chip");
  composer.append(controls.modeNote, controls.sessionNote, controls.note, controls.status, box);
  panel.replaceChildren(heading, controls.title, controls.list, controls.transcript, composer);
  $("#toggle-assistant").onclick = toggleAssistant;
  const divider = $("#assistant-divider");
  divider.onpointerdown = (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const start = event.clientX;
    const width = shared.settings.assistant.width;
    divider.setPointerCapture(event.pointerId);
    divider.onpointermove = (move) => {
      shared.settings.assistant.width = Math.max(260, Math.min(600, width + start - move.clientX));
      applyAssistantLayout();
    };
    const finish = () => {
      divider.onpointermove = null;
      registry.saveSettings();
    };
    divider.onpointerup = finish;
    divider.onpointercancel = finish;
  };
  divider.onkeydown = (event) => {
    if (!["ArrowLeft", "ArrowRight"].includes(event.key)) return;
    event.preventDefault();
    shared.settings.assistant.width = Math.max(260, Math.min(600, shared.settings.assistant.width + (event.key === "ArrowLeft" ? 10 : -10)));
    registry.saveSettings();
    applyAssistantLayout();
  };
  if (!lifecycleInstalled) window.addEventListener("pagehide", () => {
    for (const conversation of model.conversations) {
      if (!conversation.running || !conversation.id) continue;
      fetch("/api/assistant/stop", { method: "POST", headers: { "X-MF-Token": shared.token, "Content-Type": "application/json" }, body: JSON.stringify({ session: conversation.id }), keepalive: true }).catch(() => {});
    }
  }, { once: true });
  lifecycleInstalled = true;
  renderConversation();
  applyAssistantLayout();
  if (shared.settings.assistant.open) registry.safe(loadProviders);
}
registry.registerShortcut({ id: "toggle-assistant", label: "Toggle Assistant", scope: "global", scopeLabel: "Global", bindings: ["Mod+Shift+KeyA"], command: toggleAssistant });
Object.assign(registry, { syncAssistantJobs, setupAssistant, applyAssistantLayout, toggleAssistant, updateAssistantAttachment, renderAssistantDiskMarks, refreshAssistantFiles });
onLanguageChange(() => {
  if (!controls) return;
  const saved = { draft: controls.input.value, attach: controls.attach.checked, unsaved: controls.unsaved.checked };
  setupAssistant();
  controls.input.value = saved.draft;
  controls.attach.checked = saved.attach;
  controls.unsaved.checked = saved.unsaved;
  updateAssistantAttachment();
});
