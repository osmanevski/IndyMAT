// Keep the command bindings testable with CodeMirror's actual key dispatcher.
// Printable Shift variants depend on the layout (and w3c-keyname's macOS
// Cmd+Shift fallback assumes US). Function keys provide a reliable alternative.
function editorCommandBindings(registry) {
  const commands = [
    ["F9", "runSelection"],
    ["Mod-Enter", "runSection"],
    ["Shift-Mod-Enter", "runAndAdvance"],
    ["Shift-Alt-Mod-Enter", "runToEnd"],
    ["Ctrl-g", "goToLine"],
    ["Mod-/", "toggleEditorComment"],
    ["Shift-F9", "toggleEditorComment"],
    ["Ctrl-i", "smartIndentSelection"],
    ["Ctrl-,", "foldAllEditor"],
    ["Ctrl-.", "unfoldAllEditor"]
  ];
  return commands.map(([key, command]) => ({
    key,
    run: () => {
      registry.safe(() => registry[command]());
      return true;
    }
  }));
}

function allowGlobalEditorShortcut(event, document) {
  if (document.querySelector("dialog[open]")) return false;
  // Shift-F9 belongs to the focused editor. Modified F-keys must never fall
  // through to the window handler's unmodified Run / Run Selection branches.
  if (["F5", "F9"].includes(event.key) && (event.shiftKey || event.altKey || event.ctrlKey || event.metaKey)) return false;
  const target = event.target;
  if (target?.closest?.("#editor") || target === document.querySelector("#command")) return true;
  if (target?.isContentEditable || target?.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return false;
  return true;
}

module.exports = { allowGlobalEditorShortcut, editorCommandBindings };
