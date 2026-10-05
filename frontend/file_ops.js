import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";

let selectedPath = "";

function selectedItem() {
  return shared.files.find((item) => item.path === selectedPath) || null;
}
function getSelectedFile() {
  return selectedPath;
}
function selectFile(path) {
  selectedPath = path || "";
  registry.renderFiles();
}
function updateFileOpToolbar() {
  let selected = !!selectedItem();
  for (let button of document.querySelectorAll("#file-ops-toolbar [data-file-operation]")) {
    if (!["create_file", "create_folder"].includes(button.dataset.fileOperation)) button.disabled = !selected;
  }
}
function closeFileContextMenu() {
  registry.$("#file-context-menu").hidden = true;
}
function openFileContextMenu(event, path) {
  event.preventDefault();
  event.stopPropagation();
  selectedPath = path;
  registry.renderFiles();
  let menu = registry.$("#file-context-menu");
  menu.hidden = false;
  let width = menu.offsetWidth;
  let height = menu.offsetHeight;
  menu.style.left = Math.min(event.clientX, innerWidth - width - 6) + "px";
  menu.style.top = Math.min(event.clientY, innerHeight - height - 6) + "px";
  menu.querySelector("button")?.focus();
}
function askFileValue(titleSource, labelSource, initial, actionSource) {
  return new Promise((resolve) => {
    let form = registry.el("form", "file-operation-dialog");
    let fieldLabel = registry.el("label", "", t(labelSource));
    let input = registry.el("input");
    let button = registry.el("button", "", t(actionSource));
    let done = false;
    input.value = initial;
    input.required = true;
    input.setAttribute("aria-label", t(labelSource));
    button.type = "submit";
    fieldLabel.append(input);
    form.append(fieldLabel, button);
    form.onsubmit = (event) => {
      event.preventDefault();
      done = true;
      resolve(input.value.trim());
      registry.$("#modal").close();
    };
    let stopLanguage = null;
    registry.$("#modal").addEventListener("close", () => {
      stopLanguage?.();
      if (!done) resolve(null);
    }, { once: true });
    registry.modal(t(titleSource), form);
    stopLanguage = onLanguageChange(() => {
      fieldLabel.firstChild.textContent = t(labelSource);
      button.textContent = t(actionSource);
      input.setAttribute("aria-label", t(labelSource));
      registry.$("#modal-title").textContent = t(titleSource);
    });
    input.focus();
    input.select();
  });
}
function duplicateName(name) {
  let dot = name.lastIndexOf(".");
  if (dot > 0) return name.slice(0, dot) + t("Copy suffix") + name.slice(dot);
  return name + t("Copy suffix");
}
function affectedPath(path, source) {
  return path === source || path.startsWith(source + "/");
}
function destinationPath(path, source, destination) {
  return destination + path.slice(source.length);
}
function relocateOpenPaths(source, destination) {
  for (let tab of shared.tabs) {
    if (!affectedPath(tab.path, source)) continue;
    tab.path = destinationPath(tab.path, source, destination);
  }
  relocateFolderHistory(source, destination);
  registry.renderTabs();
  registry.persistDrafts();
  registry.relocateBreakpointPaths(source, destination);
}
function trashOpenPaths(source) {
  relocateFolderHistory(source, null);
  let activeRemoved = shared.active && affectedPath(shared.active.path, source) && !shared.active.dirty;
  for (let tab of shared.tabs) {
    if (affectedPath(tab.path, source) && tab.dirty) {
      tab.hash = null;
      tab.saved = null;
      tab.dirty = true;
    }
  }
  shared.tabs = shared.tabs.filter((tab) => !affectedPath(tab.path, source) || tab.dirty);
  if (activeRemoved) {
    shared.active = null;
    if (shared.tabs.length) registry.switchTab(shared.tabs[shared.tabs.length - 1]);
    else registry.createUntitled();
  } else {
    registry.renderTabs();
    registry.persistDrafts();
  }
  registry.relocateBreakpointPaths(source, null);
}
function countText(info) {
  if (!info.directory || !info.item_count) return "";
  return info.count_truncated ? " " + t("Folder contains at least {count} items.", { count: info.item_count }) : " " + t("Folder contains {count} items.", { count: info.item_count });
}
async function inspectSelection(item) {
  return registry.api("file-operation", { operation: "inspect", source: item.path });
}
function ensureNoOpenCollision(source, destination) {
  let collision = shared.tabs.find((tab) => !affectedPath(tab.path, source) && affectedPath(tab.path, destination));
  if (collision) throw new Error(t("The destination path is already used by another open tab."));
}
function expectedFileHashes(source) {
  return shared.tabs.filter((tab) => affectedPath(tab.path, source) && tab.hash).map((tab) => ({ path: tab.path, hash: tab.hash }));
}
function relocateFolderHistory(source, destination) {
  let history = [];
  let index = -1;
  shared.folderHistory.forEach((path, oldIndex) => {
    let affected = affectedPath(path, source);
    if (!affected || destination !== null) {
      history.push(affected ? destinationPath(path, source, destination) : path);
      if (oldIndex <= shared.folderIndex) index = history.length - 1;
    }
  });
  shared.folderHistory = history.length ? history : [shared.currentFolder];
  shared.folderIndex = Math.max(0, index);
}
function acceptFileOperationJob(result) {
  if (result.breakpoint_job) {
    shared.lastJob = result.breakpoint_job;
    shared.activeOutput = null;
    shared.busy = true;
    registry.setStatus({ status: "running", elapsed: 0 });
  }
  if (result.warning) registry.toast(result.warning);
}
function keepVisibleSelection(preferred, previousIndex) {
  let selected = shared.files.find((item) => item.path === preferred) || shared.files[Math.min(Math.max(0, previousIndex), shared.files.length - 1)];
  selectedPath = selected?.path || "";
  registry.renderFiles();
}
async function fileOperation(operation) {
  closeFileContextMenu();
  if (operation === "create_file" || operation === "create_folder") {
    let folder = shared.currentFolder;
    let label = operation === "create_file" ? "File name" : "Folder name";
    let name = await askFileValue(operation === "create_file" ? "New File" : "New Folder", label, operation === "create_file" ? "yeni.m" : "yeni_klasor", "Create");
    if (!name) return;
    let result = await registry.api("file-operation", { operation, source: folder, name });
    selectedPath = result.path;
    await registry.refreshFiles(false);
    let created = shared.files.find((entry) => entry.path === result.path);
    if (created?.editable) await registry.openFile(result.path);
    registry.toast(operation === "create_file" ? t("File created.") : t("Folder created."));
    return result;
  }
  let item = selectedItem();
  if (!item) throw new Error(t("Select a file or folder first."));
  let info = await inspectSelection(item);
  if (operation === "rename") {
    if (info.directory && info.item_count && !confirm(t("Rename “{name}”?{details}", { name: item.name, details: countText(info) }))) return;
    let name = await askFileValue("Rename File", "New name", item.name, "Apply");
    if (!name || name === item.name) return;
    let destination = item.path.slice(0, item.path.lastIndexOf("/") + 1) + name;
    ensureNoOpenCollision(item.path, destination);
    let result = await registry.api("file-operation", { operation, source: item.path, name, expected: expectedFileHashes(item.path) });
    acceptFileOperationJob(result);
    relocateOpenPaths(item.path, result.path);
    selectedPath = result.path;
    await registry.refreshFiles(false);
    registry.toast(result.warning || t("Name changed."));
    return result;
  }
  if (operation === "duplicate") {
    if (info.directory && info.item_count && !confirm(t("Copy “{name}”?{details}", { name: item.name, details: countText(info) }))) return;
    let name = await askFileValue("Create a Copy", "Copy name", duplicateName(item.name), "Copy");
    if (!name) return;
    let result = await registry.api("file-operation", { operation, source: item.path, name });
    selectedPath = result.path;
    await registry.refreshFiles(false);
    registry.toast(t("Copy created."));
    return result;
  }
  if (operation === "move") {
    let previousIndex = shared.files.findIndex((entry) => entry.path === item.path);
    if (info.directory && info.item_count && !confirm(t("Move “{name}”?{details}", { name: item.name, details: countText(info) }))) return;
    let destination = await askFileValue("Move to Folder", "Destination folder", shared.currentFolder, "Move");
    if (!destination) return;
    let expected = destination.replace(/\/$/, "") + "/" + item.name;
    ensureNoOpenCollision(item.path, expected);
    let result = await registry.api("file-operation", { operation, source: item.path, destination, expected: expectedFileHashes(item.path) });
    acceptFileOperationJob(result);
    relocateOpenPaths(item.path, result.path);
    selectedPath = result.path;
    await registry.refreshFiles(false);
    keepVisibleSelection(result.path, previousIndex);
    registry.toast(result.warning || t("Item moved."));
    return result;
  }
  if (operation === "trash") {
    let previousIndex = shared.files.findIndex((entry) => entry.path === item.path);
    let dirty = shared.tabs.filter((tab) => tab.dirty && affectedPath(tab.path, item.path));
    let dirtyText = dirty.length ? " " + t("{count} unsaved tabs will remain open and ask for a name when saved again.", { count: dirty.length }) : "";
    if (!confirm(t("Move “{name}” to Trash?{details}{unsaved}", { name: item.name, details: countText(info), unsaved: dirtyText }))) return;
    let result = await registry.api("file-operation", { operation, source: item.path });
    acceptFileOperationJob(result);
    trashOpenPaths(item.path);
    await registry.refreshFiles(false);
    keepVisibleSelection("", previousIndex);
    registry.toast(result.warning || t("Item moved to Trash."));
    return result;
  }
  throw new Error(t("Invalid file operation."));
}
function fileRowKeydown(event, path) {
  if (event.key === "Enter") {
    event.preventDefault();
    event.stopPropagation();
    selectedPath = path;
    registry.safe(() => fileOperation("rename"));
  }
  if (event.metaKey && event.key === "Backspace") {
    event.preventDefault();
    event.stopPropagation();
    selectedPath = path;
    registry.safe(() => fileOperation("trash"));
  }
}
function setupFileOps() {
  for (let button of document.querySelectorAll("[data-file-operation]")) {
    button.onclick = () => registry.safe(() => fileOperation(button.dataset.fileOperation));
  }
  document.addEventListener("pointerdown", (event) => {
    if (!event.target.closest?.("#file-context-menu")) closeFileContextMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") closeFileContextMenu();
  });
  updateFileOpToolbar();
}

