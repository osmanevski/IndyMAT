const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const { chromium } = require("playwright");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json"));
  const [base, token] = launch.url.split("#");
  const suffix = String(Date.now());
  const stem = `ui_command_window_${suffix}`;
  const fixture = path.resolve("workspace", stem + ".m");
  fs.writeFileSync(fixture, `function value=${stem}()\n  value=missing_${stem};\nendfunction\n`);
  const captured = require("./fixtures/command_window_errors.json");
  const extraFixtures = captured.cases.filter((item) => ["cw_capture_nested", "cw_capture_parse", "cw_capture_script"].includes(item.name)).map((item) => {
    let name = item.name.replace("cw_capture", stem), file = path.resolve("workspace", name + ".m");
    fs.writeFileSync(file, item.source.replaceAll(item.name, name).replace("first=1;", `${stem}_script_first=1;`));
    return { ...item, name, file };
  });
  const api = (route, data) => fetch(base + "api/" + route, { method: data === undefined ? "GET" : "POST", headers: { "X-MF-Token": token, ...data === undefined ? {} : { "Content-Type": "application/json" } }, body: data === undefined ? undefined : JSON.stringify(data) });
  const state = async () => await (await api("state")).json();
  const waitIdle = async (job) => {
    for (let index = 0; index < 500; index++) {
      let value = await state();
      if (value.status === "idle" && (!job || value.job === job)) return value;
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error("engine idle timeout");
  };
  let browser, context;
  const errors = [];
  try {
    browser = await chromium.launch({ headless: true });
    context = await browser.newContext({ locale: "tr-TR", permissions: ["clipboard-read", "clipboard-write"] });
    let page = await context.newPage(), executeCount = 0, inputCount = 0;
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) => {
      let pathname = new URL(request.url()).pathname;
      if (pathname === "/api/execute" && request.method() === "POST") executeCount++;
      if (pathname === "/api/input" && request.method() === "POST") inputCount++;
    });
    await page.goto(launch.url);
    await page.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hazır");
    const submitAndWait = async (action = () => page.locator("#command").press("Enter")) => {
      const expectedEntries = Math.min(100, await page.locator("#console .console-entry").count() + 1);
      let accepted = page.waitForResponse((response) => new URL(response.url()).pathname === "/api/execute" && response.request().method() === "POST");
      await action();
      let response = await accepted;
      let result = await response.json();
      assert.equal(response.status(), 202, JSON.stringify(result));
      let completed = await waitIdle(result.job);
      await page.waitForFunction(({ output, error, count }) => {
        let entries = [...document.querySelectorAll("#console .console-entry")], entry = entries.at(-1);
        return document.querySelector("#status-text").textContent === "Hazır" &&
          entries.length === count &&
          entry?.querySelector(".console-error")?.textContent === (error || "") &&
          entry?.querySelector(".console-output:not(.console-error)")?.textContent === (output || "") &&
          !!entry?.querySelector(".console-time")?.textContent;
      }, { output: completed.output, error: completed.error, count: expectedEntries });
      return completed;
    };

    let before = executeCount;
    await page.locator("#command").fill("if true");
    await page.locator("#command").press("Enter");
    assert.equal(executeCount, before, "incomplete block executed");
    assert.equal(await page.locator("#command").inputValue(), "if true\n");
    await page.keyboard.insertText(`${stem}_multi=41;\nend`);
    await page.locator("#command").press("Enter");
    await page.waitForFunction((name) => document.querySelector("#status-text").textContent === "Hazır" && document.querySelector("#variables").innerText.includes(name), stem + "_multi");
    assert.equal(executeCount, before + 1, "multiline command was not one job");
    assert.equal(inputCount, 0, "multiline command leaked to stdin");
    let multilineHistory = await (await api("history")).json();
    assert(multilineHistory.includes(`if true\n${stem}_multi=41;\nend`));

    // Review 4: indexing's end cannot close the surrounding if block.
    const indexed = `if true\n${stem}_array=[1 2];\n${stem}_indexed=${stem}_array(:, end)`;
    await page.locator("#command").fill(indexed);
    const indexedBefore = executeCount;
    await page.locator("#command").press("Enter");
    assert.equal(await page.locator("#command").inputValue(), indexed + "\n");
    assert.equal(executeCount, indexedBefore);
    await page.keyboard.insertText("end");
    await submitAndWait();
    assert.equal(executeCount, indexedBefore + 1);
    const beforeShift = executeCount;

    await page.locator("#command").fill(`${stem}_shift=1;`);
    await page.locator("#command").press("Shift+Enter");
    await page.keyboard.insertText(`${stem}_shift=${stem}_shift+1;`);
    assert.equal(executeCount, beforeShift, "Shift+Enter executed instead of adding a line");
    await page.locator("#command").press("Enter");
    await page.waitForFunction((name) => document.querySelector("#status-text").textContent === "Hazır" && document.querySelector("#variables").innerText.includes(name), stem + "_shift");

    const recalled = `${stem}_plot_recall=7;`;
    await page.locator("#command").fill(recalled);
    await submitAndWait();
    await page.locator("#command").fill(`${stem}_other=8;`);
    await submitAndWait();
    await page.locator("#command").fill(`${stem}_plot`);
    await page.locator("#command").press("ArrowUp");
    assert.equal(await page.locator("#command").inputValue(), recalled);
    await page.locator("#command").press("ArrowDown");
    assert.equal(await page.locator("#command").inputValue(), `${stem}_plot`, "draft was not restored");

    const outputNeedle = `${stem}_output`;
    await page.locator("#command").fill(`disp('${outputNeedle}'); disp('${outputNeedle}');`);
    await page.locator("#command").press("Enter");
    await page.waitForFunction((needle) => document.querySelector("#status-text").textContent === "Hazır" && document.querySelector("#console").innerText.includes(needle), outputNeedle);
    await page.locator("#console-search").fill(outputNeedle);
    await page.waitForFunction(() => document.querySelectorAll("#console mark.console-match").length === 2);
    assert.equal(await page.locator("#console-match-count").innerText(), "1 / 2");
    await page.locator("#console-search-next").click();
    await page.waitForFunction(() => document.querySelector("#console-match-count").textContent === "2 / 2");
    assert.equal(await page.locator("#console-match-count").innerText(), "2 / 2");
    assert.equal(await page.locator("#console mark.console-match.current").count(), 1);
    await page.locator("#console-search").fill("");

    await page.locator("#command").fill(`${stem}();`);
    let failed = await submitAndWait();
    assert.equal(failed.error, `'missing_${stem}' undefined near line 2, column 9\n  ${stem}:2`);
    const currentEntry = page.locator("#console .console-entry").last();
    let link = currentEntry.locator(`.console-error .console-location[data-line="2"]`);
    await link.waitFor({ state: "visible" });
    assert.equal(await link.count(), 1);
    assert.equal(await link.getAttribute("data-path"), fixture);
    assert.equal(await link.innerText(), `${stem}:2`);
    await link.click();
    await page.waitForFunction((name) => document.querySelector("#editor-label").textContent === name && document.querySelector("#cursor").textContent.startsWith("Satır 2,"), stem + ".m");

    for (const item of extraFixtures) {
      if (item.mode === "file") {
        await page.locator("#refresh-files").click();
        await page.locator("#files").getByRole("button", { name: item.name + ".m", exact: true }).click();
        await page.waitForFunction((name) => document.querySelector("#editor-label").textContent === name, item.name + ".m");
        failed = await submitAndWait(() => page.locator("#run").click());
      } else {
        await page.locator("#command").fill(`${item.name}();`);
        failed = await submitAndWait();
      }
      assert(failed.error, item.name + " did not fail");
      const targetLine = item.name.endsWith("_nested") ? 4 : 2;
      let target = currentEntry.locator(`.console-error .console-location[data-line="${targetLine}"]`);
      await target.waitFor({ state: "visible" });
      assert.equal(await target.count(), 1, item.name);
      assert.equal(await target.getAttribute("data-path"), item.file);
      await target.click();
      await page.waitForFunction(({ name, line }) => document.querySelector("#editor-label").textContent === name && document.querySelector("#cursor").textContent.startsWith(`Satır ${line},`), { name: item.name + ".m", line: targetLine });
      assert.equal(await currentEntry.getByRole("button", { name: "run:78", exact: true }).count(), 0, "unresolved internal frame was linked");
    }
    await page.locator("#command").fill(`error('command line failure'); % ${stem}`);
    failed = await submitAndWait();
    assert.equal(failed.error, "command line failure");
    assert.equal(await currentEntry.locator(".console-location").count(), 0);

    await page.locator("#history-search").fill(`${stem}_plot_recall`);
    let row = page.locator("#history .history-row").filter({ has: page.getByRole("button", { name: recalled, exact: true }) });
    await row.locator('input[type="checkbox"]').check();
    let runBefore = executeCount;
    await submitAndWait(() => page.locator("#history-run").click());
    assert.equal(executeCount, runBefore + 1, "selected history did not run exactly once");
    await row.locator('input[type="checkbox"]').check();
    await page.locator("#history-copy").click();
    await page.waitForFunction(() => document.querySelector("#toast").textContent === "Seçili komutlar panoya kopyalandı.");
    assert.equal(await page.evaluate(() => navigator.clipboard.readText()), recalled);
    let deleteBefore = executeCount;
    let deleted = page.waitForResponse((response) => new URL(response.url()).pathname === "/api/history" && response.request().method() === "POST");
    await page.locator("#history-delete").click();
    assert.equal((await deleted).status(), 200);
    await page.waitForFunction((command) => ![...document.querySelectorAll(".history-item")].some((node) => node.textContent === command), recalled);
    assert.equal(executeCount, deleteBefore, "history deletion executed a command");
    assert.equal(inputCount, 0);
    assert.deepEqual(errors, []);

    // Review 3: deliberately synthetic DOM stress fixture, not an Octave result.
    // Measure the input task and a heartbeat while decorating a full 1 MB.
    await page.locator("#clear-console").click();
    await page.evaluate(() => {
      const entry = document.createElement("div"), output = document.createElement("pre");
      entry.className = "console-entry";
      output.className = "console-output";
      output.textContent = "a".repeat(1_000_000);
      entry.append(output);
      document.querySelector("#console").append(entry);
    });
    const perf = await page.evaluate(async () => {
      const input = document.querySelector("#console-search");
      const start = performance.now();
      input.value = "a";
      input.dispatchEvent(new Event("input"));
      const handlerMs = performance.now() - start;
      await new Promise((resolve) => setTimeout(resolve, 0));
      return { handlerMs, heartbeatMs: performance.now() - start };
    });
    await page.waitForFunction(() => document.querySelector("#console-match-count").textContent === "1 / 1000+");
    assert.equal(await page.locator("#console mark").count(), 1000);
    assert(perf.handlerMs < 50, `search input handler blocked for ${perf.handlerMs} ms`);
    assert(perf.heartbeatMs < 250, `search blocked the page heartbeat for ${perf.heartbeatMs} ms`);
    console.log("COMMAND WINDOW 1 MB BROWSER TIMING " + JSON.stringify(perf));
    await page.locator("#console-search").fill("");
    await page.evaluate(() => {
      for (let i = 0; i < 3; i++) {
        const entry = document.createElement("div"), output = document.createElement("pre");
        entry.className = "console-entry";
        output.className = "console-output";
        output.textContent = "ğ".repeat(250000);
        entry.append(output);
        document.querySelector("#console").append(entry);
      }
    });
    await page.waitForFunction(() => new TextEncoder().encode(document.querySelector("#console").textContent).length <= 1_000_000);
    console.log("UI COMMAND WINDOW PASS: multiline jobs, prefix draft recall, output search, error navigation and history actions.");
  } finally {
    if (context) await context.close().catch(() => {});
    if (browser) await browser.close().catch(() => {});
    fs.rmSync(fixture, { force: true });
    for (const item of extraFixtures) fs.rmSync(item.file, { force: true });
    let detail = await (await api("history-detail")).json().catch(() => ({ entries: [] }));
    let ids = detail.entries.filter((entry) => entry.code.includes(stem)).map((entry) => entry.id);
    if (ids.length) await api("history", { action: "delete", ids }).catch(() => {});
    let snapshot = await state().catch(() => ({}));
    if (snapshot.status === "idle") {
      await api("execute", { mode: "code", code: `clear ${stem}_array ${stem}_indexed ${stem}_multi ${stem}_shift ${stem}_plot_recall ${stem}_other ${stem}_script_first ${stem} ${extraFixtures.map((item) => item.name).join(" ")};` }).catch(() => {});
      await waitIdle().catch(() => {});
    }
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
