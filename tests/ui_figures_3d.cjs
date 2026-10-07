"use strict";
const { assert, expect, openFigures, run, require3D, pixel, view } = require("./figure_ui_support.cjs");

(async () => {
  const { browser, page, errors } = await openFigures();
  try {
    for (const [name, code, point, rgb] of [
      ["R3 plot3", "plot3([0 1 2],[0 1 0],[0 1 2],'-o','linewidth',3,'markersize',12,'color',[0.8 0.2 0.4],'markerfacecolor',[0.8 0.2 0.4]);", [1, 1, 1], [204, 51, 102]],
      ["R3 scatter3", "scatter3([0 1 2],[0 1 0],[0 1 2],[100 400 225],[0.8 0.2 0.4],'filled');", [1, 1, 1], [204, 51, 102]],
      ["R3 surf", "surf([0 1 2],[0 1],ones(2,3),[1 3 5;2 4 6],'edgecolor','none');colormap([1 0 0;0 1 0;0 0 1;1 1 0]);caxis([1 5]);axis([0 2 0 1 0 2]);view(0,90);cb=colorbar();ylabel(cb,'Intensity');", [0.4, 0.3, 1], [255, 0, 0]],
      ["R3 mesh", "mesh([0 1],[0 1],[0 1;1 0]);axis([0 1 0 1 0 1]);", [1 / 3, 1 / 3, 2 / 3], [255, 255, 255]]
    ]) {
      await run(page, `close all;figure('name','${name}');${code}title('${name}');xlabel('X');ylabel('Y');zlabel('Z');grid on;`);
      await require3D(page, name);
      // Read a few interior pixels, not a screenshot golden or stale buffer.
      await pixel(page, point, rgb);
      const dimensions = await page.locator(".figure-webgl").evaluate((canvas) => ({ width: canvas.width, height: canvas.height, css: canvas.getBoundingClientRect().width, dpr: devicePixelRatio }));
      assert(Math.abs(dimensions.width - dimensions.css * dimensions.dpr) <= 1);
      assert(dimensions.height > 1);
      const before = await view(page);
      await page.locator("#theme").click();
      assert.deepEqual(await view(page), before, "theme reset the camera");
      await page.locator("#theme").click();
      await expect(page.locator(".figure-labels")).toBeVisible();
      if (name === "R3 surf") {
        // The neighboring cell has one flat blue owner, including both triangles.
        // Blue must remain blue even though the line contrast rule would change it.
        await pixel(page, [1.4, 0.3, 1], [0, 0, 255]);
        const swatches = await page.locator(".figure-3d").evaluate((node) => {
          const canvas = node.querySelector(".figure-labels");
          const ctx = canvas.getContext("2d");
          // Octave eastoutside colorbar occupies the right part of the figure.
          // Find saturated swatches away from text/box lines at three heights.
          return [0.25, 0.5, 0.75].map((f) => {
            const y = Math.floor(canvas.height * f);
            const pixels = ctx.getImageData(Math.floor(canvas.width * 0.8), y, Math.max(1, Math.floor(canvas.width * 0.12)), 1).data;
            return [...pixels].filter((_, i) => i % 4 === 3 && pixels[i] === 255).length;
          });
        });
        assert(swatches.every((count) => count > 2), "colorbar swatches missing");
      }
    }
    await run(page, "close all;figure('name','R3 reduced');r3_t=linspace(0,10,5001);plot3(r3_t,sin(r3_t),cos(r3_t));");
    await require3D(page, "R3 reduced", /^R3 reduced · Etkileşimli veri 5001 noktadan \d+ noktaya azaltıldı\.$/);
    assert.deepEqual(errors, []);
    console.log("UI FIGURES 3D PASS: real plot3/scatter3/surf/mesh, interior pixels, opaque mesh faces, colorbar, DPR and theme camera preservation.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
