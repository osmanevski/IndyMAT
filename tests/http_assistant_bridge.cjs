const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { assistantServer, delay } = require('./assistant_harness.cjs');
(async () => {
  const server = await assistantServer();
  const directory = path.dirname(server.workspace);
  const wait = async (session) => {
    for (let i = 0; i < 600; i++) {
      const result = await (await server.request('assistant/events?session=' + session)).json();
      if (!result.running) return result;
      await delay(30);
    }
    throw new Error('Bridge fixture timeout');
  };
  const bridge = (token, endpoint, body, headers = {}) => fetch(server.base + 'api/bridge/' + endpoint, { method: 'POST', headers: { 'X-IndyMAT-Bridge': token, 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(body) });
  try {
    assert.equal((await bridge(server.token, 'tools', {}, { 'X-MF-Token': server.token })).status, 403);
    for (const provider of ['claude', 'codex']) {
      const started = await server.request('assistant/start', { provider, session_access: 'run', prompt: 'bridge-run' });
      assert.equal(started.status, 202);
      const { session } = await started.json();
      const credentialFile = path.join(directory, '.matlab-free', 'bridge', session + '.json');
      const configFile = path.join(directory, '.matlab-free', 'bridge', session + '.mcp.json');
      const credential = JSON.parse(fs.readFileSync(credentialFile, 'utf8'));
      assert.notEqual(credential.token, server.token);
      assert.equal(fs.statSync(credentialFile).mode & 0o777, 0o600);
      assert.equal(fs.statSync(configFile).mode & 0o777, 0o600);
      assert.equal(fs.statSync(path.dirname(credentialFile)).mode & 0o777, 0o700);
      const events = (await wait(session)).events;
      assert(events.some((event) => event.type === 'text' && event.text.includes('7741')));
      assert(events.some((event) => event.type === 'tool' && event.name === 'indymat · run_code'));
      const state = await (await server.request('state')).json();
      assert(state.variables.some((variable) => variable.name === 'bridge_fixture_' + provider));
      assert(state.assistant_jobs.some((job) => job.provider === provider && job.code.includes('7741')));
      assert.equal((await bridge(credential.token, 'tools', {}, { Origin: new URL(server.base).origin })).status, 403);
      assert.equal((await bridge(credential.token, 'tools', {}, { Origin: '' })).status, 403);
      const badHost = await new Promise((resolve, reject) => {
        const request = http.request(new URL('api/bridge/tools', server.base), { method: 'POST', headers: { Host: 'evil.invalid', 'X-IndyMAT-Bridge': credential.token } }, (response) => { response.resume(); resolve(response.statusCode); });
        request.on('error', reject);
        request.end('{}');
      });
      assert.equal(badHost, 403);
      assert.equal((await server.request('execute', { code: 'must_not_run=1;' }, { 'X-MF-Token': credential.token })).status, 403);
      const status = await (await bridge(credential.token, 'call', { name: 'session_status', arguments: {} })).json();
      assert.equal(status.isError, false);
      assert.equal(JSON.parse(status.content[0].text).access_level, 'run');
      const resumed = await server.request('assistant/start', { provider, conversation: session, session_access: 'run', prompt: 'bridge-run' });
      assert.equal(resumed.status, 202);
      await wait(session);
      assert.equal((await server.request('assistant/start', { provider, conversation: session, session_access: 'inspect', prompt: 'no' })).status, 400);
      assert.equal((await server.request('assistant/remove', { session })).status, 200);
      assert(!fs.existsSync(credentialFile));
      assert(!fs.existsSync(configFile));
      assert.equal((await bridge(credential.token, 'call', { name: 'session_status' })).status, 403);
    }
    const { session } = await (await server.request('assistant/start', { provider: 'claude', session_access: 'inspect', prompt: 'hello' })).json();
    await wait(session);
    const credential = JSON.parse(fs.readFileSync(path.join(directory, '.matlab-free', 'bridge', session + '.json'), 'utf8'));
    const tools = await (await bridge(credential.token, 'tools', {})).json();
    assert(!tools.tools.some((tool) => tool.name === 'run_code'));
    const refused = await (await bridge(credential.token, 'call', { name: 'run_code', arguments: { code: 'inspect_escape=1;' } })).json();
    assert.equal(refused.isError, true);
    assert(refused.content[0].text);
    assert.equal((await server.request('assistant/start', { provider: 'agy', session_access: 'inspect', prompt: 'no' })).status, 400);
    console.log('HTTP ASSISTANT BRIDGE PASS: both MCP fixtures/resume, live variables, console feed, capability isolation/revocation, Origin/Host, modes and permissions.');
  } finally { await server.close(); }
})().catch((error) => { console.error(error); process.exitCode = 1; });
