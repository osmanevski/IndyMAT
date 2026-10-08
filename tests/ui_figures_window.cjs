"use strict";
const { assert, expect, openFigures, run, require3D, view, mode, drag, settle, projected } = require("./figure_ui_support.cjs");
async function ratioCheck(page) {
  const values = await page.locator('.interactive-plot').evaluate(node => ({vp:node.figureTest.viewport(),size:node.figureTest.sourceSize(),bounds:node.getBoundingClientRect().toJSON()}));
  assert.equal(values.size.length,2);
  assert(Math.abs(values.vp.width/values.vp.height-values.size[0]/values.size[1])<.002,'source aspect changed');
  assert(Math.abs(values.vp.x+values.vp.width/2-values.bounds.width/2)<2,'horizontal drift');
  assert(Math.abs(values.vp.y+values.vp.height/2-values.bounds.height/2)<2,'vertical drift');
}
(async()=>{
 const {browser,page,errors,execute}=await openFigures();
 try {
  await run(page,"close all;figure('name','Window 2D');plot(1:5,[2 1 4 3 5]);");
  const canvas=page.locator('.interactive-plot canvas');await expect(canvas).toBeVisible();
  for(const size of [{width:1800,height:700},{width:900,height:1400}]) {await page.setViewportSize(size);await settle(page);await ratioCheck(page);}
  const initial=await canvas.getAttribute('data-view'),count=execute.length;
  await page.locator('#plot-expand').click();
  const modal=page.locator('#modal[open]');
  await expect(modal.locator('.figure-window-reset')).toBeVisible();
  await expect(modal.locator('.figure-window-reset')).toHaveAttribute('aria-label','Grafik görünümünü sıfırla');
  assert.equal(await modal.evaluate(n=>getComputedStyle(n).resize),'both');
  for(const [w,h] of [[740,480],[500,720],[900,400]]) {await modal.evaluate((n,[w,h])=>{n.style.width=w+'px';n.style.height=h+'px'},[w,h]);await settle(page);await ratioCheck(page);}
  const b=await canvas.boundingBox();await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.wheel(0,-180);
  await expect.poll(()=>canvas.getAttribute('data-view')).not.toBe(initial);
  await modal.locator('.figure-window-reset').click();await expect.poll(()=>canvas.getAttribute('data-view')).toBe(initial);
  await modal.locator('.figure-window-fullscreen').click();await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(true);
  await settle(page);await ratioCheck(page);await expect(modal.locator('.figure-window-reset')).toBeVisible();
  await modal.locator('.figure-window-fullscreen').click();await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(false);
  await page.locator('#modal-close').click();
  await run(page,"close all;figure('name','Window 3D');plot3([0 1 2],[0 1 0],[0 1 2],'-o','markersize',12,'displayname','Known vertex');axis([0 2 0 1 0 2]);");
  await require3D(page,'Window 3D');await ratioCheck(page);const initial3d=await view(page);
  await page.locator('#plot-expand').click();await mode(page,'rotate');await drag(page,36,14);assert.notDeepEqual(await view(page),initial3d);
  await modal.locator('.figure-window-fullscreen').click();await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(true);await settle(page);await ratioCheck(page);
  await modal.locator('.figure-window-reset').click();assert.deepEqual(await view(page),initial3d);
  await modal.locator('.figure-window-fullscreen').click();await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(false);
  await mode(page,'tips');const v=await projected(page,[1,1,1]);const wrap=await page.locator('.figure-3d').boundingBox();await page.mouse.click(wrap.x+v.x,wrap.y+v.y);await expect(page.locator('.figure-pinned-tip')).toHaveCount(1);
  await modal.evaluate(n=>{n.style.width='600px';n.style.height='650px'});await settle(page);
  const point=await projected(page,[1,1,1]);const tip=await page.locator('.figure-pinned-tip').evaluate(n=>({x:parseFloat(n.style.left),y:parseFloat(n.style.top)}));assert(Math.abs(tip.x-point.x-9)<3,'pinned tip drifted after resize');
  await modal.locator('.figure-window-reset').click();await expect(page.locator('.figure-pinned-tip')).toHaveCount(0);
  await page.locator('#modal-close').click();assert.equal(execute.length,count+1);assert.deepEqual(errors,[]);
  await run(page,"close all;figure('name','Wide title','position',[100 100 640 220]);subplot(1,2,1);surf(peaks(8));title('Surface');subplot(1,2,2);plot(1:3);sgtitle({'Shared title','Second line'});");
  await require3D(page,'Wide title');await page.setViewportSize({width:900,height:1400});await settle(page);await ratioCheck(page);
  await page.locator('#plot-expand').click();await modal.evaluate(n=>{n.style.width='500px';n.style.height='740px'});await settle(page);await ratioCheck(page);await expect(modal.locator('.figure-window-reset')).toBeVisible();
  assert.deepEqual(errors,[]);await page.locator('#modal-close').click();
  console.log('UI FIGURE WINDOW PASS: source aspect/centering, resizable modal/fullscreen, local reset 2D/3D, pinned tips and mixed 3D/2D title.');
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
