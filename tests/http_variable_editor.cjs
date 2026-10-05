const assert = require("assert");
const fs = require("fs");
const { randomUUID } = require("crypto");
const { typedCell } = require('../frontend/variable_editor_utils.cjs');

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const [base, token] = launch.url.split("#");
  const request = (endpoint, body) => fetch(base + endpoint, { method: body === undefined ? "GET" : "POST", headers: { "X-MF-Language": "tr", "X-MF-Token": token, ...body === undefined ? {} : { "Content-Type": "application/json" } }, body: body === undefined ? undefined : JSON.stringify(body) });
  const responseJSON = async (response) => ({ status: response.status, body: await response.json() });
  const waitIdle = async () => {
    for (let index = 0; index < 400; index++) {
      let state = await (await request("api/state")).json();
      if (state.status === "idle" || state.status === "dead") return state;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error("Variable editor job timeout");
  };
  const suffix = randomUUID().replaceAll("-", "");
  const prefix = "http_ve_" + suffix;
  const matrix = prefix + "_matrix", nd = prefix + "_nd", integer = prefix + "_integer", cell = prefix + "_cell", logical = prefix + "_logical", character = prefix + "_char", floating = prefix + "_floating", keep = prefix + "_keep";
  const job = async (body) => {
    let response = await request("api/variable", body);
    assert.equal(response.status, 202, JSON.stringify(await response.json()));
    return waitIdle();
  };
  try {
    let response = await request("api/execute", { mode: "code", code: `${matrix}=reshape(1:20,[4,5]);${nd}=reshape(1:48,[4,3,4]);${integer}=int8([1,2;3,4]);${cell}={struct('field',single(7)),reshape(1:8,[2,2,2])};${logical}=logical([0,1]);${character}='ab';${floating}=single(1);${keep}=913;` });
    assert.equal(response.status, 202);
    let state = await waitIdle();
    const epoch = state.epoch;
    let read = await job({ action: "read", name: nd, path: [], row: 2, column: 2, rows: 2, columns: 2, slices: [3], epoch });
    assert(!read.error, read.error);
    assert.equal(read.kind, "variable-read");
    assert.deepEqual(read.variable_action.rows, [["30","34"],["31","35"]]);
    assert.deepEqual(read.variable_action.size, [4,3,4]);
    let nested = await job({ action: "read", name: cell, path: [{ kind: "cell", indices: [1,2] }], row: 1, column: 1, rows: 2, columns: 2, slices: [2], epoch });
    assert(!nested.error, nested.error);
    assert.deepEqual(nested.variable_action.rows, [["5","7"],["6","8"]]);
    let rejected = await responseJSON(await request("api/variable", { action: "read", name: matrix, path: [], row: 1, column: 1, rows: 101, columns: 1, slices: [], epoch }));
    assert.equal(rejected.status, 400);
    rejected = await responseJSON(await request("api/variable", { action: "read", name: `${matrix};clear`, path: [], row: 1, column: 1, rows: 1, columns: 1, slices: [], epoch }));
    assert.equal(rejected.status, 400);
    read = await job({ action: "read", name: integer, path: [], row: 1, column: 1, rows: 2, columns: 2, slices: [], epoch });
    let readJob = read.job;
    rejected = await responseJSON(await request("api/variable", { action: "write", name: integer, path: [], row: 1, column: 1, height: 1, width: 1, slices: [], epoch, read_job: readJob, class: "int8", size: [2,2], values: [{ type: "integer", value: "128" }] }));
    assert.equal(rejected.status, 400);
    let written = await job({ action: "write", name: integer, path: [], row: 1, column: 1, height: 2, width: 2, slices: [], epoch, read_job: readJob, class: "int8", size: [2,2], values: ["-128","127","5","6"].map((value) => ({ type: "integer", value })) });
    assert(!written.error, written.error);
    read = await job({ action: "read", name: integer, path: [], row: 1, column: 1, rows: 2, columns: 2, slices: [], epoch });
    assert.deepEqual(read.variable_action.rows, [["-128","127"],["5","6"]]);
    read = await job({ action: "read", name: logical, path: [], row: 1, column: 1, rows: 1, columns: 2, slices: [], epoch });
    written = await job({ action: "write", name: logical, path: [], row: 1, column: 1, height: 1, width: 2, slices: [], epoch, read_job: read.job, class: "logical", size: [1,2], values: [{ type: "logical", value: true },{ type: "logical", value: false }] });
    assert(!written.error, written.error);
    read = await job({ action: "read", name: logical, path: [], row: 1, column: 1, rows: 1, columns: 2, slices: [], epoch });
    assert.deepEqual(read.variable_action.rows, [["true","false"]]);
    read = await job({ action: "read", name: character, path: [], row: 1, column: 1, rows: 1, columns: 2, slices: [], epoch });
    rejected = await responseJSON(await request('api/variable', { action: 'write', name: character, path: [], row: 1, column: 1, height: 1, width: 1, slices: [], epoch, read_job: read.job, class: 'char', size: [1,2], values: [{ type: 'char', value: 'z' }] }));
    assert.equal(rejected.status, 400, 'byte-cell char writes must be refused');
    written = await job({ action: "write", name: character, path: [], row: 1, column: 1, height: 1, width: 1, slices: [], epoch, read_job: read.job, class: "char", size: [1,2], values: [typedCell('char', 'ğİş🙂')] });
    assert(!written.error, written.error);
    read = await job({ action: "read", name: character, path: [], row: 1, column: 1, rows: 1, columns: 2, slices: [], epoch });
    assert.equal(read.variable_action.text, 'ğİş🙂');
    assert.deepEqual(read.variable_action.size, [1,10]);
    assert.equal(read.variable_action.editable, true);
    read = await job({ action: "read", name: floating, path: [], row: 1, column: 1, rows: 1, columns: 1, slices: [], epoch });
    written = await job({ action: "write", name: floating, path: [], row: 1, column: 1, height: 1, width: 1, slices: [], epoch, read_job: read.job, class: "single", size: [1,1], values: [{ type: "special", value: "NaN" }] });
    assert(!written.error, written.error);
    read = await job({ action: "read", name: floating, path: [], row: 1, column: 1, rows: 1, columns: 1, slices: [], epoch });
    assert.deepEqual(read.variable_action.rows, [["NaN"]]);
    written = await job({ action: 'write', name: floating, path: [], row: 1, column: 1, height: 1, width: 1, slices: [], epoch, read_job: read.job, class: 'single', size: [1,1], values: [typedCell('single', '-0')] });
    assert(!written.error, written.error);
    response = await request('api/execute', {mode:'code',code:`assert(isa(${floating},'single') && signbit(${floating}) && 1/${floating}<0); assert(strcmp(native2unicode(uint8(${character}),'UTF-8'),'ğİş🙂'));`});
    assert.equal(response.status, 202);
    state = await waitIdle();
    assert(!state.error, state.error);
    response = await request("api/execute", { mode: "code", code: `${integer}=int8([9,9;9,9]);` });
    assert.equal(response.status, 202);
    await waitIdle();
    rejected = await responseJSON(await request("api/variable", { action: "write", name: integer, path: [], row: 1, column: 1, height: 1, width: 1, slices: [], epoch, read_job: read.job, class: "int8", size: [2,2], values: [{ type: "integer", value: "1" }] }));
    assert.equal(rejected.status, 400);
    assert(rejected.body.error.includes("son okumadan sonra"));
    read = await job({ action: "read", name: integer, path: [], row: 1, column: 1, rows: 2, columns: 2, slices: [], epoch });
    let mismatch = await job({ action: "write", name: integer, path: [], row: 1, column: 1, height: 1, width: 1, slices: [], epoch, read_job: read.job, class: "int8", size: [1,4], values: [{ type: "integer", value: "1" }] });
    assert(mismatch.error && mismatch.error.includes("boyutu değişti"), mismatch.error);
    let end = await (await request("api/state")).json();
    assert.equal(end.epoch, epoch);
    assert(end.variables.some((item) => item.name === keep && item.preview === "913"));
    console.log("HTTP VARIABLE EDITOR PASS: bounds, N-D slices, typed writes, stale metadata and persistent session.");
  } finally {
    try {
      let state = await (await request("api/state")).json();
      if (state.status === "idle") {
        let names = [matrix, nd, integer, cell, logical, character, floating, keep].filter((name) => state.variables.some((item) => item.name === name));
        if (names.length) {
          await request("api/workspace", { action: "clear-names", names, confirm: true });
          await waitIdle();
        }
      }
    } catch {}
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
