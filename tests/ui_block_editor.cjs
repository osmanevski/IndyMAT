"use strict";
// Real browser, real mouse and keyboard: the block diagram editor opens a
// .imbd.json file from the Current Folder, edits the PID loop (drag at zoom,
// wires, palette, all 13 types, Sum signs, keyboard move/wire/delete,
// undo/redo), saves through the hash-checked save, reopens, runs it on the
// real Octave session, checks the plotted line against im_example_model through
// Octave, stops a run, selects the block named by a compile error, switches to
// text and back, refuses a broken file and a wrong Save As name, and renders
// 200 blocks. Measurements go to INDYMAT_BLOCK_LOG when set.
const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const { chromium, expect } = require("@playwright/test");

const LOG = process.env.INDYMAT_BLOCK_LOG || "";
const SHOTS = process.env.INDYMAT_BLOCK_SHOTS || "";
const workspace = path.resolve("workspace");
const file = path.join(workspace, "block_editor_ui.imbd.json");
const bad = path.join(workspace, "block_editor_ui_bad.imbd.json");
const big = path.join(workspace, "block_editor_ui_200.imbd.json");
const example = fs.readFileSync(path.resolve("workspace/examples/pid_kapali_cevrim.imbd.json"), "utf8");
const measurements = [];
const note = (line) => { measurements.push(line); console.log(line); };

function bigDiagram() {
  const blocks = [], connections = [], layout = {};
  for (let k = 0; k < 200; k++) {
    const id = "b" + k;
    blocks.push(k === 0 ? { id, type: "Sine", params: { amplitude: 1, omega: 1, phase: 0, bias: 0 }, sampleTime: 0 } : k === 199 ? { id, type: "Scope", params: {}, sampleTime: 0 } : { id, type: "Gain", params: { gain: 1 }, sampleTime: 0 });
    if (k) connections.push({ from: { block: "b" + (k - 1), port: 1 }, to: { block: id, port: 1 } });
    layout[id] = { x: (k % 20) * 170, y: Math.floor(k / 20) * 90 };
  }
  const simulation = { mode: "continuous", t0: 0, tf: 1, solver: "ode45", outputDt: 0.1, relTol: 1e-6, absTol: 1e-8, maxStep: 0.1 };
  return JSON.stringify({ imbVersion: 1, model: { schemaVersion: 1, blocks, connections, simulation }, layout: { blocks: layout } });
}

