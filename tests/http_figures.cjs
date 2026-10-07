const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const { randomBytes } = require("node:crypto");

(async () => {
  // The gate starts a separate server for this file. Never log launch data.
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const [base, token] = launch.url.split("#");
  const job = randomBytes(16).toString("hex");
  const folder = path.resolve(".matlab-free/jobs", job);
  const cap = 8 * 1024 * 1024;
  const route = (file, identity = job) => "api/figure?job=" + encodeURIComponent(identity) + "&file=" + encodeURIComponent(file);
  const request = (file, headers = {}, identity = job) => fetch(base + route(file, identity), { headers: { "X-MF-Token": token, ...headers } });
  const png = fs.readFileSync("static/icon.png");
  const json = fs.readFileSync("tests/fixtures/figure_v3/surf_vector.json");
  fs.mkdirSync(folder, { recursive: true });
  try {
    fs.writeFileSync(path.join(folder, "figure-1.png"), png);
    fs.writeFileSync(path.join(folder, "figure-1.json"), json);
    let response = await request("figure-1.png");
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("content-type"), "image/png");
    assert.deepEqual(Buffer.from(await response.arrayBuffer()), png);
    response = await request("figure-1.json");
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("content-type"), "application/json; charset=utf-8");
    assert.deepEqual(Buffer.from(await response.arrayBuffer()), json);
    assert.equal(response.headers.get("cache-control"), "no-store");
    assert.equal(response.headers.get("x-content-type-options"), "nosniff");
    // Whitespace keeps this valid JSON at the exact raw UTF-8 boundary.
    const bounded = Buffer.alloc(cap, 32);
    json.copy(bounded);
    fs.writeFileSync(path.join(folder, "figure-2.json"), bounded);
    response = await request("figure-2.json");
    assert.equal(response.status, 200);
    assert.deepEqual(Buffer.from(await response.arrayBuffer()), bounded);
    fs.writeFileSync(path.join(folder, "figure-3.json"), "");
    fs.truncateSync(path.join(folder, "figure-3.json"), cap + 1);
    response = await request("figure-3.json");
    assert.equal(response.status, 413);
    const errorBytes = Buffer.from(await response.arrayBuffer());
    assert(errorBytes.length < 512, "oversized artifact did not get a small error");
    const error = JSON.parse(errorBytes.toString("utf8"));
    assert.equal(error.reason_code, "json_budget");
    assert.deepEqual(error.reason_args, { actual: cap + 1, limit: cap });
    assert.equal((await request("figure-1.png")).status, 200, "PNG fallback was lost after JSON refusal");
    assert.equal((await fetch(base + route("figure-1.json"))).status, 403);
    assert.equal((await request("figure-1.json", { "X-MF-Token": "invalid" })).status, 403);
    assert.equal((await request("figure-1.json", { Origin: "https://evil.example" })).status, 403);
    assert.equal((await request("figure-1.json", { Origin: new URL(base).origin })).status, 200);
    assert.equal((await request("figure-1.json", { "Sec-Fetch-Site": "cross-site" })).status, 403);
    const badHost = await new Promise((resolve, reject) => {
      const target = new URL(base + route("figure-1.json"));
      const call = http.request({ hostname: target.hostname, port: target.port, path: target.pathname + target.search,
        headers: { Host: "evil.example", "X-MF-Token": token } }, (result) => {
        result.resume();
        resolve(result.statusCode);
      });
      call.on("error", reject);
      call.end();
    });
    assert.equal(badHost, 403);
    for (const invalid of ["", "../" + job, "A".repeat(32), "f".repeat(31), "f".repeat(33), job + "/x"]) {
      assert.equal((await request("figure-1.json", {}, invalid)).status, 400);
    }
    for (const invalid of ["", "../figure-1.json", "figure-1.json/extra", "figure-a.json", "figure--1.json", "figure-1.svg", "figure-1.JSON", "figure-1.json\0"]) {
      assert.equal((await request(invalid)).status, 400);
    }
    assert.equal((await request("figure-99.json")).status, 404);
    assert.equal((await request("figure-99.png")).status, 404);
    assert.equal((await request("figure-1.json", {}, "0".repeat(32))).status, 404);
    fs.symlinkSync(path.resolve("static/icon.png"), path.join(folder, "figure-4.png"));
    assert.equal((await request("figure-4.png")).status, 403, "symlink escaped the artifact root");
    console.log("HTTP FIGURES PASS: artifact bytes, bounded JSON, PNG fallback and request guards.");
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
