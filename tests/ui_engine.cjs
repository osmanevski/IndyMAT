// The engine choice in Settings, in a real browser: GNU Octave is the default,
// a missing engine says how to build it, and a switch works both ways.
const { chromium, expect } = require("@playwright/test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const browser = await chromium.launch({ headless: true });
  const page = await (await browser.newContext({ locale: "en-US", viewport: { width: 1400, height: 900 } })).newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  let answer = true;
  const asked = [];
  page.on("dialog", (dialog) => { asked.push(dialog.message()); answer ? dialog.accept() : dialog.dismiss(); });
  await page.goto(launch.url);
  await expect(page.locator("#status-text")).toHaveText("Ready", { timeout: 60000 });
  await expect(page.locator("#engine-version")).toContainText("GNU Octave");
  assert.equal(await page.evaluate(() => document.body.classList.contains("engine-rust")), false);
  const engineSelect = page.locator('select[data-setting="computeEngine"]');
  const openSettings = async () => {
    await page.locator("#settings").click();
    await expect(page.locator(".settings-dialog")).toBeVisible();
  };
  await openSettings();
  await expect(engineSelect).toHaveValue("octave");
  await expect(page.locator(".settings-dialog .engine-note")).toContainText("default and recommended");

  // A missing engine: a clear message, no restart, and the selection stays on Octave.
  const message = "The Rust engine is not built yet. Build it with: cd motor && cargo build --release";
  await page.route("**/api/engine", (route) => route.fulfill({ json: { engine: "octave", available: { octave: true, rust: false }, problems: { octave: "", rust: message } } }));
  await engineSelect.selectOption("rust");
  await expect(page.locator("#toast")).toHaveText(message);
  await expect(page.locator(".settings-dialog .engine-note")).toHaveText(message);
  await expect(engineSelect).toHaveValue("octave");
  assert.deepEqual(asked, [], "nothing is confirmed for an engine that cannot start");
  await page.unroute("**/api/engine");

  const info = await page.evaluate(async (token) => (await fetch("/api/engine", { headers: { "X-MF-Token": token } })).json(), launch.url.split("#")[1]);
  if (info.available.rust) {
    // Declining the confirmation changes nothing.
    answer = false;
    await engineSelect.selectOption("rust");
    await expect.poll(() => asked.length).toBe(1);
    assert.match(asked[0], /Variables and figures in memory will be cleared/);
    await expect(engineSelect).toHaveValue("octave");
    await expect(page.locator("#engine-version")).toContainText("GNU Octave");
    answer = true;
    try {
      await engineSelect.selectOption("rust");
      await expect(page.locator("#engine-version")).toHaveText("Rust engine (experimental)", { timeout: 15000 });
      await expect(page.locator("#status-text")).toHaveText("Ready");
      assert.equal(await page.evaluate(() => document.body.classList.contains("engine-rust")), true);
      await expect(page.locator("#publish")).toBeHidden({ timeout: 5000 });
      await expect(page.locator(".run-more")).toBeHidden();
      await expect(page.locator('[data-action="packages"]').first()).toBeHidden();
      await page.locator("#command").fill("A = magic(3)");
      await page.locator("#command").press("Enter");
      await expect(page.locator('#variables tr[data-name="A"]')).toBeVisible();
      await page.locator("#command").fill("plot(1:3)");
      await page.locator("#command").press("Enter");
      await expect(page.locator("#plot-area canvas")).toBeVisible();
      // The engine draws its own PNGs: the PNG view shows a real image, like Octave's.
      await page.locator("#plot-png").click();
      const image = page.locator("#plot-area img");
      await expect(image).toBeVisible();
      await expect.poll(() => image.evaluate((node) => node.naturalWidth)).toBeGreaterThan(0);
      await page.locator("#plot-interactive").click();
      await expect(page.locator("#plot-area canvas")).toBeVisible();
      await openSettings();
      await expect(engineSelect).toHaveValue("rust");
      await engineSelect.selectOption("octave");
      await expect(page.locator("#engine-version")).toContainText("GNU Octave", { timeout: 30000 });
      await expect(page.locator("#status-text")).toHaveText("Ready", { timeout: 60000 });
      assert.equal(await page.evaluate(() => document.body.classList.contains("engine-rust")), false);
      await expect(page.locator('#variables tr[data-name="A"]')).toHaveCount(0);
      await expect(page.locator('[data-action="packages"]').first()).toBeVisible();
      await page.locator(".run-more summary").click();
      await expect(page.locator("#publish")).toBeVisible();
      await page.locator(".run-more summary").click();
    } finally {
      await page.evaluate(async (token) => fetch("/api/engine", { method: "POST", headers: { "X-MF-Token": token, "Content-Type": "application/json" }, body: JSON.stringify({ engine: "octave", confirm: true }) }), launch.url.split("#")[1]);
      fs.rmSync(".matlab-free/engine.json", { force: true });
    }
  }
  assert.deepEqual(errors, []);
  await browser.close();
  console.log("ENGINE UI PASS");
})().catch((error) => { console.error(error); process.exit(1); });
