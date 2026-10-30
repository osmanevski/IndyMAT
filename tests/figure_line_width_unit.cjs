"use strict";
// LineWidth is exported in points; the 2-D viewer converts it to CSS pixels
// (1 pt = 96/72 px) at the figure's on-screen scale, at least one device pixel.
const assert=require("node:assert/strict"),path=require("node:path"),Module=require("node:module"),esbuild=require("esbuild");
(async()=>{
 const registry={};global.figureLineWidthHarness={registry};
 const bundle=await esbuild.build({stdin:{contents:'import "./figures.js";',resolveDir:path.resolve("frontend")},bundle:true,write:false,platform:"node",format:"cjs",plugins:[{name:"boundaries",setup(build){
 build.onResolve({filter:/^\.\/(state|registry|figure3d|figure_tools|figure_window_controls|i18n)\.js$/},args=>({path:path.basename(args.path),namespace:"stub"}));
 build.onLoad({filter:/.*/,namespace:"stub"},args=>({contents:{"state.js":"export default {};","registry.js":"export default globalThis.figureLineWidthHarness.registry;","figure3d.js":"export class Figure3D {};export function drawColorbars() {}","figure_tools.js":"export const figureReasonText=()=>'';export const figureReductionText=()=>'';export function unmountFigureTools(){}","figure_window_controls.js":"export const mountFigureWindowControls=()=>()=>{};","i18n.js":"export const t=x=>x;export const onLanguageChange=()=>()=>{};"}[args.path],loader:"js"}));}}]});
 const m=new Module(path.resolve("frontend/line_width_test.js"),module);m._compile(bundle.outputFiles[0].text,path.resolve("frontend/line_width_test.js"));
 const proto=registry.InteractiveFigure.prototype;
 const view=(figureWidth,viewWidth)=>Object.assign(Object.create(proto),{data:{source:{figure_size:[figureWidth,420]}},figureViewport:{x:0,y:0,width:viewWidth,height:300}});
 const close=(a,b)=>assert(Math.abs(a-b)<1e-12,`${a} vs ${b}`);
 close(view(560,560).lineWidthPx(1.5),2);
 close(view(560,280).lineWidthPx(3),2);
 close(view(560,442).lineWidthPx(1.5),1.5*96/72*442/560);
 // 0.5 pt at panel scale is thinner than a pixel: clamped to one device pixel.
 close(view(560,442).lineWidthPx(0.5),1);
 globalThis.devicePixelRatio=2;close(view(560,442).lineWidthPx(0.5),0.5*96/72*442/560);delete globalThis.devicePixelRatio;
 // Missing or invalid widths use MATLAB's default 0.5 pt; no figure size means scale 1.
 close(view(560,1120).lineWidthPx(undefined),0.5*96/72*2);
 close(Object.assign(Object.create(proto),{data:{},figureViewport:{width:300}}).lineWidthPx(3),4);
 console.log("PASS line width: points to CSS px at figure scale, one-device-pixel floor");
})().catch(error=>{console.error(error);process.exit(1);});
