"use strict";
const { assert, expect, openFigures, run, settle } = require("./figure_ui_support.cjs");
(async () => {
  const { browser, page, errors, execute } = await openFigures();
  try {
    await run(page,"close all;figure('name','2D view lifecycle');plot([0 1 2],[0 1 0],'-o','displayname','Anchor');axis([0 2 0 2]);");
    await expect(page.locator("#plot-area canvas")).toBeVisible();
    const count = execute.length;
    await page.locator("#plot-expand").click();
    const plot = page.locator("#modal[open] .interactive-plot"), canvas = plot.locator("canvas"), tip = plot.locator(".plot-data-tip");
    await settle(page);
    const initial = await canvas.getAttribute("data-view");
    const point = () => plot.evaluate(node => node.figureTest.project(0,1,1));
    let p = await point(), bounds = await canvas.boundingBox();
    await page.mouse.move(bounds.x+p.x,bounds.y+p.y);await page.mouse.down();await page.mouse.up();
    await expect(tip).toBeVisible();await expect(tip).toHaveText("Anchor: x = 1, y = 1");
    await page.locator("#modal[open]").evaluate(node=>{node.style.width="700px";node.style.height="650px";});
    await settle(page);p = await point();
    const pinned = await tip.evaluate(node => ({x:parseFloat(node.style.left),y:parseFloat(node.style.top)}));
    assert(Math.abs(pinned.x-p.x-9)<2,"2D pin did not follow resized sample");
    assert(Math.abs(pinned.y-p.y+30)<2,"2D pin drifted vertically");
    await page.locator("#modal[open] .figure-window-reset").click();await expect(tip).toBeHidden();
    bounds = await canvas.boundingBox();p = await point();
    await page.mouse.move(bounds.x+p.x,bounds.y+p.y);await page.mouse.wheel(0,-120);
    await expect.poll(()=>canvas.getAttribute("data-view")).not.toBe(initial);
    await page.keyboard.down("Shift");await page.mouse.down();await page.mouse.move(bounds.x+p.x+35,bounds.y+p.y+20,{steps:3});
    await canvas.press("Home");await page.mouse.up();await page.keyboard.up("Shift");
    await expect.poll(()=>canvas.getAttribute("data-view")).toBe(initial);
    await expect(tip).toBeHidden();
    assert.equal(execute.length,count,"reset/resize submitted numerical work");
    assert.deepEqual(errors,[]);
    await page.locator("#modal-close").click();
    console.log("UI FIGURE VIEW STATE PASS: 2D pinned resize, reset during active pan, local session preservation.");
  } finally { await browser.close(); }
})().catch(error=>{console.error(error);process.exit(1);});
