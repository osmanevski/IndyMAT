const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const { randomUUID } = require("node:crypto");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const [base, token] = launch.url.split("#");
  const request = (route, headers = {}) => fetch(base + route, { headers: { "X-MF-Language": "tr", "X-MF-Token": token, ...headers } });
  const post = (route, body) => fetch(base + route, { method: "POST", headers: { "X-MF-Language": "tr", "X-MF-Token": token, "Content-Type": "application/json" }, body: JSON.stringify(body) });
  const state = async () => await (await request("api/state")).json();
  const waitIdle = async () => {
    for (let index = 0; index < 200; index++) {
      const snapshot = await state();
      if (snapshot.status === "idle") return snapshot;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error("Octave job timeout");
  };
  const initial = await waitIdle();
  const unique = "editor_intel_" + randomUUID().replaceAll("-", "");
  const folder = path.join(initial.workspace, unique);
  const outside = path.join(os.tmpdir(), unique + "_outside.m");
  fs.mkdirSync(folder);
  fs.writeFileSync(path.join(folder, "a.m"), `function y = ${unique}(x)\ntext = '${unique}';\n% ${unique}\ny = ${unique}(x);\nend\n`);
  fs.mkdirSync(path.join(folder, "nested"));
  fs.writeFileSync(path.join(folder, "nested", "nested.m"), "function nested_only\nend\n");
  fs.writeFileSync(outside, "function outside_only\nend\n");
  fs.symlinkSync(outside, path.join(folder, "linked.m"));
  try {
    let response = await post("api/folder", { path: folder });
    assert.equal(response.status, 202);
    const before = await waitIdle();
    assert.equal((await fetch(base + "api/symbols")).status, 403, "tokenless symbol index was accepted");
    assert.equal((await request("api/symbols", { Origin: "https://evil.example" })).status, 403);
    const badHost = await new Promise((resolve, reject) => {
      const target = new URL(base + "api/symbols");
      const call = http.request({ hostname: target.hostname, port: target.port, path: target.pathname, headers: { Host: "evil.example", "X-MF-Language": "tr", "X-MF-Token": token } }, (result) => {
        result.resume();
        resolve(result.statusCode);
      });
      call.on("error", reject);
      call.end();
    });
    assert.equal(badHost, 403);
    assert.equal((await request("api/symbols?path=/tmp")).status, 400, "path-bearing index request was accepted");
    response = await request("api/symbols");
    assert.equal(response.status, 200);
    let body = await response.json();
    assert(body.functions.some((item) => item.name === unique && item.path === path.join(folder, "a.m")));
    assert(!body.functions.some((item) => ["nested_only", "outside_only"].includes(item.name)), "index recursed or followed a symlink");
    response = await request("api/symbols?name=" + unique);
    assert.equal(response.status, 200);
    body = await response.json();
    assert.deepEqual(body.occurrences.map((item) => [path.basename(item.path), item.line]), [["a.m", 1], ["a.m", 4]]);
    assert.equal((await request("api/symbols?name=bad-name")).status, 400);
    for (let index = 0; index < 205; index++) fs.writeFileSync(path.join(folder, `z${String(index).padStart(3, "0")}.m`), `function z${index}\nend\n`);
    body = await (await request("api/symbols")).json();
    assert.equal(body.scanned_files, 200);
    assert.equal(body.truncated, true);
    assert.deepEqual(body.limits, { entries: 2000, files: 200, file_bytes: 500000, total_bytes: 2000000, occurrences: 2000, functions: 2000, result_bytes: 262144, time_ms: 500 });
    // Review #7: a small source can expand into a much larger symbol response.
    const denseFile = path.join(folder, "00-dense.m");
    fs.writeFileSync(denseFile, "function f\n".repeat(40000));
    response = await request("api/symbols");
    assert.equal(response.status, 200);
    let raw = Buffer.from(await response.arrayBuffer());
    body = JSON.parse(raw.toString("utf8"));
    assert(body.truncated);
    assert(body.functions.length > 0 && body.functions.length <= body.limits.functions);
    assert(raw.length <= body.limits.result_bytes, "serialized response exceeded byte cap");
    const longName = "f".repeat(1000);
    fs.writeFileSync(denseFile, `function ${longName}\n`.repeat(400));
    response = await request("api/symbols?name=" + longName);
    assert.equal(response.status, 200);
    raw = Buffer.from(await response.arrayBuffer());
    body = JSON.parse(raw.toString("utf8"));
    assert(body.truncated);
    assert(body.functions.length > 0 && body.occurrences.length > 0);
    assert(body.functions.length < body.limits.functions);
    assert(raw.length <= body.limits.result_bytes, "combined functions/occurrences exceeded byte cap");
    const after = await state();
    assert.equal(after.job, before.job, "symbol indexing changed the persistent session job");
    assert.equal(after.epoch, before.epoch, "symbol indexing reset the persistent session");
    assert.equal(after.cwd, before.cwd, "symbol indexing changed the persistent session folder");
    console.log("HTTP EDITOR INTEL PASS: guarded current-folder index, no recursion/symlinks, hard limits and unchanged Octave identity.");
  } finally {
    try {
      await post("api/folder", { path: initial.workspace });
      await waitIdle();
    } catch {}
    fs.rmSync(folder, { recursive: true, force: true });
    fs.rmSync(outside, { force: true });
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
