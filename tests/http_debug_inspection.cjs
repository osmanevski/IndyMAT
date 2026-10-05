const assert = require('assert');
const fs = require('fs');
const path = require('path');
const {randomUUID} = require('crypto');

(async () => {
  const {url} = JSON.parse(fs.readFileSync('.matlab-free/launch.json', 'utf8'));
  const [base, token] = url.split('#');
  const suffix = randomUUID().replaceAll('-', '');
  const stem = 'http_debug_inspect_' + suffix;
  const file = path.resolve('workspace', stem + '.m');
  fs.writeFileSync(file, `function out=${stem}(seed)\n  outer_local=seed+100;\n  out=middle(seed);\n  function mid=middle(value)\n    middle_local=[value,value+10];\n    mid=inner(value);\n  endfunction\n  function answer=inner(value)\n    inner_local=value+1;\n    answer=inner_local*2;\n  endfunction\nendfunction\n`);
  const request = (route, data) => fetch(base + 'api/' + route, {method: data === undefined ? 'GET' : 'POST', headers: {'X-MF-Language': 'tr', 'X-MF-Token': token, ...(data === undefined ? {} : {'Content-Type': 'application/json'})}, body: data === undefined ? undefined : JSON.stringify(data)});
  const state = async () => await (await request('state')).json();
  const wait = async (predicate, label) => {
    for (let index = 0; index < 500; index++) {
      const value = await state();
      if (predicate(value)) return value;
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    throw new Error(label + ' timeout: ' + JSON.stringify(await state()));
  };
  const waitIdle = job => wait(value => value.status === 'idle' && (!job || value.job === job), 'idle');
  const waitPaused = (job, serial = -1) => wait(value => value.status === 'paused' && value.job === job && value.debug?.ready && value.debug.serial > serial, 'paused');
  try {
    let response = await request('breakpoints-clear', {});
    assert.equal(response.status, 202);
    await waitIdle((await response.json()).job);
    response = await request('debug', {command: 'inspect', code: 'idle_name'});
    assert.equal(response.status, 400, 'debug inspection accepted while idle');
    response = await request('breakpoint', {path: file, line: 10, action: 'set', condition: '', enabled: true});
    assert.equal(response.status, 202);
    await waitIdle((await response.json()).job);
    response = await request('execute', {mode: 'code', code: `${stem}_result=${stem}(4);`});
    assert.equal(response.status, 202);
    const {job} = await response.json();
    let snapshot = await waitPaused(job);
    const epoch = snapshot.epoch;
    let serial = snapshot.debug.serial;
    response = await request('debug', {command: 'frame', code: 2});
    assert.equal(response.status, 200);
    snapshot = await waitPaused(job, serial);
    assert.equal(snapshot.debug.frame, 2);
    serial = snapshot.debug.serial;
    response = await request('debug', {command: 'inspect', code: 'middle_local'});
    assert.equal(response.status, 200);
    snapshot = await waitPaused(job, serial);
    assert.equal(snapshot.job, job);
    assert.equal(snapshot.epoch, epoch);
    assert.equal(snapshot.debug.frame, 2);
    assert.equal(snapshot.debug.detail.name, 'middle_local');
    assert.deepStrictEqual(snapshot.debug.detail.rows, [['4', '14']]);
    for (const code of ['middle_local; dbquit', 'middle_local(1)', '__mf_hidden']) {
      response = await request('debug', {command: 'inspect', code});
      assert.equal(response.status, 400, code);
    }
    assert.equal((await state()).status, 'paused');
    console.log('HTTP DEBUG INSPECTION PASS: selected-frame detail, same job/epoch, real-pause and name guards.');
  } finally {
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
