"use strict";
const assert=require("node:assert/strict"),fs=require("node:fs");
const c=require("../frontend/figure_color_utils.cjs");
const maps=require("./fixtures/contour_colormaps12.json");
// Numeric octave-cli map samples, no graphics or pixel measurements.
const css=fs.readFileSync("static/style.css","utf8");
const palettes=[...css.matchAll(/--plot-bg:(#[0-9a-f]{6});--plot-text:(#[0-9a-f]{6})/g)]
 .map(([,bg,text])=>[bg,text].map(s=>s.slice(1).match(/../g).map(v=>parseInt(v,16)/255)));
function hs(rgb) {
 const lo=Math.min(...rgb),hi=Math.max(...rgb),d=hi-lo,l=(hi+lo)/2;
 const h=d===0?0:hi===rgb[0]?((rgb[1]-rgb[2])/d+6)%6:hi===rgb[1]?(rgb[2]-rgb[0])/d+2:(rgb[0]-rgb[1])/d+4;
 return [h,d===0?0:d/(1-Math.abs(2*l-1))];
}
function check(input,bg,text) {
 const saved=structuredClone(input),out=c.adjustContourPalette(input,bg,text);
 assert.deepEqual(input,saved);
 out.forEach((rgb,i)=>{
  assert(rgb.every(v=>Number.isFinite(v)&&v>=0&&v<=1));
  assert(c.contrast(rgb,bg)>=3-1e-3);
  const a=hs(input[i]),b=hs(rgb);
  if(a[1]>0)assert(Math.min(Math.abs(a[0]-b[0]),6-Math.abs(a[0]-b[0]))<1e-9);
  assert(Math.abs(a[1]-b[1])<1e-9);
 });
 for(let i=0;i<input.length;i++)for(let j=0;j<i;j++){
  if(JSON.stringify(input[i])!==JSON.stringify(input[j]))assert.notDeepEqual(out[i],out[j]);
  const before=c.luminance(input[i])-c.luminance(input[j]);
  assert(before*(c.luminance(out[i])-c.luminance(out[j]))>=-1e-12);
 }
 if(input.every(rgb=>c.contrast(rgb,bg)>=3))assert.strictEqual(out,input);
 else {
  const lighter=c.contrast([1,1,1],bg)>=c.contrast([0,0,0],bg);
  const threshold=lighter?3*(c.luminance(bg)+.05)-.05:1-((c.luminance(bg)+.05)/3-.05);
  const xs=input.map(rgb=>lighter?c.luminance(rgb):1-c.luminance(rgb)),end=Math.max(...xs);
  const pivot=end>threshold?Math.min(threshold+.2,end):Math.min(1,threshold+.2);
  xs.forEach((x,i)=>{if(x>=pivot)assert.strictEqual(out[i],input[i]);});
 }
 return out;
}
for(const [bg,text] of palettes) {
 for(const [name,map] of Object.entries(maps)){check(map,bg,text);console.log("PASS",name,bg);}
 for(const input of [[[0,0,0]],[[1,1,1]],[[.1,0,.2],[.7,.8,.2]],
  [[.01,.01,.01],[.1,.1,.1]],[[.9,.9,.9],[1,1,1]],[[0,0,0],[0,0,0]],
  [text,text]])check(input,bg,text);
}
const [bg,text]=palettes[1];
const series=(level,group=0)=>({role:"contour",contour_group:group,level,
 contour_color:{mode:"flat",association:"level",encoding:"indexed",mapping:"direct",cdata_class:"double",data:{shape:[1,1],values:[level]}}});
const a={colormap:{shape:[2,3],values:[.05,.8,0,.9,.1,.2]},clim:[1,2],series:[series(1),series(2)]};
const display=c.contourDisplayColors(a,bg,text);
assert.equal(display.size,2);
const alone={...a,series:[series(1,1),series(2,2)]};
assert.deepEqual(c.contourDisplayColors(alone,bg,text).get(alone.series[0]),c.adjustContourPalette([[.05,0,.1]],bg,text)[0]);
const manual={role:"contour",contour_color:{mode:"constant",association:"constant",rgb:[0,0,0]}};
assert.equal(c.contourDisplayColors({...a,series:[manual,{role:"line"}]},bg,text).size,0);
const stops=c.colorbarGradient(a);
c.contourDisplayColors(a,bg,text);
assert.deepEqual(c.colorbarGradient(a),stops);
console.log("PASS single/two/all-readable/all-unreadable/greys; group isolation; manual colours and colourbars untouched");
