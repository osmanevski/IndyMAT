const assert = require('node:assert/strict');
const vm = require('node:vm');
const { buildSync } = require('esbuild');

const features = ['symbols', 'editor_commands', 'file_ops', 'workspace', 'command_window', 'bootstrap'];
function harness(dispatcherFirst = false) {
  const imports = features.map(name => `import './frontend/${name}.js';`);
  imports.splice(dispatcherFirst ? 0 : imports.length, 0, "import './frontend/shortcut_registry.js';");
  const source = `import registry from './frontend/registry.js';
    import shared from './frontend/state.js';${imports.join('\n')}
    globalThis.testModules = {registry, shared};`;
  const built = buildSync({ stdin: { contents: source, resolveDir: process.cwd() }, bundle: true, write: false, platform: 'node', format: 'cjs', packages: 'external' });
  const nodes = new Map();
  const listeners = [];
  let modalOpen = false;
  function target(scope, editable = false) {
    const node = {
      value: '', selectionStart: 0, selectionEnd: 0, scrollHeight: 22,
      style: {}, dataset: {}, children: [], events: {}, classList: { toggle() {} },
      closest(selector) {
        if (selector === '#editor') return scope === 'editor' ? this : null;
        if (selector === '#left-panel') return scope === 'file-list' ? this : null;
        if (selector === '#right-panel') return scope === 'workspace' ? this : null;
        if (selector === '.file-button') return scope === 'file-list' && !editable ? this : null;
        if (selector.includes('input, textarea')) return editable ? this : null;
        if (selector === 'tr') return scope === 'workspace' && !editable ? this : null;
        return null;
      },
      setRangeText(text, start, end) {
        this.value = this.value.slice(0, start) + text + this.value.slice(end);
        this.selectionStart = this.selectionEnd = start + text.length;
      },
      addEventListener(type, fn) { this.events[type] = fn; }, removeEventListener() {}, focus() {}, select() {},
      setAttribute() {}, append(...items) { this.children.push(...items); }, replaceChildren(...items) { this.children = items; }
    };
    return node;
  }
  const command = target('command-line', true);
  nodes.set('#command', command);
  const document = {
    querySelector: selector => selector === 'dialog[open]' ? (modalOpen ? {} : null) : nodes.get(selector),
    querySelectorAll: () => [],
    addEventListener() {}
  };
  const context = { console, navigator: { platform: 'MacIntel' }, document,
    window: { addEventListener(type, fn, capture) { listeners.push({ type, fn, capture }); } },
    MutationObserver: class { observe() {} }, confirm: () => true,
    require: () => ({ foldAll() {}, unfoldAll() {}, EditorView: {} }) };
  vm.runInNewContext(built.outputFiles[0].text, context);
  const { registry: r, shared: s } = context.testModules;
  const pending = [];
  Object.assign(r, {
    $: selector => {
      if (!nodes.has(selector)) nodes.set(selector, target('global'));
      return nodes.get(selector);
    },
    safe: fn => {
      const promise = Promise.resolve().then(fn);
      pending.push(promise);
      return promise;
    },
    on() {}, el: () => target('global'), toast() {},
    renderFiles() {}, renderTabs() {}, persistDrafts() {}, relocateBreakpointPaths() {},
    refreshFiles: async () => {}, words: ['plot', 'poly']
  });
  s.settings = { shortcuts: {} };
  s.engine = { status: 'idle' };
  s.editor = { state: { selection: { ranges: [{ empty: true }] } } };
  r.setupShortcutRegistry();
  r.setupShortcutRegistry();
  assert.equal(listeners.length, 1, 'dispatcher installed twice');
  const dispatch = event => {
    listeners[0].fn(event);
    if (!event.stopped) event.target.onkeydown?.(event);
    return event;
  };
  return { r, s, target, command, dispatch, nodes,
    modal(open) { modalOpen = open; },
    async flush() { while (pending.length) await Promise.all(pending.splice(0)); }
  };
}

