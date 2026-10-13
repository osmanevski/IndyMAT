"use strict";
// Diagnostic only, deliberately not a ui_ gate file. Run in an orchestrator
// fresh copy after building, with that copy's server already running.
const { openFigures, run } = require('./figure_ui_support.cjs');
(async () => {
 const {browser,page,errors}=await openFigures();
 try {
  await run(page,"close all;figure('name','D26 timing diagnostic');patch('vertices',[0 0 1;1 0 1;1 1 1;0 1 1],'faces',1:4,'facecolor',[.2 .6 .8],'facealpha',.5,'edgecolor','none');view(2);axis([0 1 0 1 0 2]);axis off;");
  const sample=async label=>{
   await page.waitForFunction(()=>{
    const node=document.querySelector('.figure-3d'),d=node?.figureTest?.transparency();
    return d?.frames>0&&!d.pending || !node&&document.querySelector('#plot-area img');
   },undefined,{timeout:10000});
   const result=await page.evaluate(()=>{
    const node=document.querySelector('.figure-3d');
    if(!node)return {fallback:document.querySelector('#figure-caption')?.textContent};
    const gl=node.querySelector('.figure-webgl').getContext('webgl2'),ext=gl.getExtension('WEBGL_debug_renderer_info');
    return {renderer:gl.getParameter(ext?ext.UNMASKED_RENDERER_WEBGL:gl.RENDERER),width:gl.drawingBufferWidth,height:gl.drawingBufferHeight,...node.figureTest.transparency()};
   });
   console.log(label,JSON.stringify(result));return !result.fallback;
  };
  if(await sample('INITIAL'))for(let i=0;i<7;i++){
   await page.locator('#rotate-right').click();
   await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
   if(!await sample('FRAME '+(i+1)))break;
  }
  console.log('PAGE_ERRORS',JSON.stringify(errors));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
