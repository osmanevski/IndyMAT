"use strict";
const { assert, expect, openFigures, run, settle } = require("./figure_ui_support.cjs");

// The shape of the user's own example: three line plots with boxed two-line
// TeX notes, three horizontal histograms with a red curve, a figure title.
const report = [
  "N=64;n=0:N-1;x={mod(n*7,13)/13,mod(n*5,11)/11+mod(n*3,7)/7,6+sin(n)};",
  "figure('name','G1 rapor','Color','w','Position',[100 50 1000 800]);",
  "for k=1:3",
  "axes('Position',[0.10,0.72-0.30*(k-1),0.62,0.20],'FontSize',9);",
  "plot(n,x{k},'k.-','MarkerSize',10);axis([0 N-1 0 12]);xticks(0:16:48);yticks(0:12);grid on;ylabel('Genlik');",
  "title(sprintf('(%c) X',96+k),'FontSize',10);",
  "text(3,11,sprintf('teorik: \\\\sigma = %.3f\\nölçülen: \\\\sigma = %.3f',k/10,std(x{k})),'VerticalAlignment','top','BackgroundColor','w','EdgeColor','k','FontSize',9);",
  "axes('Position',[0.79,0.72-0.30*(k-1),0.17,0.20],'FontSize',9);",
  "histogram(x{k},'Normalization','pdf','Orientation','horizontal','FaceColor',[0.7 0.7 0.7]);",
  "hold on;plot([0 .5 0],[0 6 12],'r','LineWidth',1.5);hold off;ylim([0 12]);yticks(0:12);grid on;title('pdf','FontSize',10);",
  "end",
  "sgtitle('Rastgele değişkenlerin dağılımları','FontWeight','bold');"
].join("\n");

