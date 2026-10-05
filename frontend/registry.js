// Feature modules may load before the keyboard dispatcher (including isolated
// feature harnesses). Retain registrations until the dispatcher attaches.
const pendingShortcuts = [];
export default {
  pendingShortcuts,
  registerShortcut(definition) {
    pendingShortcuts.push(definition);
  }
};