function keyboard(binding, target, layout = 'US', namedOnly = false) {
  const parts = binding.split('+');
  const code = parts.pop();
  const names = { Space: ' ', Slash: '/', Comma: ',', Period: '.', Equal: '=', Minus: '-' };
  let key = names[code] || (code.startsWith('Key') ? code.slice(3).toLowerCase() : code.startsWith('Digit') ? code.slice(5) : code);
  if (layout === 'TR' && code === 'KeyI') key = 'ı';
  return { key, code: namedOnly ? '' : code, target, currentTarget: null,
    metaKey: parts.includes('Mod'), ctrlKey: parts.includes('Ctrl'), altKey: parts.includes('Alt'), shiftKey: parts.includes('Shift'),
    isComposing: false, defaultPrevented: false, stopped: false,
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; }
  };
}

function settingsHarness(storage, locks, includeEditor = false) {
  const source = `import registry from './frontend/registry.js';
    import shared from './frontend/state.js';
    import './frontend/settings.js';
    ${includeEditor ? "import './frontend/editor.js';" : ''}
    globalThis.testModules = {registry, shared};`;
  const built = buildSync({ stdin: { contents: source, resolveDir: process.cwd() }, bundle: true, write: false, platform: 'node', format: 'cjs', packages: 'external' });
  const listeners = {};
  const style = () => ({ setProperty(key, value) { this[key] = value; }, removeProperty(key) { delete this[key]; }, getPropertyValue(key) { return this[key] || ''; } });
  const root = { style: style() };
  const consolePanel = { style: style(), getBoundingClientRect: () => ({ width: 800 }) };
  const nodes = { '.console-panel': consolePanel, '.bottom-panels': { getBoundingClientRect: () => ({ width: 800 }) } };
  const classes = new Set();
  const context = { console, require, innerWidth: 1512, localStorage: storage, navigator: { locks }, queueMicrotask,
    window: { addEventListener(type, fn) { listeners[type] = fn; } },
    document: { documentElement: root, body: { classList: { toggle(name, enabled) { if (enabled) classes.add(name); else classes.delete(name); } } }, addEventListener() {} },
    getComputedStyle: node => node.style };
  vm.runInNewContext(built.outputFiles[0].text, context);
  const { registry: r, shared: s } = context.testModules;
  const notices = [];
  Object.assign(r, { $: selector => nodes[selector], on() {}, toast(message) { notices.push(message); }, shortcutDefinitions: [], shortcutPlatformMac: true });
  r.setupSettings();
  return { r, s, root, consolePanel, classes, notices,
    resize(width) { context.innerWidth = width; listeners.resize(); },
    storageEvent(key = 'mf-settings-v1') { listeners.storage({ storageArea: storage, key }); }
  };
}

