import shared from "./state.js";
import registry from "./registry.js";
import { t, onLanguageChange } from "./i18n.js";

function rememberFolder(path) {
  if (shared.folderHistory[shared.folderIndex] === path) return;
  shared.folderHistory = shared.folderHistory.slice(0, shared.folderIndex + 1);
  shared.folderHistory.push(path);
  shared.folderIndex = shared.folderHistory.length - 1;
}
function renderFolder() {
  if (!shared.currentFolder) return;
  let address = registry.$("#folder-address");
  address.value = shared.folderRoot !== shared.defaultWorkspace && shared.currentFolder.startsWith(shared.folderRoot) ? "~" + shared.currentFolder.slice(shared.folderRoot.length) : shared.currentFolder;
  address.title = shared.currentFolder;
  if (document.activeElement !== address) address.scrollLeft = address.scrollWidth;
  registry.$("#folder-back").disabled = shared.folderIndex <= 0;
  registry.$("#folder-forward").disabled = shared.folderIndex >= shared.folderHistory.length - 1;
  registry.$("#folder-up").disabled = shared.currentFolder === shared.folderRoot;
  let nav = registry.$("#folder-breadcrumb");
  nav.replaceChildren();
  let path = shared.folderRoot, label = shared.folderRoot === shared.defaultWorkspace ? shared.folderRoot.split("/").pop() : "~", rootButton = registry.el("button", "", label || "/");
  rootButton.title = shared.folderRoot;
  rootButton.onclick = () => registry.safe(() => navigateFolder(shared.folderRoot));
  nav.append(rootButton);
  let relative = shared.currentFolder.slice(shared.folderRoot.length).replace(/^\//, "");
  for (let part of relative.split("/").filter(Boolean)) {
    path += "/" + part;
    let target = path, b = registry.el("button", "", part);
    b.title = target;
    b.onclick = () => registry.safe(() => navigateFolder(target));
    nav.append(b);
  }
}
function renderFiles() {
  const list = registry.$("#files"), query = registry.$("#file-search").value.toLocaleLowerCase("tr");
  list.replaceChildren();
  for (let f of shared.files.filter((f2) => f2.name.toLocaleLowerCase("tr").includes(query))) {
    let selected = registry.getSelectedFile?.() === f.path;
    let b = registry.el("button", "file-button" + (f.directory ? " folder" : "") + (shared.active?.path === f.path ? " active" : "") + (selected ? " selected" : ""));
    b.dataset.path = f.path;
    b.title = f.path + (f.directory ? "" : f.editable ? "" : " " + t("— Click to download"));
    b.append(registry.fileLabel(f.name));
    b.onclick = () => {
      registry.selectFile?.(f.path);
      registry.safe(() => f.directory ? navigateFolder(f.path) : f.editable ? registry.openFile(f.path) : downloadFile(f.path));
    };
    b.oncontextmenu = (event) => registry.openFileContextMenu?.(event, f.path);
    b.onkeydown = registry.registerShortcut ? null : (event) => registry.fileRowKeydown?.(event, f.path);
    list.append(b);
  }
  if (!list.children.length) list.append(registry.el("div", "muted-empty", t("No supported files were found in this folder.")));
  if (shared.listingTruncated) list.append(registry.el("div", "file-limit", t("Showing the first 1000 items. The list was limited.")));
  registry.updateFileOpToolbar?.();
}
async function refreshFiles(record = true) {
  let data = await registry.api("files");
  shared.files = data.entries || [];
  shared.listingTruncated = !!data.truncated;
  shared.defaultWorkspace = data.workspace;
  shared.folderRoot = data.root;
  shared.draftScope = data.root;
  if (!shared.folderHistory.length) {
    shared.folderHistory = [data.current];
    shared.folderIndex = 0;
  } else if (record && data.current !== shared.folderHistory[shared.folderIndex]) rememberFolder(data.current);
  shared.currentFolder = data.current;
  registry.invalidateSymbolIndex?.();
  renderFolder();
  renderFiles();
  return data;
}
async function navigateFolder(path, record = true) {
  registry.requireIdle();
  let result = await registry.api("folder", { path });
  shared.lastJob = result.job;
  shared.activeOutput = null;
  shared.busy = true;
  registry.setStatus({ status: "running", elapsed: 0 });
  if (record) rememberFolder(result.path);
  shared.currentFolder = result.path;
  await refreshFiles(false);
}
function quickOpen() {
  let wrap = registry.el("div", "quick-open"), input = registry.el("input"), list = registry.el("div", "quick-file-list"), selected = 0;
  input.setAttribute("aria-label", t("Go to File"));
  input.placeholder = t("Enter a file name");
  wrap.append(input, list);
  function matches() {
    let q = input.value.toLocaleLowerCase("tr");
    return shared.files.filter((f) => f.editable && f.path.toLocaleLowerCase("tr").includes(q)).slice(0, 100);
  }
  function draw() {
    let found = matches();
    selected = Math.min(selected, Math.max(0, found.length - 1));
    list.replaceChildren();
    found.forEach((f, i) => {
      let b = registry.el("button", "quick-file" + (i === selected ? " selected" : ""), f.path);
      b.onclick = () => registry.safe(async () => {
        await registry.openFile(f.path);
        registry.$("#modal").close();
      });
      list.append(b);
    });
    if (!found.length) list.append(registry.el("div", "muted-empty", t("No files found.")));
  }
  input.oninput = () => {
    selected = 0;
    draw();
  };
  input.onkeydown = (e) => {
    let found = matches();
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      selected = Math.max(0, Math.min(found.length - 1, selected + (e.key === "ArrowDown" ? 1 : -1)));
      draw();
    }
    if (e.key === "Enter" && found[selected]) {
      e.preventDefault();
      registry.safe(async () => {
        await registry.openFile(found[selected].path);
        registry.$("#modal").close();
      });
    }
  };
  draw();
  registry.modal(t("Go to File"), wrap);
  const stopLanguage = onLanguageChange(() => {
    input.setAttribute("aria-label", t("Go to File"));
    input.placeholder = t("Enter a file name");
    registry.$("#modal-title").textContent = t("Go to File");
    draw();
  });
  registry.$("#modal").addEventListener("close", stopLanguage, { once: true });
  input.focus();
}
async function downloadFile(path) {
  let url = await registry.blobAPI("download?path=" + encodeURIComponent(path));
  download(url, path.split("/").pop());
  setTimeout(() => URL.revokeObjectURL(url), 5e3);
}
function download(url, name) {
  let a = registry.el("a");
  a.href = url;
  a.download = name;
  a.click();
}

registry.setupFileSearch = () => {
  registry.$("#file-search").oninput = renderFiles;
};
Object.assign(registry, { rememberFolder, renderFolder, renderFiles, refreshFiles, navigateFolder, quickOpen, downloadFile, download });

onLanguageChange(() => {
  renderFolder();
  renderFiles();
});