(async () => {
  fs.writeFileSync(file, example);
  const broken = JSON.parse(example);
  broken.layout.view = { zoom: 1 };
  fs.writeFileSync(bad, JSON.stringify(broken, null, 2));
  fs.writeFileSync(big, bigDiagram());
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ locale: "tr-TR", viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => { errors.push(error.message); console.error(error.stack); });
  const dialogs = [];
  let dismissNext = false;
  page.on("dialog", (dialog) => {
    dialogs.push(dialog.message());
    if (dismissNext) {
      dismissNext = false;
      return dialog.dismiss();
    }
    return dialog.accept();
  });
  const idle = () => page.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hazır", null, { timeout: 60000 });
  const hook = (name, ...args) => page.locator("#block-editor").evaluate((node, { name, args }) => node.blockEditorTest[name](...args), { name, args });
  const core = () => page.locator("#block-editor").evaluate((node) => {
    const c = node.blockEditorTest.core();
    return { blocks: c.blocks.map((b) => b.id), connections: c.connections.map((l) => `${l.from.block}>${l.to.block}:${l.to.port}`), layout: c.layout, model: c.model(), history: c.history.length };
  });
  const centreOf = async (id) => {
    const r = await hook("screenOf", id);
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  };
  const dragMouse = async (from, to, steps = 12) => {
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps });
    await page.mouse.up();
  };
  const clickBlock = async (id) => {
    const c = await centreOf(id);
    await page.mouse.click(c.x, c.y);
  };
  const openFromFolder = async (name) => {
    await page.locator("#files").getByRole("button", { name, exact: true }).click();
  };
  const command = async (code) => {
    const response = page.waitForResponse((reply) => reply.url().endsWith("/api/execute") && reply.request().method() === "POST");
    await page.locator("#command").fill(code);
    await page.locator("#command").press("Enter");
    const { job } = await (await response).json();
    await idle();
    return page.locator(`.console-entry[data-job="${job}"]`);
  };
  try {
    await page.goto(launch.url);
    await idle();
    await page.locator("#folder-address").fill(workspace);
    await page.locator("#folder-address").press("Enter");
    await idle();

    // ---- open from the Current Folder ---------------------------------------
    await openFromFolder("block_editor_ui.imbd.json");
    await expect(page.locator("#block-editor")).toBeVisible();
    await expect(page.locator("#editor")).toBeHidden();
    await expect(page.locator("#code-issues")).toHaveText("Blok diyagramı");
    await expect(page.locator("#lines")).toHaveText("11 blok · 14 bağlantı");
    await expect(page.locator("#save-state")).toHaveText("Kaydedildi");
    await expect(page.locator("#block-editor .block-node")).toHaveCount(11);
    await expect(page.locator("#block-type option")).toHaveCount(13);
    await expect(page.locator("#clear-console")).toBeVisible();
    await expect(page.locator('[data-action="save-as"]')).toBeVisible();
    note(`render 11 blocks: ${(await hook("render")).toFixed(2)} ms`);

    // The feedback wire plant -> error runs below every block.
    const feedback = await page.evaluate(() => {
      const wires = [...document.querySelectorAll("#block-editor .block-wire.feedback .block-wire-line")];
      const nodes = [...document.querySelectorAll("#block-editor .block-node rect")].map((r) => r.getBoundingClientRect().bottom);
      return { count: wires.length, bottom: Math.max(...wires.map((w) => w.getBoundingClientRect().bottom)), lowest: Math.max(...nodes) };
    });
    assert.equal(feedback.count, 1, "one feedback wire");
    assert.ok(feedback.bottom > feedback.lowest + 4, `feedback wire runs below the blocks: ${JSON.stringify(feedback)}`);

    // ---- drag drift at three zoom levels: cursor 100 px, block 100 px ----------
    for (const k of [0.5, 1.1, 2]) {
      await hook("centre", "integral", k);
      const before = await hook("screenOf", "integral"), start = { x: before.x + 20, y: before.y + 10 };
      const history = (await core()).history;
      await dragMouse(start, { x: start.x + 100, y: start.y + 37 });
      const after = await hook("screenOf", "integral");
      const dx = after.x - before.x, dy = after.y - before.y;
      note(`drag at zoom ${k}: cursor 100.0 x 37.0 px, block ${dx.toFixed(2)} x ${dy.toFixed(2)} px`);
      assert.ok(Math.abs(dx - 100) <= 0.5 * k + 0.6 && Math.abs(dy - 37) <= 0.5 * k + 0.6, `drift at zoom ${k}: ${dx}, ${dy}`);
      assert.equal((await core()).history, history + 1, "one undo entry per drag");
      await page.keyboard.press("ControlOrMeta+z");
      const back = await hook("screenOf", "integral");
      assert.ok(Math.abs(back.x - before.x) < 0.01 && Math.abs(back.y - before.y) < 0.01, "undo returns the block");
    }
    await page.locator("#block-fit").click();

    // ---- parameter panel: numbers only ------------------------------------------
    await clickBlock("proportional");
    await expect(page.locator("#cursor")).toHaveText("Seçili: proportional");
    const gain = page.locator('#block-panel input[data-param="gain"]');
    await gain.fill("pi");
    await gain.press("Enter");
    await expect(gain.locator("xpath=following-sibling::span")).toContainText("yalnız sayı");
    await expect(gain).toHaveAttribute("aria-invalid", "true");
    await gain.fill("5");
    await gain.press("Enter");
    await expect(page.locator("#save-state")).toHaveText("Kaydedilmemiş değişiklikler");
    assert.equal((await core()).model.blocks.find((b) => b.id === "proportional").params.gain, 5);

    // ---- wires with the mouse ---------------------------------------------------
    let state = await core();
    const igIndex = state.connections.indexOf("integral>integral_gain:1");
    const wire = await hook("wirePoint", igIndex);
    await page.mouse.click(wire.x, wire.y);
    await expect(page.locator("#cursor")).toHaveText(`Seçili: ${igIndex + 1}. bağlantı`);
    await page.keyboard.press("Delete");
    await expect(page.locator("#lines")).toHaveText("11 blok · 13 bağlantı");
    await dragMouse(await hook("portScreen", "integral", "out", 1), await hook("portScreen", "integral_gain", "in", 1));
    await expect(page.locator("#lines")).toHaveText("11 blok · 14 bağlantı");
    assert.ok((await core()).connections.includes("integral>integral_gain:1"), "wire drawn by mouse");
    // An occupied input refuses a second driver.
    await dragMouse(await hook("portScreen", "reference", "out", 1), await hook("portScreen", "error", "in", 1));
    await expect(page.locator("#block-status")).toContainText("zaten bağlı");
    await expect(page.locator("#lines")).toHaveText("11 blok · 14 bağlantı");

    // ---- palette: every one of the 13 types, Delete and undo/redo ---------------
    const types = await page.locator("#block-type option").evaluateAll((options) => options.map((o) => o.value));
    assert.deepEqual(types, ["Constant", "Step", "Sine", "Gain", "Sum", "Saturation", "Integrator", "TransferFcn", "Mux", "Scope", "UnitDelay", "SampledInput", "ClampingGate"]);
    for (const type of types) {
      await page.locator("#block-type").selectOption(type);
      await page.locator("#block-add").click();
    }
    await expect(page.locator("#block-editor .block-node")).toHaveCount(24);
    state = await core();
    assert.deepEqual(state.model.blocks.slice(11).map((b) => b.type), types);
    assert.ok(state.model.blocks.slice(11).every((b) => b.sampleTime === 0), "new blocks follow the continuous sample time");
    await expect(page.locator("#cursor")).toHaveText("Seçili: clampingGate1");
    await page.keyboard.press("Delete");
    await expect(page.locator("#block-editor .block-node")).toHaveCount(23);
    await page.keyboard.press("ControlOrMeta+z");
    await expect(page.locator("#block-editor .block-node")).toHaveCount(24);
    await page.keyboard.press("ControlOrMeta+Shift+z");
    await expect(page.locator("#block-editor .block-node")).toHaveCount(23);
    for (let k = 0; k < 14; k++) await page.locator("#undo").click();
    await expect(page.locator("#block-editor .block-node")).toHaveCount(11);
    assert.equal((await core()).model.blocks.find((b) => b.id === "proportional").params.gain, 5, "undo stopped before the gain edit");

    // ---- variable input count: Sum signs ---------------------------------------
    await clickBlock("control");
    const signs = page.locator('#block-panel input[data-param="signs"]');
    await signs.fill("1 1");
    await signs.press("Enter");
    await expect(page.locator("#lines")).toHaveText("11 blok · 13 bağlantı");
    assert.ok(!(await core()).connections.includes("derivative_gain>control:3"), "removed input drops its wire");
    await clickBlock("control");
    await page.keyboard.press("ControlOrMeta+z");
    await expect(page.locator("#lines")).toHaveText("11 blok · 14 bağlantı");
    assert.ok((await core()).connections.includes("derivative_gain>control:3"), "undo restores the wire");

    // ---- keyboard: focus, move, delete wire, draw wire ----------------------------
    const ref = page.locator('#block-editor .block-node[data-id="reference"]');
    await ref.focus();
    await expect(page.locator("#cursor")).toHaveText("Seçili: reference");
    const p0 = (await core()).layout.reference;
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Shift+ArrowRight");
    assert.deepEqual((await core()).layout.reference, { x: p0.x + 1, y: p0.y + 20 });
    state = await core();
    await page.locator(`#block-editor .block-wire[data-index="${state.connections.indexOf("integral>integral_gain:1")}"]`).focus();
    await page.keyboard.press("Delete");
    await expect(page.locator("#lines")).toHaveText("11 blok · 13 bağlantı");
    await page.locator('#block-editor .block-node[data-id="integral"]').focus();
    await page.keyboard.press("w");
    await expect(page.locator("#block-status")).toContainText("Hedef: integral_gain girişi 1");
    await page.keyboard.press("Enter");
    await expect(page.locator("#lines")).toHaveText("11 blok · 14 bağlantı");
    assert.ok((await core()).connections.includes("integral>integral_gain:1"), "wire drawn by keyboard");
    await page.locator('#block-editor .block-node[data-id="reference"]').focus();
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    assert.deepEqual((await core()).layout.reference, p0);

    // ---- save, close, reopen --------------------------------------------------
    await page.keyboard.press("ControlOrMeta+s");
    await expect(page.locator("#save-state")).toHaveText("Kaydedildi");
    const saved = fs.readFileSync(file, "utf8");
    const savedModel = JSON.parse(saved).model;
    assert.deepEqual(savedModel, (await core()).model, "the file holds the editor's model");
    assert.equal(savedModel.blocks.find((b) => b.id === "proportional").params.gain, 5);
    const expected = JSON.parse(example).model;
    expected.blocks.find((b) => b.id === "proportional").params.gain = 5;
    const sorted = (model) => ({ ...model, connections: model.connections.map((l) => JSON.stringify(l)).sort() });
    assert.deepEqual(sorted(savedModel), sorted(expected), "after all edits the model is the PID loop with Kp=5 (wire order aside)");
    await page.locator(".editor-tab.active .close-tab").click();
    await openFromFolder("block_editor_ui.imbd.json");
    await expect(page.locator("#block-editor")).toBeVisible();
    await expect(page.locator("#save-state")).toHaveText("Kaydedildi");
    assert.deepEqual((await core()).model, savedModel, "reopened model");

    // ---- run on the real session, compare the plot through Octave -----------------
    await command("keep_me = 731; ans = 42;");
    const submitted = page.waitForResponse((reply) => reply.url().endsWith("/api/execute") && reply.request().method() === "POST");
    await page.locator("#run").click();
    const accepted = await (await submitted).json();
    await idle();
    const entry = page.locator(`.console-entry[data-job="${accepted.job}"]`);
    await expect(entry.locator(".console-command")).toHaveText(">> Blok diyagramını çalıştır block_editor_ui.imbd.json");
    await expect(entry.locator(".console-generated summary")).toHaveText("Üretilen kod");
    await expect(entry.locator(".console-generated pre")).toContainText("block_editor_ui_result = (@im_simulate)('{\"schemaVersion\":1,");
    await expect(page.locator("#block-status")).toHaveText("Benzetim bitti.");
    await expect(page.locator("#figure-caption")).toContainText("block_editor_ui.imbd.json: output", { timeout: 20000 });
    const check = await command("ref=(@im_simulate)((@im_example_model)('pid_kapali_cevrim',struct('gains',[5 1 1]))); f=findobj(0,'type','figure','-regexp','name','^block_editor_ui\\.imbd\\.json: output$'); assert(numel(f)==1); ln=findobj(f,'type','line'); assert(isequal(get(ln,'ydata')(:),ref.signals.output)); assert(isequal(get(ln,'xdata')(:),ref.t)); assert(isequal(block_editor_ui_result.signals.output,ref.signals.output)); assert(ans==42 && keep_me==731); printf('PLOT MATCH %d\\n', numel(ref.t)); clear ref f ln;");
    await expect(check.locator(".console-output").first()).toContainText("PLOT MATCH 4001");
    await expect(check.locator(".console-error")).toHaveText("");

    // ---- Stop during a run -----------------------------------------------------
    const second = page.waitForResponse((reply) => reply.url().endsWith("/api/execute") && reply.request().method() === "POST");
    await page.locator("#run").click();
    const stopped = await (await second).json();
    await expect(page.locator("#stop")).toBeEnabled();
    await page.waitForTimeout(300);
    await page.locator("#stop").click();
    await idle();
    const stopState = await page.evaluate(async (job) => (await (await fetch("/api/state", { headers: { "X-MF-Token": sessionStorage.getItem("mf-token") } })).json()), stopped.job);
    assert.equal(stopState.job, stopped.job);
    note(`stop: job ${stopped.job} ended in ${stopState.elapsed?.toFixed(3)} s, error ${JSON.stringify((stopState.error || "").split("\n")[0])}`);
    assert.ok(stopState.error && stopState.elapsed < 3, "Stop ended the run before the simulation finished");
    await expect(page.locator("#block-status")).toContainText("Çalıştırma başarısız");
    const kept = await command("assert(exist('block_editor_ui_result','var')==1); printf('KEPT %d\\n', numel(block_editor_ui_result.t));");
    await expect(kept.locator(".console-output").first()).toContainText("KEPT 4001");

    // ---- a compile error selects the named block ------------------------------------
    state = await core();
    const refWire = await hook("wirePoint", state.connections.indexOf("reference>error:1"));
    await page.mouse.click(refWire.x, refWire.y);
    await page.keyboard.press("Delete");
    await clickBlock("plant");
    await page.locator("#run").click();
    await idle();
    await expect(page.locator("#block-status")).toContainText("Block error input 1 has no driver.");
    await expect(page.locator("#cursor")).toHaveText("Seçili: error");
    await expect(page.locator('#block-editor .block-node[data-id="error"]')).toHaveClass(/failed/);
    await expect(page.locator('#block-panel input[data-param="id"]')).toHaveValue("error");
    await page.locator('#block-editor .block-node[data-id="error"]').focus();
    await page.keyboard.press("ControlOrMeta+z");
    await expect(page.locator("#lines")).toHaveText("11 blok · 14 bağlantı");
    await page.keyboard.press("ControlOrMeta+s");
    await expect(page.locator("#save-state")).toHaveText("Kaydedildi");

    // ---- open as text and back -------------------------------------------------
    await page.locator("#block-open-text").click();
    await expect(page.locator("#editor")).toBeVisible();
    await expect(page.locator("#block-editor")).toBeHidden();
    await expect(page.locator(".cm-content")).toContainText('"imbVersion": 1');
    await expect(page.locator("#open-as-diagram")).toBeVisible();
    await page.locator("#open-as-diagram").click();
    await expect(page.locator("#block-editor")).toBeVisible();
    await expect(page.locator("#save-state")).toHaveText("Kaydedildi");

    // ---- Save As keeps the diagram suffix ------------------------------------------
    await page.locator('[data-action="save-as"]').click();
    await page.locator("#modal-body input").fill("block_editor_ui_copy.json");
    await page.locator("#modal-body button[type=submit]").click();
    await expect(page.locator("#toast")).toContainText(".imbd.json ile bitmeli");
    assert.ok(!fs.existsSync(path.join(workspace, "block_editor_ui_copy.json")), "no file written under a wrong name");

    // ---- a broken file opens as text with the reason, repaired in place ------------
    await openFromFolder("block_editor_ui_bad.imbd.json");
    await expect(page.locator("#editor")).toBeVisible();
    await expect(page.locator("#block-editor")).toBeHidden();
    await expect(page.locator("#diagram-note")).toContainText("Metin olarak açıldı: layout desteklenmeyen view alanını içeriyor.");
    await page.locator(".cm-content").fill(example);
    await page.locator("#open-as-diagram").click();
    await expect(page.locator("#block-editor")).toBeVisible();
    await expect(page.locator("#save-state")).toHaveText("Kaydedilmemiş değişiklikler");
    assert.ok(fs.readFileSync(bad, "utf8").includes('"view"'), "the broken file is not overwritten automatically");
    // A dirty diagram asks before closing, and survives a reload as a draft.
    dismissNext = true;
    await page.locator(".editor-tab.active .close-tab").click();
    assert.match(dialogs.at(-1), /block_editor_ui_bad\.imbd\.json/);
    await expect(page.locator(".editor-tab.active")).toContainText("block_editor_ui_bad.imbd");
    await page.waitForTimeout(400);
    await page.reload();
    await idle();
    await page.locator(".editor-tab", { hasText: "block_editor_ui_bad.imbd" }).click();
    await expect(page.locator("#block-editor")).toBeVisible();
    await expect(page.locator("#save-state")).toHaveText("Kaydedilmemiş değişiklikler");
    await expect(page.locator("#lines")).toHaveText("11 blok · 14 bağlantı");
    assert.ok(fs.readFileSync(bad, "utf8").includes('"view"'), "a restored draft is not written to disk");

    // ---- 200 blocks --------------------------------------------------------------
    await openFromFolder("block_editor_ui_200.imbd.json");
    await expect(page.locator("#block-editor .block-node")).toHaveCount(200);
    const first = await hook("lastRender");
    const times = [];
    for (let k = 0; k < 5; k++) times.push(await hook("render"));
    times.sort((a, b) => a - b);
    note(`render 200 blocks / 199 wires: first ${first.toFixed(1)} ms, median of 5 re-renders ${times[2].toFixed(1)} ms`);
    assert.ok(times[2] < 250, "200-block render time");

    if (SHOTS) {
      await page.locator(".editor-tab", { hasText: "block_editor_ui.imbd" }).first().click();
      await clickBlock("control");
      for (const theme of ["dark", "light"]) {
        const dark = await page.evaluate(() => document.body.classList.contains("dark"));
        if ((theme === "dark") !== dark) await page.locator("#theme").click();
        await page.waitForTimeout(200);
        await page.screenshot({ path: path.join(SHOTS, `blok-editor-${theme === "dark" ? "koyu" : "acik"}-1280.png`) });
      }
    }
    assert.deepEqual(errors, []);
    if (LOG) fs.writeFileSync(LOG, measurements.join("\n") + "\n");
    console.log("UI BLOCK EDITOR PASS: open, drag drift at 3 zooms, panel, mouse/keyboard wires, 13 types, signs, undo/redo, save/reopen, run vs Octave, Stop, compile-error selection, text switch, Save As suffix, broken file, 200 blocks.");
  } finally {
    await context.close();
    await browser.close();
    for (const name of [file, bad, big, path.join(workspace, "block_editor_ui_copy.json")]) fs.rmSync(name, { force: true });
  }
})().catch((error) => { console.error(error); process.exit(1); });
