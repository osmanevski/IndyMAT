function editorIntelBindings(registry, document) {
  return [["F12", "goToDefinition"], ["Shift-F12", "findOccurrences"], ["Alt-F7", "goToDefinition"], ["Alt-F8", "findOccurrences"]].map(([key, command]) => ({
    key,
    run: () => {
      if (document.querySelector("dialog[open]")) return false;
      registry.safe(() => registry[command]());
      return true;
    }
  }));
}

module.exports = { editorIntelBindings };