Object.assign(registry, { getSelectedFile, selectFile, updateFileOpToolbar, closeFileContextMenu, openFileContextMenu, fileOperation, fileRowKeydown, setupFileOps });
registry.registerShortcut?.({ defaultMatch: { keys: ["Enter"], modifiers: "any" }, id: "files.rename", label: t("Rename File"), scope: "file-list", scopeLabel: t("File List"), bindings: ["Enter"], when: (event) => !!event.target.closest?.(".file-button"), command: (event) => {
  selectedPath = event.target.closest(".file-button").dataset.path;
  registry.safe(() => fileOperation("rename"));
} });
registry.registerShortcut?.({ defaultMatch: { keys: ["Backspace"], modifiers: "meta" }, id: "files.trash", label: t("Move File to Trash"), scope: "file-list", scopeLabel: t("File List"), bindings: ["Mod+Backspace"], when: (event) => !!event.target.closest?.(".file-button"), command: (event) => {
  // Keyboard focus is independent of the toolbar/context-menu selection.
  const row = event.target.closest?.(".file-button");
  if (row) selectedPath = row.dataset.path;
  return registry.safe(() => fileOperation("trash"));
} });
export { relocateOpenPaths, trashOpenPaths, relocateFolderHistory, expectedFileHashes };

onLanguageChange(() => {
  for (const definition of registry.shortcutDefinitions || []) {
    if (definition.id === "files.rename") definition.label = t("Rename File");
    if (definition.id === "files.trash") definition.label = t("Move File to Trash");
    if (["files.rename", "files.trash"].includes(definition.id)) definition.scopeLabel = t("File List");
  }
});
