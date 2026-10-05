const assert = require('node:assert/strict');
const fs = require('node:fs');
const utils = require('../frontend/assistant_utils.cjs');
const settings = require('../frontend/shortcut_registry_utils.cjs');
for (const value of ['none', 'inspect', 'run']) {
  const sanitized = settings.sanitizeSettings({ version: 1, assistant: { sessionAccess: value } });
  assert.equal(sanitized.assistant.sessionAccess, value);
  const base = settings.cloneDefaults();
  const patch = settings.settingsPatch(base, sanitized);
  assert.equal(settings.sanitizeSettings(settings.applyPatch(base, patch)).assistant.sessionAccess, value);
}
assert.equal(settings.sanitizeSettings({ version: 1, assistant: { sessionAccess: 'bypass' } }).assistant.sessionAccess, undefined);
const granted = settings.sanitizeSettings({ version: 1, assistant: { sessionAccess: 'run' } });
const reset = settings.applyPatch(granted, settings.settingsPatch(granted, settings.cloneDefaults()));
assert.equal(reset.assistant.sessionAccess, 'none');
let transcript = utils.reduceTranscript([], { type: 'tool', name: 'mcp__indymat__run_code', text: JSON.stringify({ code: 'x=17;' }), turn: 'one' });
assert.equal(transcript[0].name, 'indymat · run_code');
assert.equal(transcript[0].text, 'x=17;');
transcript = utils.reduceTranscript(transcript, { type: 'tool', name: 'indymat · run_code', text: 'x=17;', turn: 'one' });
assert.equal(transcript.length, 1);
// Exercise actual browser-state ingestion without a browser. A completed job
// missed between polls must still have its code and output in Command Window.
const source = fs.readFileSync(require.resolve('../frontend/assistant.js'), 'utf8');
const body = source.slice(source.indexOf('function syncAssistantJobs('), source.indexOf('function setupAssistant()'));
const normalOutput = { out: { textContent: 'user output' }, err: {}, time: {} };
const shared = { activeOutput: normalOutput, lastJob: 'user-job' };
const entries = [];
const registry = { addConsole: (label) => {
  const entry = { label, out: {}, err: {}, time: {} };
  entries.push(entry);
  shared.activeOutput = entry;
  return entry;
} };
const sync = new Function('shared', 'registry', 'sessionJobs', body + '\nreturn syncAssistantJobs;')(shared, registry, new Map());
const job = { job: 'bridge-job', epoch: 1, code: 'x=17;', provider: 'claude', output: '17', error: '', elapsed: .25, status: 'idle' };
sync({ job: 'user-job', assistant_jobs: [job] });
assert.equal(entries[0].label, '% Assistant (Claude)\nx=17;');
assert.equal(entries[0].out.textContent, '17');
assert.equal(shared.activeOutput, normalOutput);
assert.equal(shared.lastJob, 'user-job');
sync({ job: 'bridge-job', assistant_jobs: [job] });
assert.equal(entries.length, 1);
assert.equal(shared.activeOutput, entries[0]);
assert.equal(shared.lastJob, 'bridge-job');
shared.lastJob = 'new-user-job';
shared.activeOutput = normalOutput;
sync({ job: 'bridge-job', assistant_jobs: [job] }, 'previous-job');
assert.equal(shared.lastJob, 'new-user-job');
assert.equal(shared.activeOutput, normalOutput);
console.log('ASSISTANT BRIDGE UNIT PASS: settings, tool lines, deduplication, missed-job console output and normal-output preservation.');
