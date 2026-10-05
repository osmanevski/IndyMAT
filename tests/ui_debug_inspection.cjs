const assert = require('assert');
const fs = require('fs');
const path = require('path');
const {randomUUID} = require('crypto');
const {chromium} = require('playwright');

(async () => {
  const launch = JSON.parse(fs.readFileSync('.matlab-free/launch.json', 'utf8'));
  const [base, token] = launch.url.split('#');
  const suffix = randomUUID().replaceAll('-', '');
  const stem = 'ui_debug_inspect_' + suffix;
  const file = path.resolve('workspace', stem + '.m');
  fs.writeFileSync(file, `function out=${stem}(seed)\n  outer_local=seed+100;\n  out=middle(seed);\n  function mid=middle(value)\n    middle_local=[value,value+10];\n    mid=inner(value);\n  endfunction\n  function answer=inner(value)\n    inner_local=value+1;\n    answer=inner_local*2;\n  endfunction\nendfunction\n`);
  const request = (route, data) => fetch(base + 'api/' + route, {method: data === undefined ? 'GET' : 'POST', headers: {'X-MF-Token': token, ...(data === undefined ? {} : {'Content-Type': 'application/json'})}, body: data === undefined ? undefined : JSON.stringify(data)});
  const state = async () => await (await request('state')).json();
  const wait = async predicate => {
    for (let index = 0; index < 500; index++) {
      const value = await state();
      if (predicate(value)) return value;
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    throw new Error('engine timeout: ' + JSON.stringify(await state()));
  };
  const waitIdle = job => wait(value => value.status === 'idle' && (!job || value.job === job));
  let browser;
  try {
    let response = await request('breakpoints-clear', {});
    await waitIdle((await response.json()).job);
    response = await request('breakpoint', {path: file, line: 10, action: 'set', condition: '', enabled: true});
    await waitIdle((await response.json()).job);
    response = await request('execute', {mode: 'code', code: `${stem}_result=${stem}(4);`});
    const {job} = await response.json();
    await wait(value => value.status === 'paused' && value.job === job && value.debug?.ready);
    browser = await chromium.launch({headless: true});
    const page = await browser.newPage({ locale: "tr-TR" });
    await page.goto(launch.url);
    await page.waitForFunction(() => document.querySelector('#status-text')?.textContent === 'Kesme noktasında' && document.querySelectorAll('#debug-stack .debug-frame').length >= 3);
    const frameResponse = page.waitForResponse(value => new URL(value.url()).pathname === '/api/debug' && value.request().method() === 'POST');
    await page.locator('#debug-stack .debug-frame[data-frame="2"]').click();
    assert.equal((await frameResponse).status(), 200);
    await page.waitForFunction(() => document.querySelector('#debug-stack .debug-frame[data-frame="2"]')?.getAttribute('aria-current') === 'true' && document.querySelector('#variables tr[data-name="middle_local"]'));
    const inspectResponse = page.waitForResponse(value => new URL(value.url()).pathname === '/api/debug' && value.request().postDataJSON()?.command === 'inspect');
    await page.locator('#variables tr[data-name="middle_local"] td').nth(1).dblclick();
    assert.equal((await inspectResponse).status(), 200);
    await page.waitForFunction(() => document.querySelector('#modal[open] #modal-title')?.textContent === 'middle_local — Değişken görünümü');
    const body = await page.locator('#modal-body').innerText();
    assert(body.includes('double'));
    assert(body.includes('14'));
    const snapshot = await state();
    assert.equal(snapshot.job, job);
    assert.equal(snapshot.debug.frame, 2);
    console.log('UI DEBUG INSPECTION PASS: double click opens selected-frame structured variable detail.');
  } finally {
    if (browser) await browser.close();
    await request('stop', {}).catch(() => {});
    await waitIdle().catch(() => {});
    const cleared = await request('breakpoints-clear', {}).catch(() => null);
    if (cleared?.status === 202) await waitIdle((await cleared.json()).job).catch(() => {});
    const cleanup = await request('execute', {mode: 'code', code: `clear ${stem}_result ${stem};`}).catch(() => null);
    if (cleanup?.status === 202) await waitIdle((await cleanup.json()).job).catch(() => {});
    fs.rmSync(file, {force: true});
  }
})().catch(error => {
  console.error(error);
  process.exit(1);
});
