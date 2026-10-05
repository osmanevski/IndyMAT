const assert = require('node:assert/strict');
const http = require('node:http');
const { assistantServer, delay } = require('./assistant_harness.cjs');
(async () => {
  const server = await assistantServer();
  const { request } = server;
  const wait = async (session) => {
    for (let index = 0; index < 150; index++) {
      const result = await (await request('assistant/events?session=' + session)).json();
      if (!result.running) return result;
      await delay(30);
    }
    throw new Error('Assistant turn timeout');
  };
  try {
    for (const [endpoint, body] of [['assistant/providers', undefined], ['assistant/start', { provider: 'codex', prompt: 'hello' }], ['assistant/events?session=nope', undefined], ['assistant/stop', { session: 'nope' }]]) {
      assert.equal((await request(endpoint, body, { 'X-MF-Token': 'incorrect' })).status, 403);
      assert.equal((await request(endpoint, body, { Origin: 'https://evil.invalid' })).status, 403);
      assert.equal((await request(endpoint, body, { 'Sec-Fetch-Site': 'cross-site' })).status, 403);
      const badHost = await new Promise((resolve, reject) => {
        const req = http.request(new URL('api/' + endpoint, server.base), { method: body === undefined ? 'GET' : 'POST', headers: { Host: 'evil.invalid', 'X-MF-Token': server.token, 'Content-Type': 'application/json' } }, (res) => { res.resume(); resolve(res.statusCode); });
        req.on('error', reject);
        req.end(body === undefined ? undefined : JSON.stringify(body));
      });
      assert.equal(badHost, 403);
    }
    const providers = await (await request('assistant/providers')).json();
    assert.deepEqual(providers.map((provider) => provider.available), [true, true, true]);
    for (const provider of ['claude', 'codex', 'agy']) {
      const response = await request('assistant/start', { provider, mode: 'read-only', prompt: '--dangerously-skip-permissions is just prompt text', context: { path: 'sample.m', dirty: true } });
      assert.equal(response.status, 202);
      const { session } = await response.json();
      const events = (await wait(session)).events;
      assert(events.some((event) => event.type === 'text' && event.text.includes('Fixture answer')));
      assert(events.some((event) => event.type === 'raw' && event.text.includes('future.event')));
      assert.equal(events.at(-1).state, 'completed');
      const resumed = await request('assistant/start', { provider, prompt: 'continue', conversation: session });
      assert.equal(resumed.status, 202, provider + " " + JSON.stringify(await resumed.clone().json()));
      await wait(session);
    }
    assert.equal((await request('assistant/start', { provider: 'codex', prompt: 'a', mode: 'danger-full-access' })).status, 400);
    assert.equal((await request('assistant/start', { provider: 'codex', prompt: 'a', context: { path: '../escape.m' } })).status, 403);
    assert.equal((await request('assistant/start', { provider: 'codex', prompt: 'a', flags: ['--bad'] })).status, 400);
    const { session } = await (await request('assistant/start', { provider: 'codex', prompt: 'wait' })).json();
    assert.equal((await request('assistant/stop', { session })).status, 200);
    assert.equal((await request('assistant/stop', { session })).status, 200);
    assert.equal((await wait(session)).events.at(-1).state, 'stopped');
    console.log('HTTP ASSISTANT PASS: authentication, Host/Origin, enums, context bounds, three streams, resume, stop.');
  } finally { await server.close(); }
})().catch((error) => { console.error(error); process.exitCode = 1; });
