const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const schema = require('../frontend/shortcut_registry_utils.cjs');
assert.equal(schema.cloneDefaults().preferences.adaptFileLiterals, false);
assert.equal(schema.sanitizeSettings({ version: 1, preferences: { adaptFileLiterals: true } }, [], true).preferences.adaptFileLiterals, true);
assert.equal(schema.sanitizeSettings({ version: 1, preferences: { adaptFileLiterals: 'true' } }, [], true).preferences.adaptFileLiterals, false);

(async () => {
  let enabled = false, payload, saved = 0, entry;
  const shared = { uiGeneration: 4, active: { path: '/work/entry.m', hash: 'old-hash', dirty: false }, engine: { epoch: 7, cwd: '/work', source_file_adapter_version: 1 }, publishRenders: new Map() };
  const registry = { requireIdle() {}, getSetting: () => enabled, api: async (_, value) => {
    payload = value;
    return { job: 'accepted', source_adapter: enabled ? { epoch: 7, scope: 'entry-file', status: 'adapted' } : undefined,
      source_context: enabled ? { origin: 'entry-file', path: shared.active.path, document: 'server snapshot', span: { start_utf16: 0, end_utf16: 15 } } : undefined };
  }, saveActive: async () => { saved++; shared.active.hash = 'saved-hash'; shared.active.dirty = false; return true; }, poll: async () => {}, el: (_, cls, text) => ({ cls, text }) };
  const context = vm.createContext({ shared, registry, t: (key) => key, setStatus() {}, showAdaptation() {},
    addConsole: () => entry = { wrap: { dataset: {}, querySelector: () => ({ after: (node) => entry.original = node }) } } });
  const source = fs.readFileSync(require.resolve('../frontend/console.js'), 'utf8');
  vm.runInContext(source.slice(source.indexOf('async function execute('), source.indexOf('async function runFileMode(')), context);
  await context.runFile();
  assert.deepEqual(JSON.parse(JSON.stringify(payload)), { code: '', mode: 'file', argument: '/work/entry.m', history: false });
  assert.equal(saved, 0);
  enabled = true;
  delete shared.engine.source_file_adapter_version;
  payload = null;
  await assert.rejects(() => context.runFile(), /Restart the application/);
  assert.equal(payload, null, 'old backend must not receive an adaptation request');
  shared.engine.source_file_adapter_version = 1;
  shared.active.dirty = true;
  await context.runFile();
  assert.equal(saved, 1);
  assert.equal(payload.adapt_file_literals, true);
  assert.equal(payload.file_hash, 'saved-hash');
  assert.equal(payload.source_context, undefined);
  assert.equal(payload.adapt_editor_literals, undefined);
  assert.equal(entry.sourceContext.document, 'server snapshot');
  assert.equal(entry.original.text, 'server snapshot');
  registry.saveActive = async () => false;
  shared.active.dirty = true;
  payload = null;
  await context.runFile();
  assert.equal(payload, null);

  // The entry-file server snapshot shares exact/historical navigation with editor snapshots.
  class Element {
    constructor(tag, cls = '', text = '') { this.tag = tag; this.className = cls; this.textContent = text; this.children = []; this.dataset = {}; }
    append(child) { child.parent = this; this.children.push(child); }
    querySelector(selector) { return this.children.find((child) => child.className === selector.slice(1)) || null; }
    remove() { this.parent.children.splice(this.parent.children.indexOf(this), 1); }
    setAttribute() {}
    focus() {}
    setSelectionRange(start, end) { this.selection = [start, end]; }
  }
  let modal, selection;
  const document = 'r="x";\nmissing;';
  shared.tabs = [shared.active];
  shared.editor = { state: { doc: { toString: () => shared.active.content } }, dispatch: (value) => selection = value, focus() {} };
  shared.active.content = document;
  Object.assign(registry, { el: (...args) => new Element(...args), switchTab() {}, updateCursor() {}, modal: (_, node) => modal = node });
  Object.assign(context, { t: (key, params = {}) => key.replace(/\{(\w+)\}/g, (_, name) => params[name]), onLanguageChange() {} });
  vm.runInContext(fs.readFileSync(require.resolve('../frontend/source_adapter.js'), 'utf8').replace(/^import .*;\n/gm, '').replace(/export function /g, 'function '), context);
  entry = { job: 'historical', epoch: 7, wrap: new Element('div'), sourceContext: { origin: 'entry-file', document, path: shared.active.path } };
  const state = { job: 'historical', source_adapter: { epoch: 7 }, source_error_locations: [{ job: 'historical', epoch: 7, line: 2, column: 1, line_only: true, start_utf16: 7, end_utf16: 15 }] };
  context.showSourceErrors(entry, state);
  const button = entry.wrap.children[0].children[0];
  assert.equal(button.textContent, 'Original source: line 2');
  button.onclick();
  assert.equal(selection.selection.head, 7);
  shared.active.content = 'dirty replacement';
  button.onclick();
  assert.equal(modal.value, document);
  assert.deepEqual(modal.selection, [7, 15]);
  console.log('SOURCE FILES UNIT PASS: default/persistence, F5 payload, save/hash/cancel, authoritative snapshot and historical dirty navigation.');
})().catch((error) => { console.error(error); process.exit(1); });
