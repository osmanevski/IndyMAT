import shared from "./state.js";
import registry from "./registry.js";
import "./core.js";
import "./native_window.js";
import "./shortcut_registry.js";
import "./settings.js";
import "./layout.js";
import "./symbols.js";
import "./editor.js";
import "./outline.js";
import "./files.js";
import "./file_ops.js";
import "./debugger.js";
import "./console.js";
import "./command_window.js";
import "./history_panel.js";
import "./variable_editor.js";
import "./workspace.js";
import "./publish.js";
import "./figures.js";
import "./poll.js";
import "./dialogs.js";
import "./assistant.js";
import "./bootstrap.js";

shared.token = location.hash.slice(1) || sessionStorage.getItem("mf-token") || "";
if (location.hash) {
  sessionStorage.setItem("mf-token", shared.token);
  history.replaceState(null, "", location.pathname);
}

registry.setupSettings();
registry.setupLayout();
// The CSP allows this bundled script; reveal only after stored language is applied.
document.body.style.removeProperty("visibility");
registry.setupEditor();
registry.setupEditorIntel();
registry.setupModal();
registry.setupFileSearch();
registry.setupFileOps();
registry.setupCommandWindow();
registry.setupHistoryPanel();
registry.setupVariableSearch();
registry.captureEmptyPlot();
registry.setupDialogs();
registry.setupAssistant();
registry.setupShortcutRegistry();
registry.bootstrap();
