"use strict";
const { assert, expect, openFigures, run, settle } = require("./figure_ui_support.cjs");

(async () => {
  const { browser, page, errors, execute } = await openFigures();
  try {
    const plot = page.locator("#plot-area .interactive-plot");
    const canvas = page.locator("#plot-area .interactive-plot canvas");
    const hook = (name, ...args) => plot.evaluate((node, { name, args }) => node.figureTest[name](...args), { name, args });
    const background = await page.evaluate(() => getComputedStyle(document.body).getPropertyValue("--plot-bg").trim().slice(1).match(/../g).map((v) => parseInt(v, 16)));
    const interactive = async (name, code) => {
      await run(page, `close all;figure('name','${name}');${code}`);
      await expect(page.locator("#figure-caption")).toHaveText(name, { timeout: 20000 });
      await expect(canvas).toBeVisible();
      await expect(page.locator("#plot-interactive")).toHaveClass(/active/);
      await settle(page);
    };
    // Real canvas pixels at a data position, with its two neighbours.
    const pixels = async (x, y, expected, label, tolerance = 14, axis = 0) => {
      const p = await hook("project", axis, x, y);
      for (const [dx, dy] of [[0, 0], [1, 0], [0, 1]]) {
        const rgba = await hook("pixel", p.x + dx, p.y + dy);
        for (let i = 0; i < 3; i++) assert(Math.abs(rgba[i] - expected[i]) <= tolerance, `${label}: pixel ${rgba} at data (${x}, ${y}) differs from ${expected}`);
      }
      return p;
    };
    const hover = async (x, y, axis = 0) => {
      const p = await hook("project", axis, x, y), box = await canvas.boundingBox();
      await page.mouse.move(box.x + p.x, box.y + p.y);
      return { x: box.x + p.x, y: box.y + p.y };
    };

    // bar: Octave's own rectangles (centre 20, width 5: 17.5..22.5), colour kept.
    await interactive("G1 bar", "bar([10 20 30],[1 3 2],0.5,'facecolor',[1 0 0],'displayname','seri');");
    assert.equal(await hook("faces"), 3);
    await pixels(20, 1.5, [255, 0, 0], "bar interior");
    await pixels(18, 2.9, [255, 0, 0], "bar interior near its corner");
    await pixels(15, 1.5, background, "gap between bars");
    await pixels(20, 3.15, background, "above the bar");
    await pixels(10, 0.5, [255, 0, 0], "first bar");
    await pixels(10, 1.5, background, "above the first bar");
    const count = execute.length;
    await hover(20, 1.5);
    const tip = page.locator("#plot-area .plot-data-tip");
    await expect(tip).toBeVisible();
    await expect(tip).toHaveText("seri: x = 20, y = 3");
    await hover(15, 2.5);
    await expect(tip).toBeHidden();
    // Zoom: the bars stay aligned with the axis they are drawn against.
    const initial = await canvas.getAttribute("data-view");
    await hover(20, 1.5);
    await page.mouse.wheel(0, -360);
    await expect.poll(() => canvas.getAttribute("data-view")).not.toBe(initial);
    await settle(page);
    const view = JSON.parse(await canvas.getAttribute("data-view"))[0];
    const before = JSON.parse(initial)[0];
    assert(view.x[0] > before.x[0] && view.x[1] < before.x[1] && view.x[0] < 17.5 && view.x[1] > 22.5, "zoom did not narrow x around the bar: " + JSON.stringify(view));
    assert(view.y[1] - view.y[0] < before.y[1] - before.y[0], "zoom did not narrow y");
    const left = await hook("project", 0, 17.5, 1.5), right = await hook("project", 0, 22.5, 1.5);
    for (const [p, inside] of [[left, 1], [right, -1]]) {
      const inner = await hook("pixel", p.x + inside * 4, p.y), outer = await hook("pixel", p.x - inside * 4, p.y);
      assert(inner[0] > 240 && inner[1] < 15, "bar edge moved inwards after zoom: " + inner);
      assert(Math.abs(outer[0] - background[0]) <= 14 && Math.abs(outer[2] - background[2]) <= 14, "bar edge moved outwards after zoom: " + outer);
    }
    await pixels(20, (view.y[0] + view.y[1]) / 2, [255, 0, 0], "zoomed bar interior");
    await pixels(15.5, (view.y[0] + view.y[1]) / 2, background, "gap beside the zoomed bar");
    await hover(20, 2);
    await expect(tip).toHaveText("seri: x = 20, y = 3");
    // Shift-drag pans the bars with the axis.
    const start = await hover(20, 2);
    await page.keyboard.down("Shift");
    await page.mouse.down();
    await page.mouse.move(start.x + 50, start.y, { steps: 4 });
    await page.mouse.up();
    await page.keyboard.up("Shift");
    await settle(page);
    const moved = await hook("project", 0, 20, 2);
    const box = await canvas.boundingBox();
    assert(Math.abs(box.x + moved.x - (start.x + 50)) < 2, "pan did not carry the bar with the axis");
    await pixels(20, 2, [255, 0, 0], "panned bar interior");
    await canvas.dblclick({ position: { x: moved.x, y: moved.y } });
    await expect.poll(() => canvas.getAttribute("data-view")).toBe(initial);
    assert.equal(execute.length, count, "viewing bars submitted /api/execute");

    // Grouped bars with a legend, negative values against a base value.
    await interactive("G1 grouped", "bar([1 2;3 -1]);legend('bir','iki');");
    await pixels(0.82, 0.5, [0, 114, 189], "first series");
    await pixels(1.18, 1.5, [217, 83, 25], "second series");
    await pixels(2.18, -0.5, [217, 83, 25], "negative bar below the baseline");
    await pixels(2.18, 0.5, background, "above the negative bar");
    await pixels(1.5, 1, background, "between the groups");
    await hover(2.18, -0.5);
    await expect(tip).toHaveText("iki: x = 2, y = -1");
    // The legend (top right of the axes) shows a filled swatch per bar series.
    const legend = await hook("box", 0);
    const swatches = await plot.evaluate((node, area) => {
      const found = { first: false, second: false };
      for (let y = area.y + 6; y < area.y + 46; y++) for (let x = area.x + area.w * 0.45; x < area.x + area.w - 6; x++) {
        const [r, g, b] = node.figureTest.pixel(x, y);
        if (Math.abs(r - 0) < 14 && Math.abs(g - 114) < 14 && Math.abs(b - 189) < 14) found.first = true;
        if (Math.abs(r - 217) < 14 && Math.abs(g - 83) < 14 && Math.abs(b - 25) < 14) found.second = true;
      }
      return found;
    }, legend);
    assert.deepEqual(swatches, { first: true, second: true }, "legend swatches of the bar series are missing");

    // Stacked horizontal bars: the second series starts where the first ends.
    await interactive("G1 barh", "barh([1 2;3 4],'stacked');");
    await pixels(0.5, 1, [0, 114, 189], "first segment");
    await pixels(2, 1, [217, 83, 25], "stacked segment");
    await pixels(5, 2, [217, 83, 25], "second stacked segment");
    await pixels(5, 1, background, "right of the short bar");
    await pixels(2, 1.5, background, "between the rows");
    await hover(5, 2);
    await expect(tip).toHaveText("x = 4, y = 2");

    // hist: one flat scalar colour through the colormap; tips give bin edges.
    await interactive("G1 hist", "hist([1 2 2 3 3 3],3);");
    await pixels(2, 1, [33, 145, 140], "hist bar");
    await pixels(1.3, 0.5, [33, 145, 140], "first hist bar");
    await pixels(1.3, 1.5, background, "above the first hist bar");
    await hover(2.6, 1);
    await expect(tip).toHaveText("x = [2.33333, 3], y = 3");
    // histogram (bar group with touching bars) reports its edges too.
    await interactive("G1 histogram", "histogram([1 2 2 3 3 3],'BinEdges',[0.5 1.5 2.5 3.5],'FaceColor',[.7 .7 .7]);");
    await pixels(2, 1, [179, 179, 179], "histogram bar");
    await hover(3, 1);
    await expect(tip).toHaveText("x = [2.5, 3.5], y = 3");

    // area (stacked) and fill/patch polygons.
    await interactive("G1 area", "area(1:3,[1 2;2 1;3 3]);");
    await pixels(2, 1, [0, 114, 189], "lower area");
    await pixels(2, 2.5, [217, 83, 25], "upper area");
    await pixels(1.5, 5, background, "above the areas");
    await interactive("G1 fill", "fill([0 1 1],[0 0 1],'r');hold on;patch([2 3 3 2],[0 0 1 1],[0 .6 0],'edgecolor','none');axis([0 4 0 2]);");
    await pixels(0.75, 0.25, [255, 0, 0], "triangle interior");
    await pixels(0.25, 0.75, background, "outside the triangle");
    await pixels(2.5, 0.5, [0, 153, 0], "patch interior");
    await pixels(3.5, 0.5, background, "right of the patch");
    await hover(2.5, 0.5);
    await expect(tip).toHaveText("x = [2, 3], y = 1");

    // The PNG view is still Octave's own rendering of the same figure.
    await page.locator("#plot-png").click();
    await expect(page.locator("#plot-area img")).toBeVisible();
    await page.locator("#plot-area img").evaluate((img) => img.decode());
    await page.locator("#plot-interactive").click();
    await expect(canvas).toBeVisible();

    // Next to a 3D axes the same painter draws the bars on the viewer's overlay.
    await run(page, "close all;figure('name','G1 karma');subplot(1,2,1);surf([1 3 5;2 4 6]);subplot(1,2,2);bar([1 3 2],'facecolor',[1 0 0]);");
    await page.waitForFunction(() => document.querySelector(".figure-3d")?.figureTest && document.querySelector("#figure-caption").textContent === "G1 karma", null, { timeout: 20000 });
    await expect(page.locator("#plot-interactive")).toHaveClass(/active/);
    await settle(page);
    const red = await page.locator(".figure-3d").evaluate((node) => {
      let count = 0;
      for (const canvas of node.querySelectorAll("canvas")) {
        const context = canvas.getContext("2d");
        if (!context) continue;
        const data = context.getImageData(Math.floor(canvas.width / 2), 0, Math.floor(canvas.width / 2), canvas.height).data;
        for (let i = 0; i < data.length; i += 4) if (data[i] > 240 && data[i + 1] < 15 && data[i + 2] < 15 && data[i + 3] === 255) count++;
      }
      return count;
    });
    assert(red > 500, "bars beside a 3D axes were not painted: " + red);

    // Still PNG only, with a precise Turkish reason.
    for (const [name, code, reason] of [
      ["G1 interp", "fill([0 1 1],[0 0 1],[1 2 3]);", "ara değerli renkler"],
      ["G1 alpha", "area(1:3,[1 2 3],'facealpha',0.4);", "saydamlık"],
      ["G1 faces", "patch([0 1 1;2 3 3]',[0 0 1;0 0 1]',[1;2]);", "yüz veya köşe başına yama renkleri"],
      ["G1 pie", "pie([1 2 3]);", "desteklenmeyen dolgulu şekil (dataaspectratio)"]
    ]) {
      await run(page, `close all;figure('name','${name}');${code}`);
      await expect(page.locator("#figure-caption")).toContainText(name, { timeout: 20000 });
      await expect(page.locator("#figure-caption")).toContainText("Yalnızca PNG görünümü kullanılabilir: " + reason);
      await expect(page.locator("#plot-area img")).toBeVisible();
      await expect(page.locator("#plot-interactive")).toBeDisabled();
      await expect(page.locator("#plot-area canvas")).toHaveCount(0);
    }
    assert.deepEqual(errors, []);
    console.log("UI FIGURES 2D SHAPES PASS: bar, grouped, stacked barh, hist, histogram, area, fill/patch pixels; bar tips; zoom and pan keep bars on the axis; PNG reasons in Turkish.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