(async () => {
  const { browser, page, errors, execute } = await openFigures();
  try {
    const plot = page.locator(".interactive-plot");
    const canvas = page.locator(".interactive-plot canvas");
    const hook = (name, ...args) => plot.evaluate((node, { name, args }) => node.figureTest[name](...args), { name, args });
    const token = (name) => page.evaluate((name) => getComputedStyle(document.body).getPropertyValue(name).trim().toLowerCase(), name);
    const near = (rgba, expected, label, tolerance = 14) => {
      for (let i = 0; i < 3; i++) assert(Math.abs(rgba[i] - expected[i]) <= tolerance, `${label}: ${rgba} differs from ${expected}`);
    };
    const interactive = async (name, code) => {
      await run(page, code);
      await expect(page.locator("#figure-caption")).toHaveText(name, { timeout: 20000 });
      await expect(canvas).toBeVisible();
      await expect(page.locator("#plot-interactive")).toHaveClass(/active/);
      await settle(page);
    };
    const background = (await token("--plot-bg")).slice(1).match(/../g).map((v) => parseInt(v, 16));
    const ink = await token("--plot-text");

    await interactive("G1 metin", "close all;figure('name','G1 metin');plot(1:3);" +
      "text(1.2,2.8,{'iki satır','\\sigma_x^2 = \\alpha'},'backgroundcolor','w','edgecolor','k','verticalalignment','top');" +
      "text(2.5,1.4,'düz not');text(2.5,1.2,'yeşil','color',[0 .6 0]);" +
      "text(1.6,1.2,'dönük','rotation',90,'backgroundcolor',[1 1 0]);" +
      "text(.5,.95,'üstte \\it eğik','units','normalized','horizontalalignment','center');");
    let texts = await hook("texts");
    assert.deepEqual(texts.map((item) => item.text), ["iki satır\nσx2 = α", "düz not", "yeşil", "dönük", "üstte  eğik"]);
    const [boxed, plain, green, turned, pinned] = texts;
    // Anchor at the data position; a top-aligned box hangs below it.
    const anchor = await hook("project", 0, 1.2, 2.8);
    assert(Math.abs(boxed.x - anchor.x) < 0.5 && Math.abs(boxed.y - anchor.y) < 0.5, "text is not anchored at its data position");
    assert(Math.abs(boxed.box.x - (anchor.x - 3)) < 0.5 && Math.abs(boxed.box.y - (anchor.y - 3)) < 0.5, "top-left box is not the anchor minus the margin");
    assert(boxed.box.w > 30 && boxed.box.h >= 2 * 12 + 6 - 0.5, "two-line box is too small: " + JSON.stringify(boxed.box));
    // The box keeps its serialised white; its text keeps its own black.
    near(await hook("pixel", boxed.box.x + 2, boxed.box.y + 2), [255, 255, 255], "text background");
    near(await hook("pixel", boxed.box.x + boxed.box.w - 2, boxed.box.y + boxed.box.h - 2), [255, 255, 255], "text background corner");
    near(await hook("pixel", boxed.box.x - 4, boxed.box.y - 4), background, "outside the text box");
    assert.equal(boxed.color.toLowerCase(), "#000000");
    const dark = await plot.evaluate((node, box) => {
      let found = false;
      for (let y = box.y + 2; y < box.y + box.h - 2; y++) for (let x = box.x + 2; x < box.x + box.w - 2; x++) {
        const [r, g, b] = node.figureTest.pixel(x, y);
        if (r < 90 && g < 90 && b < 90) found = true;
      }
      return found;
    }, boxed.box);
    assert(dark, "no dark glyph pixels inside the white text box");
    // Text straight on the plot follows the 3:1 rule; an explicit colour stays.
    assert.equal(plain.color.toLowerCase(), ink, "default black text must use the plot text colour on the themed background");
    assert.equal(green.color.toLowerCase(), "#009900");
    assert.equal(turned.rotation, 90);
    const normalized = await hook("box", 0);
    assert(Math.abs(pinned.x - (normalized.x + normalized.w / 2)) < 0.5 && Math.abs(pinned.y - (normalized.y + normalized.h * 0.05)) < 0.5, "normalized text is not placed in axes fractions");
    // No data tips on text.
    const bounds = await canvas.boundingBox();
    const tip = page.locator("#plot-area .plot-data-tip");
    await page.mouse.move(bounds.x + boxed.box.x + boxed.box.w / 2, bounds.y + boxed.box.y + boxed.box.h / 2);
    await settle(page);
    await expect(tip).toBeHidden();
    await page.mouse.move(bounds.x + plain.x + 10, bounds.y + plain.y);
    await settle(page);
    await expect(tip).toBeHidden();
    // Zoom away from the note: it leaves with its anchor; normalized text stays.
    const initial = await canvas.getAttribute("data-view");
    const corner = await hook("project", 0, 2.9, 1.1);
    await page.mouse.move(bounds.x + corner.x, bounds.y + corner.y);
    for (let n = 0; n < 6; n++) await page.mouse.wheel(0, -120);
    await expect.poll(async () => (await hook("texts")).map((item) => item.text).includes("iki satır\nσx2 = α")).toBe(false);
    texts = await hook("texts");
    assert(texts.some((item) => item.text === "üstte  eğik"), "normalized text must not depend on the zoom");
    await canvas.dblclick({ position: { x: corner.x, y: corner.y } });
    await expect.poll(() => canvas.getAttribute("data-view")).toBe(initial);
    assert.equal((await hook("texts")).length, 5);
    // Light theme: the same note, drawn against the light plot background.
    await page.locator("#theme").click();
    await settle(page);
    const light = await hook("texts");
    // Black has enough contrast on the light background, so it stays as serialised.
    assert.equal(light[1].color.toLowerCase(), "#000000");
    assert.notEqual(light[1].color.toLowerCase(), ink);
    near(await hook("pixel", light[0].box.x + 2, light[0].box.y + 2), [255, 255, 255], "text background in the light theme");
    await page.locator("#theme").click();
    await settle(page);

    // errorbar, xline/yline and a title in an invisible axes.
    await interactive("G1 çizgiler", "close all;figure('name','G1 çizgiler');errorbar(1:3,[1 2 3],[.2 .4 .2]);hold on;xline(2,'--','sınır');yline(2.5,'r');" +
      "axes('position',[0 .94 1 .06],'visible','off');text(.5,.5,'Görünmez eksen başlığı','horizontalalignment','center','fontweight','bold','fontsize',12);");
    assert.deepEqual(await hook("axes"), [true, false]);
    texts = await hook("texts");
    assert.deepEqual(texts.map((item) => item.text).sort(), ["Görünmez eksen başlığı", "sınır"]);
    const heading = texts.find((item) => item.axis === 1);
    const width = (await canvas.boundingBox()).width;
    assert(Math.abs(heading.x - width / 2) < 1 && Math.abs(heading.box.x + heading.box.w / 2 - width / 2) < 1, "figure title is not centred");
    // The yline crosses the whole axes in red, also after zooming out.
    const frame = await hook("box", 0);
    const level = await hook("project", 0, 2, 2.5);
    for (const x of [frame.x + 8, frame.x + frame.w - 8]) near(await hook("pixel", x, level.y), [255, 0, 0], "yline at the axes edge", 40);
    await page.mouse.move(bounds.x + frame.x + frame.w / 2, bounds.y + frame.y + frame.h / 2);
    await page.mouse.wheel(0, 240);
    await settle(page);
    const wide = await hook("project", 0, 2, 2.5);
    for (const x of [frame.x + 8, frame.x + frame.w - 8]) near(await hook("pixel", x, wide.y), [255, 0, 0], "yline after zooming out", 40);
    // The invisible title axes is not a zoom target.
    const view = await canvas.getAttribute("data-view");
    await page.mouse.move(bounds.x + heading.x, bounds.y + heading.y);
    await page.mouse.wheel(0, -240);
    await settle(page);
    assert.equal(await canvas.getAttribute("data-view"), view, "the invisible axes reacted to the wheel");

    // The report figure: interactive, six visible axes, notes, bars, title.
    await interactive("G1 rapor", "close all;" + report);
    const axes = await hook("axes");
    assert.equal(axes.filter(Boolean).length, 6, "expected six visible axes: " + JSON.stringify(axes));
    assert.equal(axes.length, 7);
    assert.equal(JSON.parse(await canvas.getAttribute("data-view")).length, 7);
    texts = await hook("texts");
    const notes = texts.filter((item) => item.text.startsWith("teorik: σ = "));
    assert.equal(notes.length, 3);
    assert(notes.every((item) => /^teorik: σ = 0\.\d00\nölçülen: σ = \d\.\d{3}$/.test(item.text)), JSON.stringify(notes.map((item) => item.text)));
    assert(texts.some((item) => item.text === "Rastgele değişkenlerin dağılımları"));
    for (const note of notes) near(await hook("pixel", note.box.x + 2, note.box.y + 2), [255, 255, 255], "note background");
    assert(await hook("faces") >= 6, "histogram bars are missing");
    const count = execute.length;
    const small = await canvas.boundingBox();
    // Enlarge: the same viewer in the modal, inside the dialog, still interactive.
    await page.locator("#plot-expand").click();
    const large = page.locator("#modal[open] .interactive-plot canvas");
    await expect(large).toBeVisible();
    await expect(page.locator("#plot-area canvas")).toHaveCount(0);
    await expect.poll(async () => (await large.boundingBox()).width).toBeGreaterThan(small.width);
    await settle(page);
    const big = await large.boundingBox(), dialog = await page.locator("#modal[open]").boundingBox();
    assert(big.x >= dialog.x && big.x + big.width <= dialog.x + dialog.width + 0.5, "the enlarged figure overflows its dialog");
    assert.equal((await hook("axes")).filter(Boolean).length, 6);
    const enlarged = await hook("texts");
    assert.equal(enlarged.filter((item) => item.text.startsWith("teorik: σ = ")).length, 3);
    assert(enlarged.every((item) => item.box.x + item.box.w <= big.width + 1 || item.axis === 6), "a note left the enlarged canvas");
    const grey = await plot.evaluate((node) => {
      // Grey histogram faces inside the three right-hand axes.
      let count = 0;
      for (const axis of [1, 3, 5]) {
      const box = node.figureTest.box(axis);
      for (let y = box.y + 1; y < box.y + box.h - 1; y += 1) for (let x = box.x + 1; x < box.x + box.w - 1; x += 1) {
        const [r, g, b] = node.figureTest.pixel(x, y);
        if (Math.abs(r - 179) < 10 && Math.abs(g - 179) < 10 && Math.abs(b - 179) < 10) count++;
      }
      }
      return count;
    });
    assert(grey > 60, "no grey histogram faces in the enlarged figure: " + grey);
    const first = await hook("box", 0), before = await large.getAttribute("data-view");
    await page.mouse.move(big.x + first.x + first.w / 2, big.y + first.y + first.h / 2);
    await page.mouse.wheel(0, -240);
    await expect.poll(() => large.getAttribute("data-view")).not.toBe(before);
    const zoomed = JSON.parse(await large.getAttribute("data-view")), start = JSON.parse(before);
    assert(zoomed[0].x[1] - zoomed[0].x[0] < start[0].x[1] - start[0].x[0], "wheel did not zoom the first axes");
    assert.deepEqual(zoomed.slice(1), start.slice(1), "zooming one axes changed another");
    await page.locator("#modal-close").click();
    await expect(page.locator("#modal[open]")).toHaveCount(0);
    await expect(page.locator("#plot-area canvas")).toBeVisible();
    assert.equal(await page.locator(".interactive-plot canvas").count(), 1);
    assert.equal(execute.length, count, "viewing the report submitted /api/execute");

    // Still PNG only: text the viewer cannot place.
    for (const [name, code, reason] of [
      ["G1 piksel", "plot(1:3);text(40,40,'p','units','pixels');", "desteklenmeyen metin (units)"],
      ["G1 latex", "plot(1:3);text(1,2,'$x$','interpreter','latex');", "desteklenmeyen metin (interpreter)"]
    ]) {
      await run(page, `close all;figure('name','${name}');${code}`);
      await expect(page.locator("#figure-caption")).toContainText(name, { timeout: 20000 });
      await expect(page.locator("#figure-caption")).toContainText("Yalnızca PNG görünümü kullanılabilir: " + reason);
      await expect(page.locator("#plot-area img")).toBeVisible();
      await expect(page.locator("#plot-interactive")).toBeDisabled();
    }
    assert.deepEqual(errors, []);
    console.log("UI FIGURES 2D TEXT PASS: boxed multi-line TeX text, colours in both themes, no tips on text, zoom, xline/yline/errorbar, invisible title axes, six-axes report figure and its enlarge modal, PNG reasons in Turkish.");
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
