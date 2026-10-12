"use strict";
// Orchestrator only: requires its fresh-copy server, rebuilt bundle and WebGL2.
// Authored without running a browser or creating graphics objects in this lane.
const {assert,expect,openFigures,run,require3D,projected,settle}=require("./figure_ui_support.cjs");
function hue(rgb) {
 const lo=Math.min(...rgb),hi=Math.max(...rgb),d=hi-lo;
 assert(d>20,"level hue was replaced by grey/text colour");
 return (hi===rgb[0]?((rgb[1]-rgb[2])/d+6)%6:hi===rgb[1]?(rgb[2]-rgb[0])/d+2:(rgb[0]-rgb[1])/d+4)*60;
}
(async()=>{
 const {browser,page,errors}=await openFigures();
 try {
  const background=()=>page.evaluate(()=>getComputedStyle(document.body).getPropertyValue("--plot-bg").trim());
  if(await background()!=="#050719")await page.locator("#theme").click();
  await expect.poll(background).toBe("#050719");
  const job=await run(page,"close all;figure('name','Contour hue contrast','position',[0 0 400 400]);[x,y]=meshgrid(linspace(0,1,21));axes('position',[0 0 1 1]);[~,h]=contour3(x,y,x,[.25 .75]);set(h,'linewidth',8);colormap([.05 0 .1;.8 .9 .2]);caxis([0 1]);view(2);axis([0 1 0 1 0 1]);axis off;");
  await require3D(page,"Contour hue contrast");
  await settle(page);
  const rgbs=[];
  for(const [point,original] of [[[.25,.5,.25],[.05,0,.1]],[[.75,.5,.75],[.8,.9,.2]]]){
   const p=await projected(page,point);
   const samples=await page.locator(".figure-3d").evaluate((node,p)=>node.figureTest.pixels([p,{x:p.x+1,y:p.y},{x:p.x,y:p.y+1}]),p);
   for(const rgba of samples){
    assert.equal(rgba[3],255);
    const h=hue(rgba.slice(0,3)),expected=hue(original.map(v=>v*255));
    assert(Math.min(Math.abs(h-expected),360-Math.abs(h-expected))<3,`level hue ${h} != ${expected}`);
   }
   rgbs.push(samples[0].slice(0,3));
  }
  assert.notDeepEqual(rgbs[0],rgbs[1],"distinct levels collapsed");
  // Assert actual producer group identity, not a synthetic grouping assumption.
  const groups=await page.evaluate(async job=>{
   const headers={"X-MF-Token":sessionStorage.getItem("mf-token")};
   const state=await(await fetch("/api/state",{headers})).json();
   if(state.job!==job)throw Error("stale contour artifact");
   const data=await(await fetch("/api/figure?job="+encodeURIComponent(job)+"&file="+encodeURIComponent(state.figures[0].data_file),{headers})).json();
   return data.axes.flatMap(a=>a.series).filter(s=>s.role==="contour").map(s=>s.contour_group);
  },job);
  assert(groups.length>=2&&groups.every(g=>g===0));
  assert.deepEqual(errors,[]);
  console.log("UI CONTOUR CONTRAST PASS: dark and light levels preserve their distinct source hues");
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exit(1);});
