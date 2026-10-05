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
    const endpoint = 'assistant/models?provider=codex';
    for (const headers of [{ 'X-MF-Token': 'incorrect' }, { Origin: 'https://evil.invalid' }, { 'Sec-Fetch-Site': 'cross-site' }]) assert.equal((await request(endpoint, undefined, headers)).status, 403);
    const badHost = await new Promise((resolve, reject) => {
      const req = http.request(new URL('api/' + endpoint, server.base), { headers: { Host: 'evil.invalid', 'X-MF-Token': server.token } }, (res) => { res.resume(); resolve(res.statusCode); });
      req.on('error', reject);
      req.end();
    });
    assert.equal(badHost, 403);
    assert.equal((await request('assistant/models?provider=bad')).status, 400);
    for (const [provider, model, effort] of [['claude', 'sonnet', 'high'], ['codex', 'gpt-6-astra', 'high'], ['agy', 'gemini-3.8-flash-high', '']]) {
      const response = await request('assistant/models?provider=' + provider);
      assert.equal(response.status, 200);
      const catalog = await response.json();
      assert.equal(catalog.models[0].id, '');
      assert(catalog.models.some((item) => item.id === model));
      assert.equal(catalog.efforts_separate, provider !== 'agy');
      const start = await request('assistant/start', { provider, mode: 'read-only', prompt: 'config', model, effort });
      assert.equal(start.status, 202);
      const { session } = await start.json();
      const events = (await wait(session)).events;
      const config = JSON.parse(events.find((event) => event.type === 'text' && event.text.startsWith('{')).text);
      assert.equal(provider === 'codex' ? config.thread.model : config.model, model);
      assert.equal(config.effort, effort);
      assert.equal((await request('assistant/start', { provider, prompt: 'hello', conversation: session, model: '' })).status, 400);
    }
    for (const model of ['not-listed', '--flag', 'a b', 'a\n']) assert.equal((await request('assistant/start', { provider: 'codex', prompt: 'hello', model })).status, 400);
    console.log('HTTP ASSISTANT MODELS PASS: route security, catalog shapes, selected model/effort dispatch, fixed model and invalid choice rejection.');
  } finally { await server.close(); }
})().catch((error) => { console.error(error); process.exitCode = 1; });
