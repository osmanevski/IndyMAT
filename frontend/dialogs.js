import shared from "./state.js";
import registry from "./registry.js";
import { t } from "./i18n.js";

function wordUnderCursor() {
  let doc = shared.editor.state.doc.toString(), head = shared.editor.state.selection.main.head, from = head, to = head;
  while (from > 0 && /[A-Za-z0-9_]/.test(doc[from - 1])) from--;
  while (to < doc.length && /[A-Za-z0-9_]/.test(doc[to])) to++;
  return doc.slice(from, to);
}
async function showDocumentation(name) {
  if (!name) throw new Error(t("Place the cursor on a function name."));
  let waiting = registry.el("div", "muted-empty", t("Loading Octave help…"));
  registry.modal(name + " — " + t("Help"), waiting);
  let data = await registry.api("help?name=" + encodeURIComponent(name));
  let wrap = registry.el("div", "doc-result");
  if (data.found) {
    wrap.append(registry.el("p", "doc-origin", data.origin_label), registry.el("pre", "", data.text));
  } else wrap.append(registry.el("p", "", data.message));
  registry.modal(name + " — " + t("Help"), wrap);
}
function help() {
  let wrap = registry.el("div"), form = registry.el("form", "doc-search"), input = registry.el("input"), button = registry.el("button", "", t("Search"));
  input.setAttribute("aria-label", t("Search Octave help"));
  input.placeholder = t("Function name, e.g. fft");
  button.type = "submit";
  form.append(input, button);
  form.onsubmit = (e) => {
    e.preventDefault();
    registry.safe(() => showDocumentation(input.value.trim()));
  };
  let info = registry.el("div");
  info.innerHTML = `<div class="help-grid"><section><h3>${t("Keyboard Shortcuts")}</h3><div class="shortcut">${t("Function help at cursor")} <kbd>F1</kbd></div><div class="shortcut">${t("Save")} <kbd>⌘ / Ctrl S</kbd></div><div class="shortcut">${t("Go to File")} <kbd>⌘ / Ctrl P</kbd></div><div class="shortcut">${t("Font size")} <kbd>⌘ / Ctrl + − 0</kbd></div><div class="shortcut">${t("Run File")} <kbd>F5</kbd></div><div class="shortcut">${t("Run Selected Code")} <kbd>F9</kbd></div><div class="shortcut">${t("Run the %% section")} <kbd>⌘ / Ctrl Enter</kbd></div><div class="shortcut">${t("Search in Editor")} <kbd>⌘ / Ctrl F</kbd></div><div class="shortcut">${t("Stop the running computation")} <kbd>Ctrl C</kbd></div><div class="shortcut">${t("Command History")} <kbd>↑ / ↓</kbd></div><h3 style="margin-top:20px">${t("Files and Variables")}</h3><p>${t("Type <code>doc fft</code> in the Command Window for GNU Octave help. Run saves the file first. Click a variable to inspect its contents. Click the breakpoint gutter of a saved .m file to start debugging.")}</p></section><section><h3>${t("Compute Engine")}</h3><p>${t("IndyMAT runs on GNU Octave. Matrices, .m scripts and functions, 2-D/3-D plots, FFT, and differential equations are available. The signal and control examples require their respective packages.")}</p><h3>${t("Compatibility Limits")}</h3><p>${t("Supported functions depend on GNU Octave and installed packages. The debugger uses the documented subset of GNU Octave file and line breakpoints and step commands. Figures appear as PNGs when execution ends; they are not live animations or graphics callbacks.")}</p><h3>${t("Session")}</h3><p>${t("Variables persist while Octave is open; use <code>save('veriler.mat')</code> to keep them. Resetting the session clears memory. Run only code you trust.")}</p></section></div>`;
  wrap.append(form, info);
  registry.modal(t("Help"), wrap);
  input.focus();
}
async function packageSession(action, name) {
  registry.requireIdle();
  let result = await registry.api("package-session", { action, name });
  shared.lastJob = result.job;
  shared.activeOutput = null;
  shared.busy = true;
  registry.setStatus({ status: "running" });
  registry.$("#modal").close();
  registry.toast(action === "load" ? t("Loading package {name}.", { name }) : t("Unloading package {name}.", { name }));
}
async function watchPackageJob(job, output, refresh) {
  for (; ; ) {
    let state = await registry.api("package-job?id=" + encodeURIComponent(job));
    output.textContent = state.output || "";
    if (state.status !== "running") {
      if (state.status === "done") {
        registry.toast(t("Package operation completed."));
        await refresh();
      } else registry.toast(state.error || t("Package operation failed."));
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
}
async function packages() {
  let wrap = registry.el("div", "package-view");
  registry.modal(t("Packages"), wrap);
  async function refresh() {
    let data = await registry.api("packages");
    wrap.replaceChildren();
    let list = registry.el("div", "package-list");
    for (let p of data.packages) {
      let row = registry.el("div", "package-row"), identity = registry.el("div");
      identity.append(registry.el("b", "", p.name), registry.el("span", "", p.version), registry.el("span", "package-state", p.loaded ? t("Loaded in this session") : t("Not loaded")));
      let actions = registry.el("div"), toggle = registry.el("button", "small-button", p.loaded ? t("Unload") : t("Load")), remove = registry.el("button", "small-button", t("Remove"));
      toggle.onclick = () => registry.safe(() => packageSession(p.loaded ? "unload" : "load", p.name));
      remove.onclick = () => registry.safe(async () => {
        if (!confirm(t("Remove package {name} from the project folder?", { name: p.name }))) return;
        let result = await registry.api("package-admin", { action: "uninstall", name: p.name, confirm: true });
        let output = registry.el("pre", "package-output");
        wrap.append(output);
        await watchPackageJob(result.job, output, refresh);
      });
      actions.append(toggle, remove);
      row.append(identity, actions);
      list.append(row);
    }
    if (!data.packages.length) list.append(registry.el("div", "muted-empty", t("No packages installed in the project.")));
    let form = registry.el("form", "package-install"), input = registry.el("input"), button = registry.el("button", "", t("Install from Octave Forge"));
    input.setAttribute("aria-label", t("Package name"));
    input.placeholder = t("Package name");
    button.type = "submit";
    form.append(input, button);
    form.onsubmit = (e) => {
      e.preventDefault();
      registry.safe(async () => {
        let name = input.value.trim();
        if (!confirm(t("Install package {name} from Octave Forge into the project's .packages folder?", { name }))) return;
        let result = await registry.api("package-admin", { action: "install", source: "forge", name, confirm: true }), output = registry.el("pre", "package-output");
        wrap.append(output);
        button.disabled = input.disabled = true;
        await watchPackageJob(result.job, output, refresh);
      });
    };
    wrap.append(registry.el("p", "subtle", t("Installing and removing packages only changes the project's .packages folder. Forge installation may fail when network access is unavailable.")), list, form);
  }
  await refresh();
}
function examples() {
  let list = registry.el("div");
  const descriptions = { "hosgeldin.m": [t("Signal and FFT"), t("5 Hz + 20 Hz · FFT · two figures")], "matrisler.m": [t("Linear Algebra"), t("Equation system · eigenvalues · SVD")], "yuzey_3d.m": [t("3-D Surface"), t("Peaks · colormap · 3-D view")], "diferansiyel_denklem.m": [t("Damped Oscillation"), t("ode45 · phase portrait")], "kontrol_sistemi.m": [t("Control System"), t("Transfer function · step · Bode · control package")], "filtre_tasarimi.m": [t("Filter Design"), t("Butterworth · filtfilt · signal package")], "veri_analizi.m": [t("Data Analysis"), t("Tables · string · datetime · datatypes package")] };
  for (let [name, [title, desc]] of Object.entries(descriptions)) {
    let b = registry.el("button", "example-card");
    b.append(registry.el("b", "", title), registry.el("span", "", desc));
    b.onclick = () => registry.safe(async () => {
      await registry.openFile(registry.joinPath(shared.defaultWorkspace, "examples/" + name));
      registry.$("#modal").close();
    });
    list.append(b);
  }
  registry.modal(t("Examples"), list);
}

registry.setupDialogs = () => {
  for (let b of document.querySelectorAll("[data-action]")) b.onclick = () => registry.safe(() => ({ new: registry.createUntitled, save: registry.saveActive, "save-as": registry.saveAs, import: () => registry.$("#file-input").click(), help, examples, packages })[b.dataset.action]());
  registry.$("#file-input").onchange = () => registry.safe(async () => {
    for (let file of registry.$("#file-input").files) {
      if (file.size > 2e7) throw new Error(t("File size limit is 20 MB."));
      const base64 = await new Promise((resolve) => {
        let reader = new FileReader();
        reader.onload = () => resolve(reader.result.split(",")[1]);
        reader.readAsDataURL(file);
      });
      await registry.api("import", { path: registry.joinPath(shared.currentFolder, file.name), data: base64 });
    }
    await registry.refreshFiles(false);
    registry.toast(t("Files imported into the Current Folder."));
    registry.$("#file-input").value = "";
  });

};
Object.assign(registry, { wordUnderCursor, showDocumentation, help, packageSession, watchPackageJob, packages, examples });
