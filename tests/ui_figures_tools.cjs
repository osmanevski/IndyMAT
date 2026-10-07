"use strict";
const { assert, expect, openFigures, run, require3D, view, projected, mode, drag, settle } = require("./figure_ui_support.cjs");

(async () => {
  const { browser, page, errors, execute } = await openFigures();
  try {
    await run(page, "close all;figure('name','R3 tools');plot3([0 1 2],[0 1 0],[0 1 2],'-o','markersize',12,'displayname','Known vertex');axis([0 2 0 1 0 2]);");
    await require3D(page, "R3 tools");
    const initial = await view(page);
    const count = execute.length;
    await drag(page, 40, 16);
    assert.notDeepEqual(await view(page), initial, "rotate drag did not change camera");
    await page.locator("#plot-fit").click();
    assert.deepEqual(await view(page), initial);
    for (const value of ["zoom", "pan"]) {
      await mode(page, value);
      await drag(page, 20, 20);
      assert.notDeepEqual(await view(page), initial, value + " did not change camera");
      await page.locator("#plot-fit").click();
      assert.deepEqual(await view(page), initial);
      await expect(page.locator(".figure-3d")).toHaveAttribute("data-figure-mode", "rotate");
    }
    const wrapped = (angle) => ((angle % 360) + 360) % 360;
    await page.locator("#rotate-left").click();
    assert.equal((await view(page))[0].azimuth, wrapped(initial[0].azimuth - 15));
    await page.locator("#rotate-right").click();
    assert.equal((await view(page))[0].azimuth, wrapped(initial[0].azimuth));
    await page.locator("#plot-fit").click();
    await mode(page, "tips");
    const p = await projected(page, [1, 1, 1]);
    await page.locator(".figure-3d").click({ position: { x: p.x, y: p.y } });
    await expect(page.locator(".figure-pinned-tip")).toContainText("Known vertex");
    for (const value of ["X: 1", "Y: 1", "Z: 1", "İndeks: 2"]) await expect(page.locator(".figure-pinned-tip")).toContainText(value);
    await page.locator(".figure-3d").press("Escape");
    await expect(page.locator(".figure-pinned-tip")).toHaveCount(0);
    await page.locator(".figure-3d").press("ArrowRight");
    await page.locator(".figure-3d").press("Shift+ArrowLeft");
    await page.locator(".figure-3d").press("=");
    await page.locator(".figure-3d").press("Home");
    assert.deepEqual(await view(page), initial);
    await page.locator("#plot-png").click();
    await expect(page.locator("#plot-area img")).toBeVisible();
    await expect(page.locator("#figure-tools")).toBeHidden();
    await page.locator("#plot-interactive").click();
    await require3D(page, "R3 tools");
    await settle(page);
    assert.equal(execute.length, count, "local tools/view switches submitted /api/execute");
    assert.deepEqual(errors, []);
    console.log("UI FIGURES TOOLS PASS: modes, orbit/zoom/pan, reset, local 15-degree buttons, original XYZ/index tips, Escape and keyboard shortcuts; no execute requests.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
