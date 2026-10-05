const { chromium, expect } = require("@playwright/test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const freshPage = async () => {
    // browser.newPage gives each scenario its own context and fresh localStorage.
    const page = await browser.newPage({ locale: "tr-TR", viewport: { width: 1440, height: 900 } });
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(launch.url);
    await ready(page);
    return page;
  };
  const ready = (page) => page.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hazır" && typeof document.querySelector("#left-divider")?.onkeydown === "function");
  const stored = (page) => page.evaluate(() => JSON.parse(localStorage.getItem("mf-settings-v1")));
  const size = (page, selector) => page.locator(selector).evaluate((node) => ({ width: node.clientWidth, height: node.clientHeight }));
  const noOverflow = (page) => expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBe(0);
  const toggle = (page, area) => page.locator(`#toggle-layout-${area}`).click();
  const drag = async (page, selector, destination) => {
    const box = await page.locator(selector).boundingBox();
    assert(box, selector + " missing");
    await page.mouse.move(box.x + box.width / 2, box.y + Math.min(100, box.height / 2));
    await page.mouse.down();
    await page.mouse.move(destination.x ?? box.x + box.width / 2, destination.y ?? box.y + Math.min(100, box.height / 2), { steps: 5 });
    await page.mouse.up();
    await expect(page.locator("body")).not.toHaveClass(/resizing-|layout-snap-/);
  };
  const checkbox = (page, name) => page.locator(`input[data-layout-panel="${name}"]`);
  try {
    let page = await freshPage();
    const original = await size(page, ".editor-panel");
    for (const [area, target, key, dimension] of [["left", "#left-panel", "ControlOrMeta+b", "width"], ["right", "#right-panel", "ControlOrMeta+Alt+b", "width"], ["bottom", ".bottom-panels", "ControlOrMeta+j", "height"]]) {
      await toggle(page, area);
      await expect(page.locator(target)).toBeHidden();
      await expect(page.locator(`#toggle-layout-${area}`)).toHaveAttribute("aria-pressed", "false");
      assert((await size(page, ".editor-panel"))[dimension] > original[dimension]);
      await page.locator(".cm-line").first().click();
      await page.keyboard.press(key);
      await expect(page.locator(target)).toBeVisible();
      await page.locator(".cm-line").first().click();
      await page.keyboard.press(key);
      await expect(page.locator(target)).toBeHidden();
      await toggle(page, area);
      await expect(page.locator(target)).toBeVisible();
      await noOverflow(page);
    }
    await expect(page.locator("#toggle-layout-left")).toHaveAttribute("title", /Birincil.*Toggle Primary Side Bar/);
    await toggle(page, "left");
    await toggle(page, "right");
    await toggle(page, "bottom");
    await expect.poll(() => stored(page)).toMatchObject({ panels: { files: false, workspace: false, history: false, debugger: false, bottom: false } });
    await page.reload();
    await ready(page);
    for (const area of ["left", "right", "bottom"]) await expect(page.locator(`#toggle-layout-${area}`)).toHaveAttribute("aria-pressed", "false");
    await expect(page.locator("#left-divider, #right-divider, #editor-divider")).toHaveCount(3);
    for (const selector of ["#left-divider", "#right-divider", "#editor-divider"]) await expect(page.locator(selector)).toBeHidden();
    await noOverflow(page);
    await page.locator("#settings").click();
    for (const name of ["files", "workspace", "history", "debugger", "bottom"]) await expect(checkbox(page, name)).not.toBeChecked();
    await checkbox(page, "files").check();
    await expect(page.locator("#toggle-layout-left")).toHaveAttribute("aria-pressed", "true");
    await page.getByRole("button", { name: "Yerleşimi sıfırla", exact: true }).click();
    await expect.poll(() => stored(page)).toMatchObject({ layout: { left: 220, right: 300, editorHeight: 56, consoleWidth: 50 } });
    for (const name of ["files", "workspace", "history", "debugger", "bottom", "figures"]) await expect(checkbox(page, name)).toBeChecked();
    await checkbox(page, "figures").uncheck();
    await page.locator("#modal-close").click();
    await expect(page.locator(".plot-panel")).toBeHidden();
    await expect(page.locator("#plot-divider")).toBeHidden();
    assert.equal((await size(page, ".console-panel")).width, (await size(page, ".bottom-panels")).width);
    await page.close();

    page = await freshPage();
    await drag(page, "#left-divider", { x: 286 });
    await expect.poll(async () => (await stored(page)).layout.left).toBe(280);
    await drag(page, "#left-divider", { x: 20 });
    await expect(page.locator("#left-panel")).toBeHidden();
    assert.equal((await stored(page)).layout.left, 280);
    await toggle(page, "left");
    assert.equal((await size(page, "#left-panel")).width, 280);
    await page.locator("#right-divider").focus();
    await page.keyboard.press("ArrowLeft");
    await expect.poll(async () => (await stored(page)).layout.right).toBe(310);
    const edge = await page.locator("#right-panel").evaluate((node) => node.getBoundingClientRect().right);
    await drag(page, "#right-divider", { x: edge - 40 });
    await expect(page.locator("#right-panel")).toBeHidden();
    await toggle(page, "right");
    assert.equal((await size(page, "#right-panel")).width, 310);
    await page.locator("#editor-divider").focus();
    await page.keyboard.press("ArrowDown");
    await expect.poll(async () => (await stored(page)).layout.editorHeight).toBeGreaterThan(57);
    const height = (await stored(page)).layout.editorHeight;
    const centerBottom = await page.locator(".center").evaluate((node) => node.getBoundingClientRect().bottom);
    await drag(page, "#editor-divider", { y: centerBottom - 15 });
    await expect(page.locator(".bottom-panels")).toBeHidden();
    assert.equal((await stored(page)).layout.editorHeight, height);
    await toggle(page, "bottom");
    assert.equal((await stored(page)).layout.editorHeight, height);
    for (const [selector, key, value] of [["#left-divider", "left", 220], ["#right-divider", "right", 300], ["#editor-divider", "editorHeight", 56], ["#plot-divider", "consoleWidth", 50]]) {
      await page.locator(selector).focus();
      await page.keyboard.press("ArrowRight");
      await page.locator(selector).dblclick();
      await expect.poll(async () => (await stored(page)).layout[key]).toBe(value);
    }
    await noOverflow(page);
    await page.close();

    page = await freshPage();
    const beforeMax = await size(page, ".editor-panel");
    await page.locator("#maximize-panel").click();
    await expect(page.locator(".editor-panel")).toBeHidden();
    await expect(page.locator("#maximize-panel")).toHaveAttribute("aria-label", "Panel boyutunu geri yükle");
    assert.equal((await size(page, ".bottom-panels")).height, (await size(page, ".center")).height);
    await page.locator("#maximize-panel").click();
    assert.equal((await size(page, ".editor-panel")).height, beforeMax.height);
    await page.locator("#maximize-panel").click();
    await toggle(page, "bottom");
    await toggle(page, "bottom");
    await expect(page.locator(".editor-panel")).toBeVisible();
    await expect(page.locator("#maximize-panel")).toHaveAttribute("aria-pressed", "false");
    await page.locator("#maximize-panel").click();
    await page.locator("#settings").click();
    await page.getByRole("button", { name: "Yerleşimi sıfırla", exact: true }).click();
    await page.locator("#modal-close").click();
    await expect(page.locator(".editor-panel")).toBeVisible();
    await expect(page.locator("#maximize-panel")).toHaveAttribute("aria-pressed", "false");
    await page.locator("#maximize-panel").click();
    await page.reload();
    await ready(page);
    await expect(page.locator(".editor-panel")).toBeVisible();
    await expect(page.locator("#maximize-panel")).toHaveAttribute("aria-pressed", "false");
    await page.close();

    // Every hidden/visible combination, with and without the fourth column.
    for (const assistant of [false, true]) {
      for (let mask = 0; mask < 8; mask++) {
        page = await freshPage();
        if (assistant) await page.locator("#toggle-assistant").click();
        for (const [index, area] of ["left", "bottom", "right"].entries()) if (!(mask & 1 << index)) await toggle(page, area);
        await noOverflow(page);
        assert((await size(page, ".center")).width >= 300);
        await page.close();
      }
    }
    page = await freshPage();
    await page.locator("#toggle-assistant").click();
    const assistantEdge = await page.locator("#right-panel").evaluate((node) => node.getBoundingClientRect().right);
    await drag(page, "#right-divider", { x: assistantEdge - 230 });
    await expect.poll(async () => (await stored(page)).layout.right).toBe(230);
    await drag(page, "#right-divider", { x: assistantEdge - 30 });
    await toggle(page, "right");
    assert.equal((await size(page, "#right-panel")).width, 230);
    for (const width of [1150, 900, 650, 390]) {
      await page.setViewportSize({ width, height: 900 });
      await noOverflow(page);
      if (width <= 900) await expect(page.locator("#toggle-layout-left")).toBeHidden();
    }
    await page.locator("#toggle-files").click();
    await expect(page.locator("#left-panel")).toBeVisible();
    await page.close();
    assert.deepEqual(errors, []);
    console.log("UI LAYOUT PASS: toggles/shortcuts, persistence, settings sync, snap/restore sizes, keyboard/double-click reset, maximise, every combination, Assistant and responsive overflow.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
