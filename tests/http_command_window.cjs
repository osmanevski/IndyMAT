const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");

(async () => {
  const { url } = JSON.parse(fs.readFileSync(".matlab-free/launch.json"));
  const [base, token] = url.split("#");
  const request = (route, data, headers = {}) => fetch(base + "api/" + route, { method: data === undefined ? "GET" : "POST", headers: { "X-MF-Language": "tr", "X-MF-Token": token, ...data === undefined ? {} : { "Content-Type": "application/json" }, ...headers }, body: data === undefined ? undefined : JSON.stringify(data) });
  const state = async () => await (await request("state")).json();
  const waitIdle = async (job) => {
    for (let index = 0; index < 400; index++) {
      let value = await state();
      if (value.status === "idle" && (!job || value.job === job)) return value;
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error("idle timeout");
  };
  const suffix = String(Date.now());
  const first = `http_command_window_${suffix}_a=11;`;
  const second = `http_command_window_${suffix}_b=22;`;
  const locationName = `http_command_window_${suffix}.m`;
  const locationPath = path.resolve("workspace", locationName);
  const capturePrefix = `http_command_window_capture_${suffix}`;
  const captureFiles = [];
  let initial = await (await request("history-detail")).json();
  fs.writeFileSync(locationPath, "value=1;\nerror('probe');\n");
  try {
    assert.equal((await fetch(base + "api/history-detail")).status, 403);
    assert.equal((await request("history-detail", undefined, { Origin: "https://evil.example" })).status, 403);
    let response = await request("execute", { mode: "code", code: first, history: true });
    assert.equal(response.status, 202);
    await waitIdle();
    response = await request("execute", { mode: "code", code: second, history: true });
    assert.equal(response.status, 202);
    let beforeMutation = await waitIdle();
    let detail = await (await request("history-detail")).json();
    let firstEntry = detail.entries.find((entry) => entry.code === first);
    let secondEntry = detail.entries.find((entry) => entry.code === second);
    assert(firstEntry && secondEntry);
    response = await request("history", { action: "delete", ids: [firstEntry.id] });
    assert.equal(response.status, 200);
    let afterDelete = await state();
    assert.equal(afterDelete.job, beforeMutation.job, "history deletion submitted a job");
    assert.equal(afterDelete.epoch, beforeMutation.epoch, "history deletion reset the session");
    let legacy = await (await request("history")).json();
    assert(!legacy.includes(first));
    assert(legacy.includes(second));
    response = await request("history", { action: "delete", ids: [secondEntry.id], code: "http_command_window_injected=1;" });
    assert.equal(response.status, 400);
    assert.equal((await state()).job, beforeMutation.job, "rejected history payload executed code");
    response = await request("error-location", { path: locationName, line: 2 });
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { path: locationPath, line: 2 });
    assert.equal((await request("error-location", { path: "../outside.m", line: 1 })).status, 403);
    assert.equal((await request("error-location", { path: locationName, line: 2, code: "disp(1)" })).status, 400);
    if (!initial.entries.length) {
      response = await request("history", { action: "clear", confirm: true });
      assert.equal(response.status, 200);
      assert.deepEqual(await (await request("history")).json(), []);
      const persisted = JSON.parse(fs.readFileSync(".matlab-free/history.json", "utf8"));
      assert.equal(persisted.version, 2);
      assert.deepEqual(persisted.entries, []);
    } else {
      response = await request("history", { action: "delete", ids: [secondEntry.id] });
      assert.equal(response.status, 200);
    }
    const captured = require("./fixtures/command_window_errors.json");
    for (const fixture of captured.cases) {
      const name = fixture.name.replace("cw_capture", capturePrefix);
      const file = path.resolve("workspace", name + ".m");
      if (fixture.source !== null) {
        fs.writeFileSync(file, fixture.source.replaceAll("cw_capture", capturePrefix).replace("first=1;", `${capturePrefix}_first=1;`));
        captureFiles.push(file);
      }
      const submitted = await request("execute", { mode: fixture.mode, code: fixture.code.replaceAll("cw_capture", capturePrefix), argument: fixture.mode === "file" ? file : "" });
      assert.equal(submitted.status, 202);
      const completed = await waitIdle((await submitted.json()).job);
      assert.equal(completed.version, captured.version);
      assert.equal(completed.output, fixture.output);
      assert.equal(completed.error.replaceAll(capturePrefix, "cw_capture").replaceAll(path.resolve("workspace"), "<workspace>"), fixture.error);
      // Print the actual HTTP response fields for orchestrator evidence.
      console.log(JSON.stringify({ case: fixture.name, version: completed.version, output: completed.output, error: completed.error }));
      for (const location of fixture.locations) {
        const candidate = location.path.replaceAll("cw_capture", capturePrefix).replace("<workspace>", path.resolve("workspace"));
        const resolved = await request("error-location", { path: candidate, line: location.line });
        if (candidate === "run") assert.equal(resolved.status, 404);
        else {
          assert.equal(resolved.status, 200);
          assert.deepEqual(await resolved.json(), { path: file, line: location.line });
        }
      }
      assert.equal((await state()).job, completed.job, "location resolution submitted a job");
      assert.equal(completed.epoch, beforeMutation.epoch, "error capture reset the session");
    }
    // Review 9: oversized history entries do not truncate or reject code jobs.
    const historyBeforeLarge = await (await request("history")).json();
    const largeCode = "% " + "x".repeat(100000) + `\n${capturePrefix}_large=73;`;
    const largeResponse = await request("execute", { mode: "code", code: largeCode, history: true });
    assert.equal(largeResponse.status, 202);
    const largeAccepted = await largeResponse.json();
    assert.equal(largeAccepted.history_recorded, false);
    const largeCompleted = await waitIdle(largeAccepted.job);
    assert(!largeCompleted.error, largeCompleted.error);
    assert(largeCompleted.variables.some((item) => item.name === capturePrefix + "_large" && item.preview === "73"));
    assert.deepEqual(await (await request("history")).json(), historyBeforeLarge);
    assert(fs.statSync(".matlab-free/history.json").size <= 1_000_000);
    console.log("HTTP COMMAND WINDOW PASS: guards, history, real caught errors, bounded locations and no execution by history/location routes.");
  } finally {
    fs.rmSync(locationPath, { force: true });
    for (const file of captureFiles) fs.rmSync(file, { force: true });
    let detail = await (await request("history-detail")).json().catch(() => ({ entries: [] }));
    let ids = detail.entries.filter((entry) => entry.code === first || entry.code === second).map((entry) => entry.id);
    if (ids.length) await request("history", { action: "delete", ids }).catch(() => {});
    let snapshot = await state().catch(() => ({}));
    if (snapshot.status === "idle") {
      await request("execute", { mode: "code", code: `clear http_command_window_${suffix}_a http_command_window_${suffix}_b ${capturePrefix}_first ${capturePrefix}_large ${capturePrefix}_function ${capturePrefix}_nested ${capturePrefix}_parse;` }).catch(() => {});
      await waitIdle().catch(() => {});
    }
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
