"use strict";
const { assert, expect, openFigures, run, require3D, states, view, drag, settle } = require("./figure_ui_support.cjs");

(async () => {
  const { browser, page, errors, execute } = await openFigures();
  try {
    const jsonRequests = [];
    page.on("request", (request) => {
      const url = new URL(request.url());
      if (url.pathname === "/api/figure" && url.searchParams.get("file")?.endsWith(".json")) jsonRequests.push(url.search);
    });
    const code = "close all;figure('name','R3 first');plot3(1:3,1:3,1:3);figure('name','R3 second');scatter3(1:3,3:-1:1,1:3,100,[0.8 0.2 0.4],'filled');";
    await run(page, code);
    await require3D(page, "R3 first");
    assert.equal(jsonRequests.length, 1, "inactive figure JSON was fetched");
    await drag(page, 42, 10);
    const first = await states(page);
    const count = execute.length;
    await page.locator("#figure-tabs button").filter({ hasText: "R3 second" }).click();
    await require3D(page, "R3 second");
    assert.equal(jsonRequests.length, 2);
    await page.locator("#figure-tabs button").filter({ hasText: "R3 first" }).click();
    await require3D(page, "R3 first");
    assert.deepEqual(await states(page), first, "tab switch lost camera");
    await page.locator("#plot-expand").click();
    await expect(page.locator("#modal[open] .figure-3d")).toBeVisible();
    await expect(page.locator("#plot-area .figure-3d")).toHaveCount(0);
    assert.equal(await page.locator(".figure-webgl").count(), 1, "enlarge created a second GL canvas");
    assert.deepEqual(await states(page), first);
    await drag(page, 20, 0);
    const enlarged = await states(page);
    await page.locator("#modal-close").click();
    await expect(page.locator("#plot-area .figure-3d")).toBeVisible();
    assert.deepEqual(await states(page), enlarged, "modal close lost its camera");
    await page.setViewportSize({ width: 1280, height: 800 });
    assert.deepEqual(await states(page), enlarged, "resize reset camera");
    await page.locator("#settings").click();
    const language = page.locator(".settings-field").filter({ has: page.locator('[data-i18n="Language"]') }).locator("select");
    await language.selectOption("en");
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    assert.deepEqual(await states(page), enlarged, "language switch reset camera");
    await language.selectOption("tr");
    await page.locator("#modal-close").click();
    await expect(page.locator("#figure-tool-instructions")).toContainText("Döndürmek için sürükleyin");
    await settle(page);
    assert.equal(execute.length, count, "lifecycle/view change executed Octave");
    await run(page, "close all;figure('name','R3 replacement');mesh([1 2;3 4]);view(25,35);");
    await require3D(page, "R3 replacement");
    const replacement = await states(page);
    assert.notEqual(replacement[0].identity.job, first[0].identity.job);
    assert.equal((await view(page))[0].azimuth, 25, "new artifact restored old camera");
    const lost = await page.locator(".figure-3d").evaluate((node) => node.figureTest.loseContext());
    assert(lost, "WEBGL_lose_context is required to verify native context-loss fallback");
    await expect(page.locator("#figure-caption")).toContainText("grafik bağlamı kayboldu");
    await expect(page.locator("#plot-area img")).toBeVisible();
    await expect(page.locator("#figure-tools")).toBeHidden();
    assert.deepEqual(errors, []);
    console.log("UI FIGURES LIFECYCLE PASS: active-only JSON, tab camera state, single-context enlarge/close, resize, live language, new artifact and context-loss PNG.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
