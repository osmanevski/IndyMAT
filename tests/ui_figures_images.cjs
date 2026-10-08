"use strict";
const { assert, expect, openFigures, run, settle } = require("./figure_ui_support.cjs");
(async () => {
  const { browser, page, errors, execute } = await openFigures();
  try {
    const plot = page.locator("#plot-area .interactive-plot"), canvas = plot.locator("canvas"), tip = plot.locator(".plot-data-tip");
    const hook = (name, ...args) => plot.evaluate((node, { name, args }) => node.figureTest[name](...args), { name, args });
    const interactive = async (name, code) => {
      await run(page, `close all;figure('name','${name}');${code}`);
      await expect(page.locator("#figure-caption")).toHaveText(name, { timeout: 20000 });
      await expect(canvas).toBeVisible();await expect(page.locator("#plot-interactive")).toHaveClass(/active/);await settle(page);
    };
    const pixel = async (x, y, rgb, label) => {
      const p = await hook("project", 0, x, y), actual = await hook("pixel", p.x, p.y);
      for (let i = 0; i < 3; i++) assert(Math.abs(actual[i] - rgb[i]) <= 3, `${label}: ${actual} differs from ${rgb}`);
      return p;
    };
    const hover = async (x, y) => {
      const p = await hook("project", 0, x, y), b = await canvas.boundingBox();
      await page.mouse.move(b.x + p.x, b.y + p.y);return { x: b.x + p.x, y: b.y + p.y };
    };
    const background = await page.evaluate(() => getComputedStyle(document.body).getPropertyValue("--plot-bg").trim().slice(1).match(/../g).map((n) => parseInt(n,16)));
    const map = "colormap([1 0 0;0 1 0;0 0 1;0 1 1;1 0 1;1 1 0]);clim([1 7]);";
    await interactive("Image extents", "imagesc([30 10],[8 4],[1 2 3;4 5 6]);" + map + "axis([0 40 0 12]);set(gca,'position',[.15 .15 .7 .7]);");
    for (const [x,y,rgb] of [[30,8,[255,0,0]],[20,8,[0,255,0]],[10,8,[0,0,255]],[30,4,[0,255,255]],[20,4,[255,0,255]],[10,4,[255,255,0]]]) await pixel(x,y,rgb,"scaled descending image");
    await pixel(35.4, 8, background, "outside first pixel edge");await pixel(34.6, 8,[255,0,0],"inside first half-cell edge");
    await pixel(4.6,4,background,"outside last pixel edge");await pixel(5.4,4,[255,255,0],"inside last half-cell edge");
    await pixel(20,10.4,background,"above row bound");await pixel(20,9.6,[0,255,0],"inside row bound");
    await hover(20,4);await expect(tip).toHaveText("Satır = 2, Sütun = 2, Değer = 5");
    const requests = execute.length, initial = await canvas.getAttribute("data-view");
    await hover(20,4);await page.mouse.wheel(0,-150);await expect.poll(() => canvas.getAttribute("data-view")).not.toBe(initial);await settle(page);await pixel(20,4,[255,0,255],"zoomed pixel");
    const before = await hover(20,4);await page.keyboard.down("Shift");await page.mouse.down();await page.mouse.move(before.x+35,before.y+15,{steps:4});await page.mouse.up();await page.keyboard.up("Shift");await settle(page);await pixel(20,4,[255,0,255],"panned pixel");
    const after = await hover(20,4);assert(Math.abs(after.x-before.x-35)<2);assert(Math.abs(after.y-before.y-15)<2);
    await canvas.dblclick();await expect.poll(() => canvas.getAttribute("data-view")).toBe(initial);
    assert.equal(execute.length,requests,"raster tools submitted Octave work");
    await interactive("Image reversed axes", "imagesc([30 10],[8 4],[1 2 3;4 5 6]);" + map + "set(gca,'xdir','reverse','ydir','normal');");
    await pixel(30,8,[255,0,0],"reversed x normal y");await pixel(10,4,[255,255,0],"last reversed pixel");
    await interactive("RGB image", "rgb=cat(3,uint8([255 0;0 255]),uint8([0 255;0 255]),uint8([0 0;255 255]));image(rgb);set(gca,'position',[.15 .15 .7 .7]);colormap(gray(2));clim([100 200]);");
    for (const [x,y,rgb] of [[1,1,[255,0,0]],[2,1,[0,255,0]],[1,2,[0,0,255]],[2,2,[255,255,255]]]) await pixel(x,y,rgb,"truecolor independent of map");
    await hover(1,2);await expect(tip).toHaveText("Satır = 2, Sütun = 1, Değer = [0, 0, 255]");
    // This Qt print build misrenders the 2-column RGB reference. The measured
    // 4-column PNG agrees with CData; the 2x2 viewer above still honors CData.
    await interactive("RGB PNG reference", "rgb=cat(3,uint8([255 0;0 255]),uint8([0 255;0 255]),uint8([0 0;255 255]));image(repelem(rgb,1,2));set(gca,'position',[.15 .15 .7 .7]);");
    // Compare the actual Qt-produced PNG's cell interiors, not screenshots.
    await page.locator("#plot-png").click();const img=page.locator("#plot-area img");await expect(img).toBeVisible();
    const native = await img.evaluate(async (img) => {
      await img.decode();const canvas=document.createElement("canvas");canvas.width=img.naturalWidth;canvas.height=img.naturalHeight;const c=canvas.getContext("2d");c.drawImage(img,0,0);
      return [[.325,.325],[.675,.325],[.325,.675],[.675,.675]].map(([x,y])=>[...c.getImageData(Math.floor(canvas.width*x),Math.floor(canvas.height*y),1,1).data]);
    });
    assert.deepEqual(native.map((v)=>v.slice(0,3)),[[255,0,0],[0,255,0],[0,0,255],[255,255,255]],"Qt PNG RGB cell reference");
    await page.locator("#plot-interactive").click();await expect(canvas).toBeVisible();
    for (const dtype of ["uint8","uint16","double","single"]) {
      await interactive("Indexed "+dtype,"image("+dtype+"([0 1;2 3]));set(gca,'position',[.15 .15 .7 .7]);colormap([1 0 0;0 1 0;0 0 1]);");
      const integer=dtype.startsWith("uint");await pixel(1,1,[255,0,0],"direct first");await pixel(2,1,integer?[0,255,0]:[255,0,0],"direct class index");await pixel(1,2,integer?[0,0,255]:[0,255,0],"direct bottom");await pixel(2,2,[0,0,255],"direct clipped");
    }
    await interactive("Image aspect", "imagesc([30 10],[8 4],[1 2 3;4 5 6]);axis image;"+map+"colorbar;hold on;plot([10 30],[4 8],'w','linewidth',2);");
    let b=await hook("box",0);assert(Math.abs(b.w/b.h-30/8)<.001,"axis image data aspect");await pixel(20,8,[0,255,0],"mixed image and line");
    await page.locator("#plot-expand").click();const enlarged=page.locator("#modal[open] .interactive-plot");await expect(enlarged.locator("canvas")).toBeVisible();await settle(page);
    b=await enlarged.evaluate((node)=>node.figureTest.box(0));assert(Math.abs(b.w/b.h-30/8)<.001,"enlarged aspect");
    const p=await enlarged.evaluate((node)=>node.figureTest.project(0,20,8));const rgba=await enlarged.evaluate((node,p)=>node.figureTest.pixel(p.x,p.y),p);assert.deepEqual(rgba.slice(0,3),[0,255,0]);await page.locator("#modal-close").click();
    await page.setViewportSize({width:1280,height:820});await settle(page);b=await hook("box",0);assert(Math.abs(b.w/b.h-30/8)<.001,"resized axis image");
    await interactive("Equal image", "imagesc([1 2 3]);axis equal;");b=await hook("box",0);const view=JSON.parse(await canvas.getAttribute("data-view"))[0];assert(Math.abs(b.w/b.h-(view.x[1]-view.x[0])/(view.y[1]-view.y[0]))<.001,"axis equal data aspect");
    await interactive("Scalar image", "imagesc(7);colormap([1 0 0;0 1 0]);clim([7 8]);");await pixel(1,1,[255,0,0],"singleton");
    await interactive("Top image tip", "imagesc([1 2;3 4]);hold on;plot(1,1,'ro','markersize',8);image([5 6;7 8]);");
    await hover(1,1);await expect(tip).toHaveText("Satır = 1, Sütun = 1, Değer = 5");
    await interactive("Top line tip", "imagesc([1 2;3 4]);hold on;image([5 6;7 8]);plot(1,1,'ro','markersize',8,'displayname','Visible overlay');");
    await hover(1,1);await expect(tip).toHaveText("Visible overlay: x = 1, y = 1");
    await run(page,"close all;figure('name','Alpha image');h=imagesc([1 2;3 4]);set(h,'alphadata',.5);");await expect(page.locator("#figure-caption")).toContainText("saydamlık");await expect(page.locator("#plot-interactive")).toBeDisabled();await expect(page.locator("#plot-area img")).toBeVisible();
    assert.deepEqual(errors,[]);console.log("UI IMAGE PASS: Qt/native RGB, scaled/direct class colors, descending centers, half cells, directions, tips, zoom/pan, equal/image aspect, mixed line/colorbar, resize/enlarge, alpha fallback.");
  } finally { await browser.close(); }
})().catch((error)=>{console.error(error);process.exit(1);});
