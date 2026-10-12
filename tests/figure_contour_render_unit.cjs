"use strict";
const assert = require("node:assert/strict");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");

(async () => {
  const registry = {};
  global.figureViewHarness = { registry };
  const build = await esbuild.build({ stdin: { contents: 'import "./figures.js";', resolveDir: path.resolve("frontend") }, bundle: true, write: false, platform: "node", format: "cjs", plugins: [{ name: "boundaries", setup(build) {
    build.onResolve({ filter: /^\.\/(state|registry|figure3d|figure_tools|figure_window_controls|i18n)\.js$/ }, (args) => ({ path: path.basename(args.path), namespace: "stub" }));
    build.onLoad({ filter: /.*/, namespace: "stub" }, (args) => ({ contents: {
      "state.js": "export default {};",
      "registry.js": "export default globalThis.figureViewHarness.registry;",
      "figure3d.js": "export class Figure3D {}; export function drawColorbars() {}",
      "figure_tools.js": "export const figureReasonText=()=>'';export const figureReductionText=()=>'';export function unmountFigureTools(){}",
      "figure_window_controls.js": "export const mountFigureWindowControls=()=>()=>{};",
      "i18n.js": "export const t=x=>x; export const onLanguageChange=()=>()=>{};"
    }[args.path], loader: "js" }));
  } }] });
  const loaded = new Module(path.resolve("frontend/view_test.js"), module);
  loaded._compile(build.outputFiles[0].text, path.resolve("frontend/view_test.js"));
  const proto = registry.InteractiveFigure.prototype;
  const fs=require("node:fs"), colors=require("../frontend/figure_color_utils.cjs");
  const geometry=require("../frontend/figure_geometry_utils.cjs"),camera=require("../frontend/figure_camera_utils.cjs");
  const validate=require("../frontend/figure_data_utils.cjs").validate;
  const a=JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/surf_vector.json")).axes[0];
  a.visible=false;a.xlim=a.ylim=[0,1];a.zlim=[0,3];a.clim=[1,2];
  a.colormap={shape:[2,3],order:"column-major",values:[.05,.8,0,.9,.1,.2]};
  a.series=[1,2].map((level,id)=>({id,kind:"line",role:"contour",contour_group:0,
   level,contour_closed:false,x:[.2,.8],y:[level/3,level/3],z:[level,level],source_indices:[0,1],
   line_color:colors.mapColor(a.colormap,id),marker:"none",marker_size:0,marker_edge_color:"none",
   marker_face_color:"none",marker_face_auto:false,line_style:"-",line_width:3,display_name:"",decimated:false,original_points:2,rendered_points:2,
   contour_color:{mode:"flat",association:"level",encoding:"indexed",mapping:"direct",cdata_class:"double",data:{shape:[1,1],order:"column-major",values:[level]}}}));
  const f=JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/surf_vector.json"));f.axes=[a];f.vertex_count=4;f.triangle_count=0;
  assert(validate(f).ok,JSON.stringify(validate(f)));
  a.series[0].contour_group=-1;assert(!validate(f).ok);a.series[0].contour_group=0;
  const palette={background:[5,7,25].map(v=>v/255),text:[224,224,224].map(v=>v/255),axis:[.1,.1,.1],grid:[.1,.1,.1]};
  const expected=colors.contourDisplayColors(a,palette.background,palette.text);
  const strokes=[];
  const ctx=new Proxy({measureText:()=>({width:10})},{get:(o,k)=>k==="stroke"?()=>strokes.push(o.strokeStyle):o[k]??(()=>{})});
  const plot=Object.assign(Object.create(proto),{palette:{background:"#050719",text:"#e0e0e0"},
    ctx,data:{axes:[a]},box:()=>({x:0,y:0,w:200,h:200}),dataPixel:v=>v*100,
    label(){},drawLegend(){},drawMarker(){},hits:[],faces:[],texts:[],drawText(){},views:[{x:[0,1],y:[0,1]}]});
  // drawAxis creates the group palette; drawSeries and seriesStyle are real.
  plot.drawAxis(a,0);
  for(const s of a.series)assert.equal(plot.seriesStyle(s).line,`rgb(${expected.get(s).map(v=>v*255).join(",")})`);
  assert(strokes.includes(plot.seriesStyle(a.series[0]).line));
  const manual={...a.series[0],contour_color:{mode:"constant",association:"constant",rgb:[.05,0,.1]}};
  assert.equal(plot.seriesStyle(manual).line,"#e0e0e0","explicit contour RGB keeps old policy");
  const filename=path.resolve("frontend/figure_webgl.js");
  const bundle=esbuild.buildSync({entryPoints:[filename],bundle:true,write:false,platform:"node",format:"cjs"});
  const m=new Module(filename,module);m._compile(bundle.outputFiles[0].text,filename);
  let serial=0;
  const gl=new Proxy({getShaderParameter:()=>true,getProgramParameter:()=>true,isContextLost:()=>false},
    {get:(o,k)=>o[k]??(k.startsWith("create")?()=>++serial:/^[A-Z_]+$/.test(k)?k:()=>{})});
  const scene=geometry.buildScene(a),renderer=new m.exports.FigureWebGL({getContext:()=>gl,addEventListener(){},removeEventListener(){}},[scene],()=>assert.fail());
  const ink=[];renderer.solid=(_,rgb)=>ink.push(Array.from(rgb));
  renderer.render([{scene,camera:camera.createCamera(a),viewport:{x:0,y:0,width:400,height:300}}],palette,1);
  assert.deepEqual(ink,a.series.map(s=>expected.get(s)));
  renderer.dispose();
  console.log("PASS real 2D drawAxis/style and WebGL line dispatch; explicit colours unchanged; group ID validation");
})().catch(error=>{console.error(error);process.exit(1);});
