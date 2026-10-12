"use strict";
const { assert, expect, openFigures, run } = require("./figure_ui_support.cjs");

(async () => {
  const { browser, page, errors } = await openFigures();
  try {
    await page.setViewportSize({ width: 1280, height: 900 });
    const name = "Reason wraps";
    const reason = "saydamlık";
    await run(page, `close all;figure('name','${name}');surf(ones(2),'facealpha',0.5);`);

    const caption = page.locator("#figure-caption");
    await expect(caption).toContainText(name);
    await expect(caption).toContainText("Yalnızca PNG görünümü kullanılabilir:");
    await expect(caption).toContainText(reason);
    const layout = await caption.evaluate((node) => {
      const lineHeight = parseFloat(getComputedStyle(node).lineHeight);
      return {
        text: node.textContent,
        clientWidth: node.clientWidth,
        scrollWidth: node.scrollWidth,
        height: node.getBoundingClientRect().height,
        lineHeight
      };
    });
    assert(layout.scrollWidth <= layout.clientWidth || layout.height > layout.lineHeight + 1,
      `Caption is clipped instead of wrapping: ${JSON.stringify(layout)}`);
    await expect(page.locator("#plot-fit")).toBeVisible();
    await expect(page.locator("#plot-fit")).toBeEnabled();
    const reset = await page.locator("#plot-fit").boundingBox();
    assert(reset && reset.x >= 0 && reset.x + reset.width <= 1280, "Figure reset control remains in the viewport");
    assert.deepEqual(errors, []);
    console.log("UI FIGURES REASON PASS: full Turkish fallback reason remains readable at 1280px.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
