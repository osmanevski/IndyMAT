const assert = require('node:assert/strict');
const utils = require('../frontend/assistant_utils.cjs');
const { splitCodeBlocks, reduceTranscript } = utils;
const settings = require('../frontend/shortcut_registry_utils.cjs');
assert.deepEqual(splitCodeBlocks('text\n```matlab\nx = 42;\n```\nafter'), [{ type: 'text', text: 'text\n' }, { type: 'code', language: 'matlab', text: 'x = 42;\n' }, { type: 'text', text: 'after' }]);
assert.deepEqual(splitCodeBlocks('<img src=x>'), [{ type: 'text', text: '<img src=x>' }]);
assert.equal(splitCodeBlocks('```\npartial')[0].text, 'partial');
assert.equal(splitCodeBlocks('inline ``` is plain')[0].type, 'text');
let messages = reduceTranscript([], { type: 'user', text: 'question', turn: 'a' });
const original = messages;
messages = reduceTranscript(messages, { type: 'text', text: 'one', turn: 'a' });
messages = reduceTranscript(messages, { type: 'text', text: 'two', turn: 'a' });
assert.equal(messages[1].text, 'onetwo');
assert.equal(original.length, 1);
messages = reduceTranscript(messages, { type: 'raw', text: '{new}', turn: 'a' });
assert.equal(messages.at(-1).role, 'raw');
messages = reduceTranscript(messages, { type: 'turn-end', state: 'stopped', turn: 'a' });
assert.equal(messages.at(-1).text, 'stopped');
messages = reduceTranscript(messages, { type: 'text', text: 'next', turn: 'b' });
assert.equal(messages.at(-1).text, 'next');
messages = reduceTranscript(messages, { type: 'text', text: 'x'.repeat(1100000), turn: 'b' });
assert.equal(messages[0].role, 'notice');
assert(messages.at(-1).text.length <= 1000000);
assert.deepEqual(settings.cloneDefaults().assistant, { open: false, width: 340 });
assert.deepEqual(settings.sanitizeSettings({ version: 1, assistant: { open: true, width: 9000 } }).assistant, { open: true, width: 600 });
{
  const labels = (name, value) => utils.reduceTranscript([], { type: 'tool', name, text: typeof value === 'string' ? value : JSON.stringify(value), turn: 't' }).map((message) => [message.label || message.name, message.text]);
  assert.deepEqual(labels('Edit', { file_path: '/a/b/yeni_1.m', added: 5, removed: 0 }), [['Edit file', 'b/yeni_1.m  +5 −0']]);
  assert.deepEqual(labels('Write', { file_path: '/a/b/x.m', content_lines: 47 }), [['Write file', 'b/x.m  +47']]);
  assert.deepEqual(labels('Read', { file_path: '/a/b/x.m' }), [['Read file', 'b/x.m']]);
  assert.deepEqual(labels('command_execution', "/bin/zsh -lc 'cat x.m'"), [['Shell', 'cat x.m']]);
  assert.deepEqual(labels('ToolSearch', { query: 'q' }), []);
  const change = { type: 'file', text: JSON.stringify([{ path: '/a/b/x.m', kind: 'update' }]), turn: 't' };
  const once = utils.reduceTranscript(utils.reduceTranscript([], change), change);
  assert.deepEqual(once.map((message) => [message.label, message.text]), [['Edit file', 'b/x.m']]);
  assert.equal(utils.changesFiles({ name: 'Write' }), true);
  assert.equal(utils.changesFiles({ name: 'Read' }), false);
}
console.log('ASSISTANT UNIT PASS: transcript, code fences, bounded memory, persisted layout.');
// Exercise the real async refresh implementation against read races, preserving
// drafts and save preconditions. No browser or Octave needed for these cases.
(async () => {
  const fs = require('node:fs');
  const source = fs.readFileSync(require.resolve('../frontend/assistant.js'), 'utf8');
  const body = source.slice(source.indexOf('async function refreshAssistantFiles()'), source.indexOf('function renderAssistantDiskMarks()'));
  const clean = { path: '/home/clean.m', content: 'old', saved: 'old', hash: 'old-hash', dirty: false };
  const dirty = { path: '/home/dirty.m', content: 'draft', saved: 'old', hash: 'old-hash', dirty: true };
  const raced = { path: '/home/raced.m', content: 'old', saved: 'old', hash: 'old-hash', dirty: false };
  const saved = { path: '/home/saved.m', content: 'old', saved: 'old', hash: 'old-hash', dirty: false };
  const missing = { path: '/home/missing.m', content: 'old', saved: 'old', hash: 'old-hash', dirty: false };
  const state = { tabs: [clean, dirty, raced, saved, missing], active: clean, editor: { state: { doc: { toString: () => clean.content } }, setState: (next) => { assert.equal(next.doc, 'disk'); } } };
  let refreshed = false;
  const registry = {
    refreshFiles: async () => { refreshed = true; },
    api: async (endpoint) => {
      if (endpoint.includes('missing')) throw new Error('deleted');
      if (endpoint.includes('raced')) { raced.dirty = true; raced.content = 'typing'; }
      if (endpoint.includes('saved')) saved.hash = 'new-save-hash';
      return { content: 'disk', hash: 'disk-hash' };
    },
    makeState: (doc) => ({ doc }), refreshDebugEditor() {}, updateCursor() {}, renderTabs() {}, persistDrafts() {}
  };
  await new Function('shared', 'registry', body + '\nreturn refreshAssistantFiles();')(state, registry);
  assert(refreshed);
  assert.equal(clean.content, 'disk');
  assert.equal(clean.hash, 'disk-hash');
  assert.equal(dirty.content, 'draft');
  assert.equal(dirty.hash, 'old-hash');
  assert.equal(dirty.assistantDiskChanged, true);
  assert.equal(raced.content, 'typing');
  assert.equal(raced.assistantDiskChanged, true);
  assert.equal(saved.hash, 'new-save-hash');
  assert.equal(saved.content, 'old');
  assert.equal(missing.content, 'old');
  assert.equal(missing.assistantDiskChanged, true);
  const base = settings.cloneDefaults();
  const next = settings.cloneDefaults();
  next.assistant.open = true;
  next.assistant.width = 450;
  const patch = settings.settingsPatch(base, next);
  assert.equal(patch.length, 2);
  assert.equal(settings.applyPatch(base, patch).assistant.width, 450);
  console.log('ASSISTANT REFRESH PASS: clean reload, dirty/read-race preservation, concurrent save guard, missing file, settings patches.');
})().catch((error) => { console.error(error); process.exitCode = 1; });