(async () => {
  const h = harness();
  const early = harness(true);
  assert.deepEqual(Array.from(h.r.shortcutDefinitions, d => d.id).sort(), Array.from(early.r.shortcutDefinitions, d => d.id).sort());
  assert.equal(h.r.pendingShortcuts.length, 0);
  assert.equal(h.r.shortcutDefinitions.length, 33, 'lost feature registrations');
  const { r, s, command } = h;
  const scopes = ['editor', 'command-line', 'file-list', 'workspace', 'global'];
  const targets = Object.fromEntries(scopes.map(scope => [scope, scope === 'command-line' ? command : h.target(scope)]));
  targets['file-list'].dataset.path = '/home/test/focused.m';
  targets.workspace._workspaceShortcutContext = { variable: { name: 'x' }, visible: [{ name: 'x' }] };
  s.files = [{ path: '/home/test/focused.m', name: 'focused.m' }];
  const originals = new Map(r.shortcutDefinitions.map(d => [d.id, d.command]));
  let calls = [];
  for (const d of r.shortcutDefinitions) d.command = () => { calls.push(d.id); };
  let checked = 0;
  // Exercise the actual capture listener and every real registration/guard,
  // including disjoint scopes and US / Turkish-Q / named-key events.
  for (const definition of r.shortcutDefinitions) {
    for (const binding of definition.bindings) {
      for (const scope of scopes) {
        for (const layout of ['US', 'TR']) {
          for (const namedOnly of [false, true]) {
            command.value = '';
            command.selectionStart = command.selectionEnd = 0;
            s.commandRecall = { matches: ['plot(1)'], index: 1, draft: '' };
            s.busy = true;
            calls = [];
            const event = keyboard(binding, targets[scope], layout, namedOnly);
            const expected = r.shortcutDefinitions.find(d => (d.scope === 'global' || d.scope === scope) && d.bindings.some(binding => r.shortcutUtils.matchesBinding(d, binding, event, true)) && (!d.when || d.when(event)));
            if ((definition.scope === scope || definition.scope === 'global') && (definition.id !== 'global.stop' || ['editor', 'command-line'].includes(scope))) {
              assert.equal(expected?.id, definition.id, `default lost: ${definition.id}/${scope}/${layout}/${namedOnly}`);
            }
            h.dispatch(event);
            assert.deepEqual(calls, expected ? [expected.id] : [], `${definition.id}/${binding}/${scope}/${layout}/${namedOnly}`);
            if (expected) assert(event.defaultPrevented && event.stopped);
            else assert.equal(event.stopped, false);
            checked++;
          }
        }
      }
    }
  }
  // Finding 1: independent parent predicates (not the registry matcher), over
  // every modifier subset and physical US / Turkish-Q key in each default.
  function parentAccepts(d, e) {
    const primary = e.metaKey || e.ctrlKey;
    const key = e.key.toLowerCase();
    switch (d.id) {
      case 'global.save': return primary && key === 's';
      case 'global.quick-open': return primary && key === 'p';
      case 'global.font-increase': return primary && ['+', '='].includes(e.key);
      case 'global.font-decrease': return primary && e.key === '-';
      case 'global.font-reset': return primary && e.key === '0';
      case 'global.documentation': return e.key === 'F1';
      case 'global.run': return e.key === 'F5' && !(primary || e.altKey || e.shiftKey);
      case 'global.run-selection': return e.key === 'F9' && !(primary || e.altKey || e.shiftKey);
      case 'global.stop': return e.ctrlKey && !e.metaKey && key === 'c';
      case 'command.complete': return e.key === 'Tab';
      case 'command.previous': return e.key === 'ArrowUp';
      case 'command.next': return e.key === 'ArrowDown';
      case 'command.submit': return e.key === 'Enter' && !e.shiftKey;
      case 'command.newline': return e.key === 'Enter' && e.shiftKey;
      case 'files.rename': return e.key === 'Enter';
      case 'files.trash': return e.key === 'Backspace' && e.metaKey;
      case 'workspace.move-up': return e.key === 'ArrowUp';
      case 'workspace.move-down': return e.key === 'ArrowDown';
      case 'workspace.toggle-selection': return [' ', 'Spacebar'].includes(e.key);
      case 'workspace.select-all': return primary && key === 'a';
      case 'workspace.delete': return ['Delete', 'Backspace'].includes(e.key);
      case 'workspace.rename': return e.key === 'F2';
      case 'workspace.open': return e.key === 'Enter';
      default: return d.bindings.includes(r.shortcutUtils.canonicalEvent(e, true, { ignoreShift: d.ignoreShift }));
    }
  }
  let differential = 0;
  for (const definition of r.shortcutDefinitions) {
    for (const binding of definition.bindings) {
      for (const layout of ['US', 'TR']) {
        for (let mask = 0; mask < 16; mask++) {
          for (const scope of scopes) {
            const e = keyboard(binding, targets[scope], layout);
            Object.assign(e, { metaKey: !!(mask & 1), ctrlKey: !!(mask & 2), altKey: !!(mask & 4), shiftKey: !!(mask & 8) });
            const punctuation = layout === 'TR' ? { Equal: ['-', '_'], Minus: ['*', '?'], Digit0: ['0', '='], Slash: ['.', ':'], Comma: ['ö', 'Ö'], Period: ['ç', 'Ç'] } : { Equal: ['=', '+'], Minus: ['-', '_'], Digit0: ['0', ')'], Slash: ['/', '?'], Comma: [',', '<'], Period: ['.', '>'] };
            if (punctuation[e.code]) e.key = punctuation[e.code][Number(e.shiftKey)];
            else if (e.shiftKey && e.key.length === 1) e.key = e.key.toUpperCase();
            command.value = '';
            command.selectionStart = command.selectionEnd = 0;
            s.commandRecall = { matches: ['x'], index: 1, draft: '' };
            s.busy = true;
            calls = [];
            const expected = r.shortcutDefinitions.find(d => (d.scope === 'global' || d.scope === scope) && parentAccepts(d, e) && (!d.when || d.when(e)));
            h.dispatch(e);
            assert.deepEqual(calls, expected ? [expected.id] : [], `parent mismatch: ${binding}/${layout}/${mask}/${scope}`);
            assert.equal(e.stopped, !!expected);
            differential++;
          }
        }
      }
    }
  }
  // Finding 2/7: even a direct hostile override cannot take over text entry.
  for (const binding of ['KeyA', 'Digit2', 'Enter', 'Tab', 'Escape', 'ArrowLeft', 'Alt+Digit2', 'Alt+BracketLeft', 'Mod+Tab', 'Mod+Comma']) {
    s.settings.shortcuts['global.save'] = [{ binding }];
    calls = [];
    const e = h.dispatch(keyboard(binding, targets.editor));
    assert.deepEqual(calls, [], `unsafe override ran: ${binding}`);
    assert.equal(e.defaultPrevented, false);
    assert.equal(e.stopped, false);
  }
  delete s.settings.shortcuts['global.save'];
  for (const key of ['Dead', 'Process', 'Unidentified']) {
    const e = keyboard('Mod+KeyS', targets.editor);
    e.key = key;
    calls = [];
    h.dispatch(e);
    assert.deepEqual(calls, []);
    assert.equal(e.defaultPrevented, false);
  }
  // Cached bindings are reused rather than allocated for every key event.
  const saveDefinition = r.shortcutDefinitions.find(d => d.id === 'global.save');
  s.settings.shortcuts['global.save'] = [{ binding: 'Mod+Shift+KeyB' }];
  assert.equal(r.activeShortcutBindings(saveDefinition), r.activeShortcutBindings(saveDefinition));
  const altGraph = keyboard('Mod+Shift+KeyB', targets.editor);
  altGraph.getModifierState = name => name === 'AltGraph';
  calls = [];
  h.dispatch(altGraph);
  assert.deepEqual(calls, []);
  assert.equal(altGraph.defaultPrevented, false);
  delete s.settings.shortcuts['global.save'];
  for (const d of r.shortcutDefinitions) d.command = originals.get(d.id);

  // Real file command: focus alone must override absent, stale and other valid
  // selections; never trash the old selection or rely on a click to select.
  const apiCalls = [];
  r.api = async (route, payload) => {
    apiCalls.push({ route, ...payload });
    return { path: '/trash/focused.m', directory: false };
  };
  for (const selected of ['', '/home/test/old-folder', '/home/test/other.m']) {
    for (const namedOnly of [false, true]) {
      s.files = [{ path: '/home/test/focused.m', name: 'focused.m' }, { path: '/home/test/other.m', name: 'other.m' }];
      s.tabs = [{ path: '/home/test/focused.m', dirty: true, content: 'dirty_value = 42;', saved: 'old', hash: 'old' }];
      r.selectFile(selected);
      apiCalls.length = 0;
      h.dispatch(keyboard('Mod+Backspace', targets['file-list'], 'US', namedOnly));
      await h.flush();
      assert.deepEqual(apiCalls.map(x => [x.operation, x.source]), [['inspect', '/home/test/focused.m'], ['trash', '/home/test/focused.m']]);
      assert.equal(s.tabs[0].content, 'dirty_value = 42;');
      assert.equal(s.tabs[0].dirty, true);
    }
  }
  // Enter follows the same focused-row selection rule before opening rename.
  h.modal(true);
  apiCalls.length = 0;
  const blockedTrash = h.dispatch(keyboard('Mod+Backspace', targets['file-list']));
  assert.equal(blockedTrash.defaultPrevented, false);
  assert.equal(blockedTrash.stopped, false);
  assert.equal(apiCalls.length, 0);
  h.modal(false);
  r.modal = () => r.$('#modal').events.close();
  r.selectFile('/home/test/other.m');
  apiCalls.length = 0;
  h.dispatch(keyboard('Enter', targets['file-list']));
  await Promise.resolve();
  assert.equal(r.getSelectedFile(), '/home/test/focused.m');
  assert.equal(apiCalls[0].operation, 'inspect');
  await h.flush();

  // Workspace inline input and Variables dialog grid own Enter/Escape/F2.
  const inline = h.target('workspace', true);
  const gridInput = h.target('workspace', true);
  for (const [target, open] of [[inline, false], [gridInput, true], [targets.workspace, true]]) {
    h.modal(open);
    for (const binding of ['Enter', 'Escape', 'F2', 'Tab', 'ArrowUp', 'ArrowDown', 'Backspace']) {
      let local = 0;
      target.onkeydown = () => local++;
      const event = h.dispatch(keyboard(binding, target));
      assert.equal(event.defaultPrevented, false, `swallowed ${binding} in local editor`);
      assert.equal(event.stopped, false);
      assert.equal(local, 1);
    }
    delete target.onkeydown;
  }
  h.modal(false);
  r.selectFile('');
  const noRow = h.target('global');
  noRow.closest = selector => selector === '#left-panel' ? noRow : null;
  const declined = h.dispatch(keyboard('Mod+Backspace', noRow));
  assert.equal(declined.defaultPrevented, false, 'declined file action consumed a key');
  assert.equal(declined.stopped, false);
  r.selectFile('/home/test/focused.m');
  const selectedButNotFocused = h.dispatch(keyboard('Mod+Backspace', noRow));
  assert.equal(selectedButNotFocused.defaultPrevented, false, 'parent file-row shortcut ran outside a row');
  assert.equal(selectedButNotFocused.stopped, false);

  // Invoke the real workspace commands through capture, where currentTarget
  // is the window rather than the row (the old per-row listener's target).
  let opens = 0;
  r.requireIdle = () => {};
  r.canOpenVariable = () => true;
  r.openVariable = async name => { assert.equal(name, 'x'); opens++; };
  s.engine = { status: 'idle', epoch: 1 };
  h.dispatch(keyboard('Enter', targets.workspace));
  await h.flush();
  assert.equal(opens, 1);
  const cell = h.target('global');
  targets.workspace.children = [cell];
  const makeElement = r.el;
  r.el = tag => tag === 'input' ? h.target('workspace', true) : makeElement(tag);
  const rename = keyboard('F2', targets.workspace);
  rename.currentTarget = { addEventListener() {} };
  h.dispatch(rename);
  assert.equal(cell.children.length, 1);
  const renameInput = cell.children[0];
  assert.equal(renameInput.value, 'x');
  const cancel = keyboard('Escape', renameInput);
  assert.equal(r.handleShortcut(cancel), false);
  assert.equal(cancel.defaultPrevented, false);
  assert.equal(cancel.stopped, false);
  renameInput.onkeydown(cancel);
  assert.equal(opens, 1, 'inline cancel opened variable details');
  r.el = makeElement;

  // Real command-window semantics: a single handler performs submit, newline,
  // completion and recall. Middle-line arrows and Escape reach the textarea.
  r.setupCommandWindow();
  assert.equal(command.onkeydown, null);
  let submissions = 0;
  r.$('#command-form').requestSubmit = () => submissions++;
  function setInput(value, position = value.length) {
    command.value = value;
    command.selectionStart = command.selectionEnd = position;
    s.commandRecall = null;
    s.commandCompletion = null;
  }
  s.busy = false;
  s.engine = { status: 'idle' };
  setInput('x=1;');
  h.dispatch(keyboard('Enter', command));
  assert.equal(submissions, 1);
  setInput('x=1;');
  h.dispatch(keyboard('Shift+Enter', command));
  assert.equal(command.value, 'x=1;\n');
  assert.equal(submissions, 1);
  setInput('if true');
  h.dispatch(keyboard('Enter', command));
  assert.equal(command.value, 'if true\n');
  assert.equal(submissions, 1);
  for (const state of [{ status: 'paused' }, { status: 'running' }, { status: 'idle', waiting_input: true }]) {
    s.engine = state;
    setInput('x=1;');
    const before = submissions;
    h.dispatch(keyboard('Shift+Enter', command));
    assert.equal(submissions, before + 1, 'Shift+Enter diverged from existing commandKeydown');
    assert.equal(command.value, 'x=1;');
  }
  s.engine = { status: 'idle' };
  setInput('pl');
  h.dispatch(keyboard('Tab', command));
  assert.equal(command.value, 'plot');
  assert.equal(s.commandCompletion.index, 0);
  s.commands = ['plot(1)', 'plot(2)'];
  setInput('pl');
  h.dispatch(keyboard('ArrowUp', command));
  assert.equal(command.value, 'plot(2)');
  assert.equal(s.commandRecall.index, 1);
  h.dispatch(keyboard('ArrowDown', command));
  assert.equal(command.value, 'pl');
  setInput('one\ntwo\nthree', 6);
  for (const binding of ['ArrowUp', 'ArrowDown', 'Escape']) {
    const event = h.dispatch(keyboard(binding, command));
    assert.equal(event.defaultPrevented, false);
    assert.equal(event.stopped, false);
  }
  setInput('x=1;');
  const composing = keyboard('Enter', command);
  composing.isComposing = true;
  h.dispatch(composing);
  assert.equal(composing.defaultPrevented, false);
  const beforeRebind = submissions;
  s.settings.shortcuts['command.submit'] = [{ binding: 'Mod+Shift+KeyB' }];
  h.dispatch(keyboard('Enter', command));
  assert.equal(submissions, beforeRebind);
  h.dispatch(keyboard('Mod+Shift+KeyB', command));
  assert.equal(submissions, beforeRebind + 1);

  // Findings 4/5/6: invoke actual settings lifecycle and storage listener.
  const utils = require('../frontend/shortcut_registry_utils.cjs');
  const values = { 'mf-settings-v1': JSON.stringify(utils.cloneDefaults()) };
  const storage = { getItem: key => values[key] ?? null, setItem: (key, value) => { values[key] = value; }, removeItem: key => { delete values[key]; } };
  let lockQueue = Promise.resolve();
  const locks = { request(name, fn) { assert.equal(name, 'mf-settings-v1'); const next = lockQueue.then(fn); lockQueue = next.catch(() => {}); return next; } };
  const tabA = settingsHarness(storage, locks);
  const tabB = settingsHarness(storage, locks);
  tabA.s.settings.preferences.theme = 'light';
  tabB.s.settings.preferences.editorFontSize = 19;
  await Promise.all([tabA.r.saveSettings(), tabB.r.saveSettings()]);
  tabA.storageEvent();
  tabB.storageEvent();
  for (const tab of [tabA, tabB]) {
    assert.equal(tab.s.settings.preferences.theme, 'light');
    assert.equal(tab.s.settings.preferences.editorFontSize, 19);
    assert.equal(tab.classes.has('dark'), false);
    assert.equal(tab.root.style['--editor-font-size'], '19px');
  }
  // Delayed storage events must retain edits already queued in this tab.
  tabA.s.settings.preferences.consoleFontSize = 18;
  const pending = tabA.r.saveSettings();
  tabA.storageEvent();
  assert.equal(tabA.s.settings.preferences.consoleFontSize, 18);
  await pending;
  tabB.storageEvent();
  assert.equal(tabB.root.style['--console-font-size'], '18px');
  tabA.s.settings.layout.left = 360;
  tabA.s.settings.layout.right = 500;
  await tabA.r.saveSettings();
  for (const width of [901, 1000, 1150, 1151, 1160]) {
    tabA.resize(width);
    assert.equal(tabA.root.style.getPropertyValue('--left'), '', `desktop override at ${width}`);
    assert.equal(tabA.root.style.getPropertyValue('--right'), '');
    tabA.r.persistLayout();
    assert.equal(JSON.parse(values['mf-settings-v1']).layout.left, 360);
    assert.equal(JSON.parse(values['mf-settings-v1']).layout.right, 500);
  }
  tabA.resize(1512);
  assert.equal(tabA.root.style['--left'], '360px');
  assert.equal(tabA.root.style['--right'], '500px');
  tabA.s.settings.panels.figures = false;
  tabA.r.applySettings();
  await tabA.r.saveSettings();
  assert.equal(tabA.consolePanel.style.getPropertyValue('flex'), '');
  tabA.r.persistLayout();
  await tabA.r.saveSettings();
  assert.equal(JSON.parse(values['mf-settings-v1']).layout.consoleWidth, 50, 'hidden plot overwrote split');
  tabA.s.settings.panels.figures = true;
  tabA.r.applySettings();
  await tabA.r.saveSettings();
  assert.equal(tabA.consolePanel.style.flex, '0 0 50%');
  const unavailable = settingsHarness({ ...storage, setItem() { throw new Error('full'); } }, locks);
  unavailable.s.settings.preferences.editorFontSize = 21;
  unavailable.r.applySettings();
  await unavailable.r.saveSettings();
  unavailable.s.settings.preferences.consoleFontSize = 20;
  unavailable.r.applySettings();
  await unavailable.r.saveSettings();
  unavailable.storageEvent();
  assert.equal(unavailable.s.settings.preferences.editorFontSize, 21);
  assert.equal(unavailable.s.settings.preferences.consoleFontSize, 20);
  assert.equal(unavailable.notices.length, 1);
  // Compatibility gate: run the real editor font/theme commands while locked
  // commits are deliberately pending. Every mirror must change synchronously.
  const mirrorValues = { 'mf-settings-v1': JSON.stringify(utils.cloneDefaults()) };
  const mirrorStorage = { getItem: key => mirrorValues[key] ?? null, setItem: (key, value) => { mirrorValues[key] = value; } };
  let release;
  const blocked = new Promise(resolve => { release = resolve; });
  const mirror = settingsHarness(mirrorStorage, { request: (name, fn) => blocked.then(fn) }, true);
  mirror.s.editor = { dispatch() {} };
  assert.equal(mirrorValues['mf-theme'], 'dark', 'ui.cjs:36 startup theme mirror');
  assert.equal(mirrorValues['mf-editor-font-size'], '13');
  mirror.r.changeEditorFont(1);
  assert.equal(mirrorValues['mf-editor-font-size'], '14', 'ui.cjs:110 font mirror before commit');
  mirror.r.applyTheme(false);
  assert.equal(mirrorValues['mf-theme'], 'light', 'ui.cjs:123 light theme mirror before commit');
  mirror.r.applyTheme(true);
  assert.equal(mirrorValues['mf-theme'], 'dark', 'ui.cjs:124 dark theme mirror before commit');
  mirror.r.updateSetting('preferences', 'editorFontSize', 17);
  assert.equal(mirrorValues['mf-editor-font-size'], '17', 'settings dialog font mirror before commit');
  assert.equal(JSON.parse(mirrorValues['mf-settings-v1']).preferences.editorFontSize, 13, 'test did not hold the async writer');
  release();
  await mirror.r.saveSettings();
  assert.equal(JSON.parse(mirrorValues['mf-settings-v1']).preferences.editorFontSize, 17);
  mirrorValues['mf-theme'] = 'light';
  mirrorValues['mf-editor-font-size'] = '24';
  mirror.storageEvent('mf-theme');
  mirror.storageEvent('mf-editor-font-size');
  assert.equal(mirror.s.settings.preferences.theme, 'dark');
  assert.equal(mirror.s.settings.preferences.editorFontSize, 17);
  const reloadedMirror = settingsHarness(mirrorStorage, locks);
  assert.equal(reloadedMirror.s.settings.preferences.theme, 'dark', 'startup mirror overrode authoritative theme');
  assert.equal(reloadedMirror.s.settings.preferences.editorFontSize, 17, 'startup mirror overrode authoritative font');
  console.log(`SETTINGS DISPATCH PASS: ${checked} default events, ${differential} parent differential cases; unsafe overrides, queued imports, focused file operations, inline/modal isolation, command-window single dispatch.`);
})().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
