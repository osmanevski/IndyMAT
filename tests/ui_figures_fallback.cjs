"use strict";
const { assert, expect, openFigures, run, require3D } = require("./figure_ui_support.cjs");

(async () => {
  const { browser, page, errors } = await openFigures();
  try {
    // Establish WebGL2 first: fallback must never hide a missing GL backend.
    await run(page, "close all;figure('name','R3 GL prerequisite');plot3(1:3,1:3,1:3);");
    await require3D(page, "R3 GL prerequisite");
    for (const [name, code, reason] of [
      ["R3 alpha", "surf(ones(2),'facealpha',0.5);", "saydamlık"],
      ["R3 budget", "surf(ones(201,200));", "40200 > 40000"]
    ]) {
      await run(page, `close all;figure('name','${name}');${code}`);
      await expect(page.locator("#figure-caption")).toContainText("Yalnızca PNG görünümü kullanılabilir:");
      await expect(page.locator("#figure-caption")).toContainText(reason);
      await expect(page.locator("#plot-area img")).toBeVisible();
      await page.locator("#plot-area img").evaluate((img) => img.decode());
      await expect(page.locator(".figure-3d")).toHaveCount(0);
      await expect(page.locator("#figure-tools")).toBeHidden();
      await expect(page.locator("#plot-interactive")).toBeDisabled();
      const downloadEvent = page.waitForEvent("download");
      await page.locator("#plot-download").click();
      const download = await downloadEvent;
      assert.equal(download.suggestedFilename(), name + ".png");
      const bytes = require("node:fs").readFileSync(await download.path());
      assert.deepEqual([...bytes.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
    }
    assert.deepEqual(errors, []);
    console.log("UI FIGURES FALLBACK PASS: real transparent and over-40000 surfaces, Turkish reasons, hidden tools and intact PNG downloads.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
