import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange, getLanguage } from "./i18n.js";

const selectedHistory = new Set();

function historyDate(created) {
  let date = new Date(created * 1000);
  if (!Number.isFinite(date.getTime())) return t("Undated");
  let today = new Date(), yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  let locale = getLanguage() === "tr" ? "tr-TR" : "en-US";
  let key = date.toLocaleDateString(locale);
  if (key === today.toLocaleDateString(locale)) return t("Today");
  if (key === yesterday.toLocaleDateString(locale)) return t("Yesterday");
  return date.toLocaleDateString(locale, { day: "numeric", month: "long", year: "numeric" });
}

function setCommandFromHistory(command) {
  let input = registry.$("#command");
  input.value = command;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.selectionStart = input.selectionEnd = command.length;
  input.focus();
}

function selectedEntries() {
  return (shared.historyEntries || []).filter((entry) => selectedHistory.has(entry.id));
}

function updateHistoryActions() {
  let count = selectedHistory.size;
  registry.$("#history-run").disabled = !count;
  registry.$("#history-copy").disabled = !count;
  registry.$("#history-delete").disabled = !count;
}

function renderHistory() {
  let locale = getLanguage() === "tr" ? "tr" : "en";
  let root = registry.$("#history"), query = registry.$("#history-search")?.value.toLocaleLowerCase(locale) || "";
  root.replaceChildren();
  let entries = (shared.historyEntries || []).filter((entry) => entry.code.toLocaleLowerCase(locale).includes(query));
  let known = new Set((shared.historyEntries || []).map((entry) => entry.id));
  for (let id of selectedHistory) if (!known.has(id)) selectedHistory.delete(id);
  let lastGroup = "";
  for (let entry of [...entries].reverse()) {
    let date = historyDate(entry.created), session = entry.session === shared.historySession ? t("This session") : t("Previous session"), group = `${date}\0${entry.session}`;
    if (group !== lastGroup) {
      let heading = registry.el("div", "history-group", `${date} · ${session}`);
      root.append(heading);
      lastGroup = group;
    }
    let row = registry.el("div", "history-row"), checkbox = registry.el("input"), command = registry.el("button", "history-item", entry.code);
    checkbox.type = "checkbox";
    checkbox.checked = selectedHistory.has(entry.id);
    checkbox.setAttribute("aria-label", t("Select command from history"));
    checkbox.onchange = () => {
      if (checkbox.checked) selectedHistory.add(entry.id);
      else selectedHistory.delete(entry.id);
      row.classList.toggle("selected", checkbox.checked);
      updateHistoryActions();
    };
    command.type = "button";
    command.title = t("Send to Command Window: {command}", { command: entry.code });
    command.onclick = () => setCommandFromHistory(entry.code);
    row.classList.toggle("selected", checkbox.checked);
    row.append(checkbox, command);
    root.append(row);
  }
  if (!entries.length) root.append(registry.el("div", "history-empty", query ? t("No matching commands.") : t("Command History is empty.")));
  registry.$("#history-count").textContent = (shared.historyEntries || []).length;
  updateHistoryActions();
}

async function refreshHistory() {
  let detail = await registry.api("history-detail");
  shared.historyEntries = detail.entries;
  shared.historySession = detail.current_session;
  shared.commands = detail.entries.map((entry) => entry.code);
  shared.cmdIndex = shared.commands.length;
  renderHistory();
  return detail;
}

async function runSelectedHistory() {
  let entries = selectedEntries();
  if (!entries.length) return;
  let code = entries.map((entry) => entry.code).join("\n");
  await registry.execute(code, "code", "", entries.length === 1 ? entries[0].code : t("History · {count} commands", { count: entries.length }), false, code);
}

async function copySelectedHistory() {
  let text = selectedEntries().map((entry) => entry.code).join("\n");
  if (!text) return;
  await navigator.clipboard.writeText(text);
  registry.toast(t("Selected commands copied to the clipboard."));
}

async function deleteSelectedHistory() {
  let ids = selectedEntries().map((entry) => entry.id);
  if (!ids.length) return;
  await registry.api("history", { action: "delete", ids });
  selectedHistory.clear();
  await refreshHistory();
}

async function clearHistory() {
  if (!confirm(t("Permanently clear all Command History?"))) return;
  await registry.api("history", { action: "clear", confirm: true });
  selectedHistory.clear();
  await refreshHistory();
}

function setupHistoryPanel() {
  registry.$("#history-search").oninput = renderHistory;
  registry.on("#history-run", runSelectedHistory);
  registry.on("#history-copy", copySelectedHistory);
  registry.on("#history-delete", deleteSelectedHistory);
  registry.on("#history-clear", clearHistory);
  updateHistoryActions();
}

Object.assign(registry, { clearHistory, deleteSelectedHistory, refreshHistory, renderHistory, runSelectedHistory, setupHistoryPanel });
onLanguageChange(renderHistory);
