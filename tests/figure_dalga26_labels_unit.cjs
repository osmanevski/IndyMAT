"use strict";
const assert=require("node:assert/strict"),fs=require("node:fs"),path=require("node:path"),Module=require("node:module"),esbuild=require("esbuild");
const {validate}=require("../frontend/figure_data_utils.cjs");
(async()=>{
 const registry={};global.figureLabelsHarness={registry};
 const bundle=await esbuild.build({stdin:{contents:'import "./figures.js";',resolveDir:path.resolve("frontend")},bundle:true,write:false,platform:"node",format:"cjs",plugins:[{name:"boundaries",setup(build){
 build.onResolve({filter:/^\.\/(state|registry|figure3d|figure_tools|figure_window_controls|i18n)\.js$/},args=>({path:path.basename(args.path),namespace:"stub"}));
 build.onLoad({filter:/.*/,namespace:"stub"},args=>({contents:{"state.js":"export default {};","registry.js":"export default globalThis.figureLabelsHarness.registry;","figure3d.js":"export class Figure3D {};export function drawColorbars() {}","figure_tools.js":"export const figureReasonText=()=>'';export const figureReductionText=()=>'';export function unmountFigureTools(){}","figure_window_controls.js":"export const mountFigureWindowControls=()=>()=>{};","i18n.js":"export const t=x=>x;export const onLanguageChange=()=>()=>{};"}[args.path],loader:"js"}));}}]});
 const m=new Module(path.resolve("frontend/labels_test.js"),module);m._compile(bundle.outputFiles[0].text,path.resolve("frontend/labels_test.js"));
 const f=JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/text_boxes.json"));
 const label=f.axes[0].series.find(s=>s.kind==="text");Object.assign(label,{role:"contour_label",contour_group:0,rotation:35,lines:["1.25"],font_size:12});
 assert(validate(f).ok,JSON.stringify(validate(f)));
 for(const mutate of [s=>s.contour_group=-1,s=>s.position=[1,2,3],s=>s.units="normalized"]){const bad=structuredClone(f);mutate(bad.axes[0].series[label.id]);assert(!validate(bad).ok);}
 const calls=[],ctx=new Proxy({translate:(x,y)=>calls.push(["translate",x,y]),rotate:r=>calls.push(["rotate",r])},{get:(o,k)=>o[k]??(()=>{})});
 const proto=registry.InteractiveFigure.prototype;
 const view=Object.assign(Object.create(proto),{ctx,data:f,initial:[{x:[0,4],y:[0,4]}],palette:{background:"#050719",text:"#e0e0e0"},
 box:()=>({x:0,y:0,w:400,h:400}),dataPixel:v=>v*100,measureRuns:runs=>runs.reduce((n,r)=>n+r.text.length*6,0),
 drawRuns:(runs,x,y,size)=>calls.push(["text",runs.map(r=>r.text).join(""),size]),texts:[]});
 view.drawText(label,0,label.id);
 assert(calls.some(c=>c[0]==="rotate"&&c[1]===-35*Math.PI/180));
 assert.deepEqual(calls.find(c=>c[0]==="translate"),["translate",150,250]);
 assert.deepEqual(calls.find(c=>c[0]==="text"),["text","1.25",12]);
 assert.equal(view.texts[0].rotation,35);assert.equal(view.texts[0].text,"1.25");
 const toolSource=fs.readFileSync("frontend/figure_tools.js","utf8");
 const reason=new Function("t",toolSource.slice(toolSource.indexOf("export function figureReasonText"),toolSource.indexOf("export function figureReductionText")).replace("export function","return function"))(x=>x);
 assert.match(reason("contour_labels",{property:"3d_text_depth"}),/3D depth/);
 assert.match(reason("contour_filled",{detail:"background_hole"}),/background holes/);
 assert.match(reason("contour_filled",{property:"faces"}),/unsupported filled contour geometry/);
 console.log("PASS clabel: guarded 2D schema, existing rotated text path preserves position/string/font/rotation, specific PNG reasons");
})().catch(error=>{console.error(error);process.exit(1);});
