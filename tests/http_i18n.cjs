const assert = require('assert');
const fs = require('fs');
const http = require('http');

(async () => {
  const launch = JSON.parse(fs.readFileSync('.matlab-free/launch.json', 'utf8'));
  const [base, token] = launch.url.split('#');
  const request = (route, language, body, extra = {}) => fetch(base + 'api/' + route, {
    method: body === undefined ? 'GET' : 'POST',
    headers: { 'X-MF-Token': token, ...(language === undefined ? {} : {'X-MF-Language': language}), ...(body === undefined ? {} : {'Content-Type': 'application/json'}), ...extra },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const error = async (language, extra) => (await (await request('i18n-missing', language, undefined, extra)).json()).error;
  const idle = async () => {
    for (let i = 0; i < 400; i++) {
      const state = await (await request('state')).json();
      if (state.status === 'idle' || state.status === 'dead') return state;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw new Error('Language job timeout');
  };
  assert.equal(await error(), 'Not found');
  const start = await idle();
  assert.equal(await error('tr'), 'Bulunamadı');
  assert.equal(await error(), 'Bulunamadı');
  for (const invalid of ['TR', 'system', 'fr', '', 'en,tr']) assert.equal(await error(invalid), 'Bulunamadı');
  for (const extra of [{'X-MF-Token': 'invalid'}, {Origin: 'https://evil.invalid'}, {'Sec-Fetch-Site': 'cross-site'}]) {
    assert.equal((await request('i18n-missing', 'en', undefined, extra)).status, 403);
    assert.equal(await error(), 'Bulunamadı');
  }
  const raw = headers => new Promise((resolve, reject) => {
    const url = new URL(base + 'api/i18n-missing');
    const req = http.request({hostname: url.hostname, port: url.port, path: url.pathname, headers}, response => {
      let text = '';response.setEncoding('utf8');response.on('data', chunk => text += chunk);
      response.on('end', () => resolve({status: response.statusCode, body: JSON.parse(text)}));
    });
    req.on('error', reject);req.end();
  });
  assert.equal((await raw({'X-MF-Token': token, 'X-MF-Language': 'tr '})).body.error, 'Bulunamadı');
  const duplicate = await raw({'X-MF-Token': token, 'X-MF-Language': ['en', 'tr']});
  assert.equal(duplicate.body.error, 'Bulunamadı');
  assert.equal((await raw({Host: 'evil.invalid', 'X-MF-Token': token, 'X-MF-Language': 'en'})).status, 403);
  assert.equal(await error(), 'Bulunamadı');
  let response = await request('execute', 'tr', {mode: 'code', code: "assert(strcmp(getenv('INDYMAT_LANGUAGE'),'tr')); language_probe=81;"});
  assert.equal(response.status, 202);assert.equal((await idle()).error, null);
  response = await request('execute', undefined, {mode: 'code', code: "pause(.5); assert(strcmp(getenv('INDYMAT_LANGUAGE'),'tr')); assert(language_probe==81);"});
  assert.equal(response.status, 202);
  assert.equal(await error('en'), 'Not found');
  assert.equal((await idle()).error, null);
  response = await request('execute', undefined, {mode: 'code', code: "assert(strcmp(getenv('INDYMAT_LANGUAGE'),'en')); assert(language_probe==81); clear language_probe;"});
  assert.equal(response.status, 202);
  const end = await idle();assert.equal(end.error, null);assert.equal(end.epoch, start.epoch);
  console.log('HTTP I18N PASS: English default, strict authenticated language headers, security ordering, real persistent Octave language changes.');
})().catch(error => { console.error(error); process.exit(1); });
