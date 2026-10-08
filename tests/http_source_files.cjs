const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

(async () => {
  // The gate supplies a fresh test server; credentials never leave this process.
  const [base, token] = JSON.parse(fs.readFileSync('.matlab-free/launch.json', 'utf8')).url.split('#');
  const request = (route, data, headers = {}) => fetch(base + 'api/' + route, {
    method: data === undefined ? 'GET' : 'POST',
    headers: { 'X-MF-Token': token, ...data === undefined ? {} : { 'Content-Type': 'application/json' }, ...headers },
    body: data === undefined ? undefined : JSON.stringify(data)
  });
  const state = async () => (await request('state')).json();
  const wait = async (predicate = (s) => s.status === 'idle') => {
    for (let i = 0; i < 1200; i++) {
      const value = await state();
      if (predicate(value)) return value;
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error('job timeout');
  };
  assert.equal((await state()).source_file_adapter_version, 1);
  const folder = path.resolve('workspace/source_files_http');
  fs.mkdirSync(folder, { recursive: true });
  const file = path.join(folder, 'entry.m');
  const digest = (data) => crypto.createHash('sha256').update(data).digest('hex');
  const payload = (source, extra = {}) => ({ code: '', mode: 'file', argument: file, history: false, adapt_file_literals: true, file_hash: digest(source), ...extra });
  const run = async (data) => {
    const response = await request('execute', data);
    assert.equal(response.status, 202, await response.clone().text());
    const accepted = await response.json();
    const result = await wait();
    assert.equal(result.job, accepted.job);
    return { accepted, result };
  };
  await wait();
  try {
    const original = 'r="ab"+"cd";';
    fs.writeFileSync(file, original);
    const native = await run({ code: '', mode: 'file', argument: file, history: false });
    assert.deepEqual(Object.keys(native.accepted), ['job']);
    assert.equal(native.result.source_adapter, undefined);
    assert.equal(native.result.variables.find((v) => v.name === 'r').class, 'double');
    const adapted = await run(payload(original));
    assert.equal(adapted.result.source_adapter.scope, 'entry-file');
    assert.equal(adapted.result.kind, 'file');
    assert.equal(adapted.result.source_adapter.status, 'adapted');
    assert.equal(adapted.accepted.source_context.document, original);
    assert.deepEqual(adapted.accepted.source_context.span, { start_utf16: 0, end_utf16: original.length });
    assert.equal(adapted.accepted.source_context.snapshot_sha256, digest(original));
    assert.equal(adapted.result.variables.find((v) => v.name === 'r').class, 'string');
    assert.equal(fs.readFileSync(file, 'utf8'), original);
    const generated = path.resolve('.matlab-free/jobs', adapted.accepted.job, 'entry/entry.m');
    assert.equal(fs.statSync(generated).mode & 0o222, 0);
    assert.match(fs.readFileSync(generated, 'utf8'), /\(@string\)/);
    assert.equal((await request('file?path=' + encodeURIComponent(generated))).status, 403);
    assert.equal((await request('execute', payload(original, { argument: generated }))).status, 403);

    const before = (await state()).job;
    const rejected = [
      payload(original, { adapt_file_literals: 'true' }),
      payload(original, { file_hash: true }),
      payload(original, { mode: 'code', argument: '', code: original }),
      payload(original, { code: original }),
      payload(original, { source_context: adapted.accepted.source_context }),
      payload(original, { adapt_editor_literals: true }),
      payload(original, { generated_path: file }),
      payload(original, { generated_text: original }),
      payload(original, { source_map: {} }),
      payload(original, { physical_path: file }),
      payload(original, { origin: 'command-window' }),
      payload(original, { source_file: { path: file } })
    ];
    for (const data of rejected) {
      const response = await request('execute', data);
      assert.equal(response.status, 400, await response.text());
      assert.equal((await state()).job, before);
    }
    assert.equal((await request('execute', payload(original), { Origin: 'https://untrusted.example' })).status, 403);
    fs.writeFileSync(file, 'changed=1;');
    assert.equal((await request('execute', payload(original))).status, 409);
    assert.equal((await state()).job, before);

    // A job already executing its immutable entry keeps the accepted snapshot.
    const inputSource = 'r="snapshot"; n=input(\'Number: \'); after_input="still snapshot";';
    fs.writeFileSync(file, inputSource);
    const response = await request('execute', payload(inputSource));
    assert.equal(response.status, 202);
    const accepted = await response.json();
    await wait((s) => s.waiting_input);
    fs.writeFileSync(file, 'r="replacement";');
    assert.equal((await request('input', { text: '17' })).status, 200);
    const inputResult = await wait();
    assert.equal(inputResult.job, accepted.job);
    assert.equal(inputResult.error, null);
    assert.equal(inputResult.variables.find((v) => v.name === 'n').preview, '17');
    assert.equal(accepted.source_context.document, inputSource);
    assert.equal(fs.readFileSync(path.resolve('.matlab-free/jobs', accepted.job, 'saved-source.bin'), 'utf8'), inputSource);

    const failing = 'r="x";\nmissing_saved_http;';
    fs.writeFileSync(file, failing);
    const failed = await run(payload(failing));
    assert.equal(failed.result.error, failed.result.raw_error);
    assert.equal(failed.result.source_error_locations[0].path, file);
    assert.equal(failed.result.source_error_locations[0].line, 2);
    assert(failed.result.error_frames.some((frame) => frame.file === path.resolve('.matlab-free/jobs', failed.accepted.job, 'entry/entry.m')));
    assert.equal(failed.accepted.source_context.document, failing);

    const fallback = 'r="x"; disp "hello"';
    fs.writeFileSync(file, fallback);
    const unchanged = await run(payload(fallback));
    assert.equal(unchanged.result.source_adapter.status, 'fallback');
    assert.equal(unchanged.result.variables.find((v) => v.name === 'r').class, 'char');
    const off = await run({ code: '', mode: 'file', argument: file, history: false, adapt_file_literals: false });
    assert.equal(off.result.source_adapter, undefined);
    console.log('HTTP SOURCE FILES PASS: default/off, explicit opt-in, saved hash 409, authoritative UTF-16 snapshot, private staging, Origin/flag/map rejection, immutable input snapshot and raw/logical errors.');
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
})().catch((error) => { console.error(error); process.exit(1); });
