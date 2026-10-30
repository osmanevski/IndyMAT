// The engine setting over HTTP on a real server: Octave by default, a guarded
// switch to the Rust engine and back, and an Octave state with no trace of it.
const assert = require("node:assert/strict");
const fs = require("node:fs");
(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const [base, token] = launch.url.split("#");
  const headers = { "X-MF-Token": token, "Content-Type": "application/json" };
  const get = async (path) => { const r = await fetch(base + "api/" + path, { headers }); return { status: r.status, body: await r.json() }; };
  const post = async (path, data) => { const r = await fetch(base + "api/" + path, { method: "POST", headers, body: JSON.stringify(data) }); return { status: r.status, body: await r.json() }; };
  const idle = async (job) => {
    for (let i = 0; i < 600; i++) {
      const { body } = await get("state");
      if (body.status === "idle" && (!job || body.job === job)) return body;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error("session did not become idle");
  };
  const run = async (code) => {
    const { status, body } = await post("execute", { code, mode: "code" });
    assert.equal(status, 202, JSON.stringify(body));
    return idle(body.job);
  };
  let info = (await get("engine")).body;
  assert.equal(info.engine, "octave", "GNU Octave is the default");
  assert.equal(info.available.octave, true);
  assert.equal((await fetch(base + "api/engine")).status, 403, "the token is required");
  const octave = await idle();
  assert.ok(/^\d/.test(octave.version), octave.version);
  for (const key of ["engine", "unsupported", "engine_notice"]) assert.equal(key in octave, false, "Octave state has no " + key);
  assert.equal((await post("engine", { engine: "rust" })).status, 400, "confirmation is required");
  assert.equal((await post("engine", { engine: "rust", confirm: "yes" })).status, 400);
  assert.equal((await post("engine", { engine: "matlab", confirm: true })).status, 400);
  assert.equal(fs.existsSync(".matlab-free/engine.json"), false, "nothing is saved before a choice");
  if (!info.available.rust) {
    const refused = await post("engine", { engine: "rust", confirm: true });
    assert.equal(refused.status, 400);
    assert.match(refused.body.error, /cd motor && cargo build --release/);
    assert.equal((await get("engine")).body.engine, "octave");
    console.log("ENGINE HTTP PASS (Rust engine not built: refused with the build command)");
    return;
  }
  await run("kept_in_octave = 1;");
  try {
    // Polling meets the closing kernel during a switch: once it has begun, nothing may read as dead or as the old session.
    const watch = async (request, target) => {
      let done = false;
      const seen = [];
      const answer = request.finally(() => { done = true; });
      while (!done) {
        const during = (await get("state")).body;
        if (!done) seen.push({ status: during.status, rust: !!during.engine });
        await new Promise((resolve) => setTimeout(resolve, 5));
      }
      const begun = seen.findIndex((item) => item.status === "starting");
      const stale = begun < 0 ? [] : seen.slice(begun).filter((item) => item.status !== "starting" && item.rust !== (target === "rust"));
      assert.deepEqual(stale, [], "the old session during the switch");
      assert.equal(seen.some((item) => item.status === "dead"), false, "no dead state during the switch");
      return { answer: await answer, seen };
    };
    const { answer: switched } = await watch(post("engine", { engine: "rust", confirm: true }), "rust");
    assert.equal(switched.status, 200, JSON.stringify(switched.body));
    assert.equal(switched.body.engine, "rust");
    assert.equal(JSON.parse(fs.readFileSync(".matlab-free/engine.json", "utf8")).engine, "rust");
    let rust = await idle();
    assert.ok(rust.epoch > octave.epoch, "the epoch rises across a switch");
    assert.equal(rust.engine.stub, false);
    assert.equal(rust.variables.length, 0, "variables are cleared");
    for (const feature of ["packages", "publish", "profiler", "help", "lint"]) assert.ok(rust.unsupported.includes(feature), feature);
    rust = await run("A = magic(3)");
    assert.match(rust.output, /8\s+1\s+6/);
    assert.deepEqual(rust.variables.map((item) => [item.name, item.size]), [["A", "3x3"]]);
    rust = await run("plot(1:3)");
    assert.equal(rust.figures.length, 1);
    const figure = rust.figures[0];
    assert.equal(figure.interactive, true);
    const data = await fetch(base + `api/figure?job=${figure.job}&file=${figure.data_file}`, { headers });
    assert.equal(data.status, 200);
    assert.equal((await data.json()).supported, true);
    assert.equal((await post("package-session", { action: "load", name: "signal" })).status, 400, "an unsupported feature is refused, not faked");
    const { answer: back, seen: seenBack } = await watch(post("engine", { engine: "octave", confirm: true }), "octave");
    assert.ok(seenBack.some((item) => item.status === "starting"), "Octave takes long enough to start that a poll sees it");
    assert.equal(back.status, 200, JSON.stringify(back.body));
    const again = await idle();
    assert.ok(again.epoch > rust.epoch);
    assert.equal(again.variables.length, 0);
    for (const key of ["engine", "unsupported"]) assert.equal(key in again, false, "back on Octave: no " + key);
    assert.match((await run("B = 2")).output, /B = 2/);
    assert.equal(JSON.parse(fs.readFileSync(".matlab-free/engine.json", "utf8")).engine, "octave");
    console.log("ENGINE HTTP PASS");
  } finally {
    await post("engine", { engine: "octave", confirm: true });
    fs.rmSync(".matlab-free/engine.json", { force: true });
  }
})().catch((error) => { console.error(error); process.exit(1); });
