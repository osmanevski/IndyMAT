const assert = require('node:assert/strict');
const fs = require('node:fs');
const utils = require('../frontend/assistant_utils.cjs');
const settings = require('../frontend/shortcut_registry_utils.cjs');
const event = { type: 'approval', id: 'one', turn: 'turn', tool: 'Edit', summary: 'x.m', detail: { text: '--- x.m\n+++ x.m\n-old\n+new\n <script>alert(1)</script>', kind: 'diff', truncated: true } };
let messages = utils.reduceTranscript([], event);
assert.equal(messages[0].label, 'Edit file');
assert.equal(messages[0].summary, 'x.m');
assert.equal(utils.pendingApprovals(messages).length, 1);
assert.equal(utils.reduceTranscript(messages, event).length, 1);
const lines = utils.approvalLines(event.detail.text);
assert.deepEqual(lines.map((line) => line.kind), ['context', 'context', 'removed', 'added', 'context']);
assert.equal(lines[4].text, ' <script>alert(1)</script>');
assert.equal(utils.reduceTranscript(messages, { type: 'approval-resolved', id: 'one', turn: 'wrong', decision: 'allow' })[0].decision, null);
assert.equal(utils.reduceTranscript(messages, { type: 'approval-resolved', id: 'wrong', turn: 'turn', decision: 'allow' })[0].decision, null);
messages = utils.reduceTranscript(messages, { type: 'approval-resolved', id: 'one', turn: 'turn', decision: 'allow-conversation' });
assert.equal(messages[0].decision, 'allow-conversation');
assert.equal(messages[0].text, '');
assert.deepEqual(messages[0].detail, {});
assert.equal(utils.pendingApprovals(messages).length, 0);
assert.equal(utils.reduceTranscript(messages, { type: 'approval-resolved', id: 'one', turn: 'turn', decision: 'deny' })[0].decision, 'allow-conversation');
messages = utils.reduceTranscript([], event);
messages = utils.reduceTranscript(messages, { type: 'turn-end', turn: 'turn', state: 'stopped' });
assert.equal(messages[0].decision, 'deny');
assert.equal(messages[1].role, 'state');
assert.equal(utils.approvalActivity({ ...event, tool: 'Bash' }).label, 'Shell');
assert.equal(utils.approvalActivity({ ...event, tool: 'Shell' }).label, 'Shell');
const steering = utils.reduceTranscript([], { type: 'user', text: 'change direction', turn: 'turn', steered: true });
assert.equal(steering[0].steered, true);
assert.equal(utils.approvalActivity({ ...event, tool: 'Unknown' }).label, 'Tool activity');
for (const mode of ['ask', 'read-only', 'edit']) {
  const sanitized = settings.sanitizeSettings({ version: 1, assistant: { mode } });
  assert.equal(sanitized.assistant.mode, mode);
  const base = settings.cloneDefaults();
  const patch = settings.settingsPatch(base, sanitized);
  assert.equal(settings.sanitizeSettings(settings.applyPatch(base, patch)).assistant.mode, mode);
}
assert.equal(settings.sanitizeSettings({ version: 1, assistant: { mode: 'bypass' } }).assistant.mode, undefined);
const saved = settings.sanitizeSettings({ version: 1, assistant: { mode: 'edit' } });
assert.equal(settings.applyPatch(saved, settings.settingsPatch(saved, settings.cloneDefaults())).assistant.mode, 'ask');
let flooded = utils.reduceTranscript([], event);
for (let index = 0; index < 1100; index++) flooded = utils.reduceTranscript(flooded, { type: 'raw', text: 'output', turn: 'turn' });
assert.equal(utils.pendingApprovals(flooded).length, 1);
assert(flooded.length <= 1001);
// Exercise production card rendering using a tiny plain-text DOM; model HTML stays text.
class Element {
  constructor(tag, cls = '', text = '') { this.tag = tag; this.className = cls; this.textContent = text; this.children = []; this.dataset = {}; this.attributes = {}; }
  append(...nodes) { this.children.push(...nodes); }
  setAttribute(name, value) { this.attributes[name] = value; }
}
const source = fs.readFileSync(require.resolve('../frontend/assistant.js'), 'utf8');
const body = source.slice(source.indexOf('function approvalLabel('), source.indexOf('function renderTranscript()'));
const el = (tag, cls, text) => new Element(tag, cls, text);
const t = (text, values = {}) => text.replace(/\{(\w+)\}/g, (_, key) => values[key] || '');
const render = new Function('el', 't', 'utils', 'registry', 'model', body + '\nreturn approvalCard;')(el, t, utils, {}, {});
const card = render(utils.approvalActivity(event), { id: 'session' });
assert.equal(card.attributes.role, 'group');
assert.equal(card.attributes['aria-label'], 'Approval required');
const diff = card.children.find((node) => node.tag === 'pre');
assert.equal(diff.children[4].textContent, ' <script>alert(1)</script>\n');
assert.equal(diff.children[2].className, 'assistant-diff-removed');
const actions = card.children.find((node) => node.className === 'assistant-approval-actions');
assert.deepEqual(actions.children.map((node) => node.textContent), ['Allow', 'Allow for this conversation', 'Deny']);
assert(actions.children.every((node) => node.tag === 'button' && node.type === 'button'));
const resolved = render({ ...utils.approvalActivity(event), decision: 'allow' }, { id: 'session' });
assert.equal(resolved.textContent, 'Allowed: Edit file · x.m');
assert.equal(resolved.children.length, 0);
console.log('ASSISTANT APPROVALS UNIT PASS: reducer, diff lines, safe card rendering, resolved cards, mode persistence.');
