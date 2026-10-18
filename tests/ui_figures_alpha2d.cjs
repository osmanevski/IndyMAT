"use strict";
// Real browser: scalar FaceAlpha/EdgeAlpha/MarkerFaceAlpha/MarkerEdgeAlpha of
// 2D objects stay interactive and blend over what lies below.
const path = require("node:path");
const os = require("node:os");
const { assert, expect, openFigures, run, settle } = require("./figure_ui_support.cjs");

const SHOTS = process.env.INDYMAT_ALPHA_SHOTS || "";

(async () => {
  const { browser, page, errors } = await openFigures();
  try {
    const plot = page.locator("#plot-area .interactive-plot");
    const canvas = page.locator("#plot-area .interactive-plot canvas");
    const hook = (name, ...args) => plot.evaluate((node, { name, args }) => node.figureTest[name](...args), { name, args });
    const backgroundNow = () => page.evaluate(() => getComputedStyle(document.body).getPropertyValue("--plot-bg").trim().slice(1).match(/../g).map((v) => parseInt(v, 16)));
    const interactive = async (name, code) => {
      await run(page, `close all;figure('name','${name}');${code}`);
      await expect(page.locator("#figure-caption")).toHaveText(name, { timeout: 20000 });
      await expect(canvas).toBeVisible();
      await expect(page.locator("#plot-interactive")).toHaveClass(/active/);
      await settle(page);
    };
    const sample = async (x, y, axis = 0) => {
      const p = await hook("project", axis, x, y);
      return hook("pixel", p.x, p.y);
    };
    const near = (rgba, expected, label, tolerance = 16) => {
      for (let i = 0; i < 3; i++) assert(Math.abs(rgba[i] - expected[i]) <= tolerance, `${label}: ${rgba} differs from ${expected}`);
    };
    const blend = (src, alpha, bg) => src.map((v, i) => alpha * v + (1 - alpha) * bg[i]);

    // fill over a line: the fill blends over the background and shows the line through.
    await interactive("A2 fill", "plot([0 4],[2 2],'-','color',[1 1 1],'linewidth',3);hold on;fill([0 4 4 0],[0 0 4 4],[1 0 0],'FaceAlpha',0.5,'EdgeColor','none');axis([0 4 0 4]);");
    let bg = await backgroundNow();
    assert.equal(await hook("faces"), 1);
    near(await sample(3, 1), blend([255, 0, 0], .5, bg), "fill over background");
    const onLine = await sample(2, 2), offLine = await sample(2, 3);
    assert(Math.abs(onLine[1] - offLine[1]) > 20 || Math.abs(onLine[2] - offLine[2]) > 20, `line not visible through the fill: ${onLine} vs ${offLine}`);

    // EdgeAlpha applies to the stroke only; the face keeps its own value.
    await interactive("A2 edge", "fill([1 3 3 1],[1 1 3 3],[1 0 0],'FaceAlpha',1,'EdgeColor',[0 1 0],'EdgeAlpha',0.4,'LineWidth',6);axis([0 4 0 4]);");
    bg = await backgroundNow();
    near(await sample(2, 2), [255, 0, 0], "opaque face interior");

    // area: transparent, tips unchanged.
    await interactive("A2 area", "area(1:4,[1 3 2 4],'FaceAlpha',0.3,'FaceColor',[1 0 0],'displayname','alan');axis([1 4 0 5]);");
    bg = await backgroundNow();
    near(await sample(2, 0.3), blend([255, 0, 0], .3, bg), "area interior");
    const tipBox = await canvas.boundingBox(), tp = await hook("project", 0, 2, 3);
    await page.mouse.move(tipBox.x + tp.x, tipBox.y + tp.y);
    await expect(page.locator("#plot-area .plot-data-tip")).toBeVisible();

    // bar with transparent faces.
    await interactive("A2 bar", "h=bar([10 20 30],[1 3 2],0.5);set(findobj(h,'type','patch'),'facecolor',[1 0 0],'facealpha',0.4);");
    bg = await backgroundNow();
    near(await sample(20, 1.5), blend([255, 0, 0], .4, bg), "bar interior");
    await interactive("A2 bar zero", "h=bar([10 20 30],[1 3 2],0.5);set(findobj(h,'type','patch'),'facecolor',[1 0 0],'facealpha',0);");
    bg = await backgroundNow();
    near(await sample(20, 1.5), bg, "alpha 0 face is invisible");

    // scatter with marker alpha: two overlapping big markers.
    await interactive("A2 scatter", "scatter([1 1.04],[1 1],900,[1 0 0],'filled','MarkerFaceAlpha',0.4,'MarkerEdgeColor','none');axis([0 3 0 3]);");
    bg = await backgroundNow();
    const single = await sample(.99, 1), both = await sample(1.02, 1);
    assert(single[0] > bg[0] + 20 && single[0] < 255 - 40, `single marker is not translucent: ${single}`);
    assert(both[0] > single[0] + 10, `overlap not darker/stronger than a single marker: ${both} vs ${single}`);

    // Data-driven alpha still sends the whole figure to its PNG.
    await run(page, "close all;figure('name','A2 png');patch([0 1 1],[0 0 1],'r','FaceAlpha','flat','FaceVertexAlphaData',[.1;.5;.9]);");
    await expect(page.locator("#figure-caption")).toContainText("A2 png", { timeout: 20000 });
    await expect(page.locator("#figure-caption")).toContainText("saydamlık");
    await expect(page.locator("#plot-area canvas")).toHaveCount(0);

    // Screenshots in both themes.
    if (SHOTS) {
      await interactive("A2 hepsi", "subplot(2,2,1);plot([0 4],[2 2],'-','color',[1 .6 0],'linewidth',3);hold on;fill([1 3 3 1],[0.5 .5 3.5 3.5],[1 0 0],'FaceAlpha',0.5,'EdgeColor',[0 .6 1],'EdgeAlpha',0.5,'LineWidth',3);axis([0 4 0 4]);title('fill');" +
        "subplot(2,2,2);area(1:4,[1 3 2 4],'FaceAlpha',0.3,'FaceColor',[1 0 0],'displayname','area');title('area');legend show;" +
        "subplot(2,2,3);h=bar([1 2 3],[1 3 2],0.6);set(findobj(h,'type','patch'),'facecolor',[0 .6 1],'facealpha',0.4);hold on;plot([0 4],[1.5 1.5],'-','color',[1 .6 0],'linewidth',2);title('bar');" +
        "subplot(2,2,4);scatter(randn(1,200),randn(1,200),200,[1 0 0],'filled','MarkerFaceAlpha',0.3,'MarkerEdgeColor',[0 .6 1],'MarkerEdgeAlpha',0.5);title('scatter');");
      for (const name of ["dark", "light"]) {
        await settle(page);
        await plot.screenshot({ path: path.join(SHOTS, `saydam-2b-${name}.png`) });
        await page.locator("#theme").click();
        await settle(page);
      }
    }
    assert.deepEqual(errors, []);
    console.log("UI FIGURES ALPHA2D PASS: fill/edge/area/bar/scatter scalar alpha blends, alpha 0 face invisible, data-driven alpha keeps PNG.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
