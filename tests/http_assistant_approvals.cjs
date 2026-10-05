const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { assistantServer, delay } = require('./assistant_harness.cjs');
(async () => {
  const server = await assistantServer();
  const sample = path.join(server.workspace, 'sample.m');
  const poll = async (session, predicate) => {
    for (let index = 0; index < 200; index++) {
      const result = await (await server.request('assistant/events?session=' + session)).json();
      if (predicate(result)) return result;
      await delay(30);
    }
    throw new Error('Approval fixture timeout');
  };
  const start = async (prompt) => {
    const response = await server.request('assistant/start', { provider: 'claude', mode: 'ask', session_access: 'none', prompt });
    assert.equal(response.status, 202);
    const result = await response.json();
    const events = await poll(result.session, (result) => result.approvals?.length === 1);
    return { ...result, approval: events.approvals[0], events: events.events };
  };
  const decide = async (turn, decision, message) => {
    const response = await server.request('assistant/approve', { session: turn.session, id: turn.approval.id, decision, ...(message ? { message } : {}) });
    assert.equal(response.status, 200);
    return response.json();
  };
  try {
    for (const provider of ['agy']) assert.equal((await server.request('assistant/start', { provider, mode: 'ask', prompt: 'hello' })).status, 400);
    const codex = await (await server.request('assistant/start', { provider: 'codex', prompt: 'approval-command-write' })).json();
    const codexPending = await poll(codex.session, (result) => result.approvals?.length === 1);
    assert.equal(codexPending.approvals[0].tool, 'Shell');
    assert.equal(codexPending.approvals[0].summary, 'printf approved > sample.m');
    assert(codexPending.approvals[0].detail.text.includes('/bin/zsh'));
    await decide({ ...codex, approval: codexPending.approvals[0] }, 'allow');
    await poll(codex.session, (result) => !result.running);
    assert.equal(fs.readFileSync(sample, 'utf8'), 'approved = 42;\n');
    fs.writeFileSync(sample, 'initial = 1;\n');
    const allowed = await start('approval');
    assert(allowed.events.some((event) => event.type === 'approval' && event.id === allowed.approval.id));
    assert.equal(allowed.approval.tool, 'Write');
    assert.equal(allowed.approval.summary, 'sample.m');
    assert(allowed.approval.detail.text.includes('-initial = 1;'));
    assert(allowed.approval.detail.text.includes('+approved = 42;'));
    assert.equal(fs.readFileSync(sample, 'utf8'), 'initial = 1;\n');
    const body = { session: allowed.session, id: allowed.approval.id, decision: 'allow' };
    assert.equal((await server.request('assistant/approve', body, { 'X-MF-Token': 'wrong' })).status, 403);
    assert.equal((await server.request('assistant/approve', body, { Origin: 'https://evil.invalid' })).status, 403);
    // fetch() refuses to forge Host, so that request is made with node:http.
    const badHost = await new Promise((resolve, reject) => {
      const forged = require('node:http').request(new URL('api/assistant/approve', server.base), { method: 'POST', headers: { Host: 'evil.invalid', 'X-MF-Token': server.token, 'Content-Type': 'application/json' } }, (response) => { response.resume(); resolve(response.statusCode); });
      forged.on('error', reject);
      forged.end(JSON.stringify(body));
    });
    assert.equal(badHost, 403);
    const decision = await decide(allowed, 'allow');
    assert.deepEqual(await decide(allowed, 'deny'), decision);
    const complete = await poll(allowed.session, (result) => !result.running);
    assert(complete.events.some((event) => event.type === 'approval-resolved' && event.id === allowed.approval.id && event.decision === 'allow'));
    assert.equal(fs.readFileSync(sample, 'utf8'), 'approved = 42;\n');
    fs.writeFileSync(sample, 'before-deny = 1;\n');
    const denied = await start('approval');
    assert.equal((await server.request('assistant/approve', { ...body, session: denied.session })).status, 400);
    await decide(denied, 'deny', 'Please keep this file');
    const denial = await poll(denied.session, (result) => !result.running);
    assert.equal(fs.readFileSync(sample, 'utf8'), 'before-deny = 1;\n');
    assert(denial.events.some((event) => event.type === 'text' && event.text.includes('Please keep this file')));
    const remembered = await start('approval-twice');
    await decide(remembered, 'allow-conversation');
    const repeated = await poll(remembered.session, (result) => !result.running);
    assert.equal(repeated.events.filter((event) => event.type === 'approval').length, 1);
    assert.equal(fs.readFileSync(sample, 'utf8'), 'approved = 43;\n');
    // The permission survives a turn resume only in that same conversation.
    const resumed = await server.request('assistant/start', { provider: 'claude', mode: 'ask', prompt: 'approval', conversation: remembered.session });
    assert.equal(resumed.status, 202);
    const again = await poll(remembered.session, (result) => !result.running);
    assert.equal(again.events.filter((event) => event.type === 'approval').length, 1);
    const stopped = await start('approval');
    await server.request('assistant/stop', { session: stopped.session });
    const end = await poll(stopped.session, (result) => !result.running);
    assert(end.events.some((event) => event.type === 'approval-resolved' && event.decision === 'deny'));
    assert.equal(fs.readFileSync(sample, 'utf8'), 'approved = 42;\n');
    const directory = path.dirname(server.workspace);
    const capability = JSON.parse(fs.readFileSync(path.join(directory, '.matlab-free', 'bridge', stopped.session + '.json'), 'utf8'));
    const tools = await fetch(server.base + 'api/bridge/tools', { method: 'POST', headers: { 'X-IndyMAT-Bridge': capability.token, 'Content-Type': 'application/json' }, body: '{}' });
    assert.deepEqual((await tools.json()).tools.map((tool) => tool.name), ['approve']);
    assert.equal((await server.request('assistant/approve', { ...body, decision: 'always' })).status, 400);
    await server.request('assistant/remove', { session: stopped.session });
    assert(!fs.existsSync(path.join(directory, '.matlab-free', 'bridge', stopped.session + '.json')));
    console.log('HTTP ASSISTANT APPROVALS PASS: event, allow/deny, exact decisions, conversation reuse, Stop, scoping and route guards.');
  } finally {
    await server.close();
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
