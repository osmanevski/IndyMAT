"use strict";
const { assert, expect, openFigures, run, settle } = require("./figure_ui_support.cjs");

(async () => {
  const { browser, page, errors, execute } = await openFigures();
  try {
    await run(page, "close all;figure('name','E2 line');plot([1 2 3 4],[10 20 15 30],'-o','displayname','series');");
    const panel = page.locator("#plot-area canvas");
    await expect(panel).toBeVisible();
    await expect(page.locator("#plot-interactive")).toHaveClass(/active/);
    const initial = await panel.getAttribute("data-view");
    const small = await panel.boundingBox();
    const count = execute.length;
    // Enlarge hosts the SAME live 2D viewer: one canvas, in the modal, larger.
    await page.locator("#plot-expand").click();
    const large = page.locator("#modal[open] .interactive-plot canvas");
    await expect(large).toBeVisible();
    await expect(page.locator("#modal[open] img")).toHaveCount(0);
    await expect(page.locator("#plot-area canvas")).toHaveCount(0);
    await expect.poll(async () => (await large.boundingBox()).width).toBeGreaterThan(small.width);
    const big = await large.boundingBox();
    assert(big.height > small.height, "enlarged canvas is not taller than the panel canvas");
    const backing = await large.evaluate((canvas) => ({ width: canvas.width, css: canvas.getBoundingClientRect().width, dpr: devicePixelRatio }));
    assert(Math.abs(backing.width - backing.css * backing.dpr) <= 2, "enlarged canvas was not resized to the modal");
    assert.equal(await large.getAttribute("data-view"), initial, "enlarge changed the view");
    // Wheel zoom inside the modal changes the axis limits.
    await page.mouse.move(big.x + big.width / 2, big.y + big.height / 2);
    await page.mouse.wheel(0, -240);
    await expect.poll(() => large.getAttribute("data-view")).not.toBe(initial);
    const zoomed = JSON.parse(await large.getAttribute("data-view"));
    const start = JSON.parse(initial);
    assert(zoomed[0].x[1] - zoomed[0].x[0] < start[0].x[1] - start[0].x[0], "wheel did not zoom in on x");
    // Shift-drag pans, a hover shows a data tip, double click resets: all inside the modal.
    await page.keyboard.down("Shift");
    await page.mouse.down();
    await page.mouse.move(big.x + big.width / 2 + 60, big.y + big.height / 2 + 20, { steps: 4 });
    await page.mouse.up();
    await page.keyboard.up("Shift");
    const panned = await large.getAttribute("data-view");
    assert.notEqual(panned, JSON.stringify(zoomed), "Shift-drag did not pan");
    await large.dblclick({ position: { x: big.width / 2, y: big.height / 2 } });
    await expect.poll(() => large.getAttribute("data-view")).toBe(initial);
    await page.mouse.move(big.x + big.width / 2, big.y + big.height / 2);
    await page.mouse.wheel(0, -120);
    await expect.poll(() => large.getAttribute("data-view")).not.toBe(initial);
    const kept = await large.getAttribute("data-view");
    // Close: the panel viewer is interactive again with the same limits.
    await page.locator("#modal-close").click();
    await expect(page.locator("#modal[open]")).toHaveCount(0);
    await expect(panel).toBeVisible();
    assert.equal(await page.locator(".interactive-plot canvas").count(), 1, "a second viewer canvas was left behind");
    assert.equal(await panel.getAttribute("data-view"), kept, "closing the modal lost the view");
    const box = await panel.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(0, 120);
    await expect.poll(() => panel.getAttribute("data-view")).not.toBe(kept);
    await page.locator("#plot-fit").click();
    await expect.poll(() => panel.getAttribute("data-view")).toBe(initial);
    // PNG mode of an interactive figure enlarges the image.
    await page.locator("#plot-png").click();
    await page.locator("#plot-expand").click();
    await expect(page.locator("#modal[open] img")).toBeVisible();
    await expect(page.locator("#modal[open] canvas")).toHaveCount(0);
    await page.locator("#modal-close").click();
    await page.locator("#plot-interactive").click();
    await expect(panel).toBeVisible();
    await settle(page);
    assert.equal(execute.length, count, "enlarging submitted /api/execute");
    // A figure without interactive data enlarges as the PNG.
    await run(page, "close all;figure('name','E2 image');imagesc(magic(4));");
    await expect(page.locator("#plot-interactive")).toBeDisabled();
    await expect(page.locator("#plot-area img")).toBeVisible();
    await page.locator("#plot-expand").click();
    await expect(page.locator("#modal[open] img")).toBeVisible();
    await page.locator("#modal[open] img").evaluate((img) => img.decode());
    await expect(page.locator("#modal[open] canvas")).toHaveCount(0);
    await page.locator("#modal-close").click();
    assert.deepEqual(errors, []);
    console.log("UI FIGURES ENLARGE PASS: 2D viewer moves into the modal, resize, wheel zoom, Shift-pan, reset, view kept on close, PNG mode and PNG-only figures show the image.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
