"use strict";
const { assert, openFigures, run, require3D, pixel } = require("./figure_ui_support.cjs");
(async () => {
  const { browser, page, errors } = await openFigures();
  try {
    // Review finding 4: a back-wall grid crossing is hidden by an opaque face.
    const base = "close all;figure('name','Peeling background');axes('position',[.1 .1 .8 .8]);" +
      "surf([0 1],[0 1],ones(2),'facecolor',[1 0 0],'edgecolor','none');" +
      "hold on;view(2);axis([0 1 0 1 0 2]);" +
      "set(gca,'xtick',[0 .5 1],'ytick',[0 .5 1],'xgrid','on','ygrid','on');";
    const sample = () => page.locator(".figure-3d").evaluate(node => {
      const p = node.figureTest.project(0, [.5, .5, 1]);
      return node.figureTest.pixels([{ x: p.x, y: p.y }]);
    });
    await run(page, base);
    await require3D(page, "Peeling background");
    await pixel(page, [.5, .5, 1], [255, 0, 0], 1);
    const opaque = await sample();
    await run(page, base +
      "patch('vertices',[0 0 1.5;.1 0 1.5;.1 .1 1.5;0 .1 1.5]," +
      "'faces',1:4,'facecolor',[0 0 1],'facealpha',.5,'edgecolor','none');");
    await require3D(page, "Peeling background");
    await page.waitForFunction(() => {
      const d = document.querySelector(".figure-3d")?.figureTest.transparency();
      return d?.frames > 0 && !d.pending;
    }, undefined, { timeout: 6000 });
    const transparent = await sample();
    assert.deepEqual(transparent, opaque, "corner transparency must not expose the back-wall grid through the red surface");
    await pixel(page, [.5, .5, 1], [255, 0, 0], 1);
    console.log("UI FIGURES PEELING BACKGROUND PASS", JSON.stringify({opaque,transparent}));
    assert.deepEqual(errors, []);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exit(1); });
