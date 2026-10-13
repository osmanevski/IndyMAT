"use strict";
const assert = require("node:assert/strict");
const path = require("node:path"), Module = require("node:module"), esbuild = require("esbuild");
const { contours } = require("./figure_dalga26_support.cjs");
const { validate } = require("../frontend/figure_data_utils.cjs");
const geometry = require("../frontend/figure_geometry_utils.cjs");
const colors = require("../frontend/figure_color_utils.cjs");
const camera = require("../frontend/figure_camera_utils.cjs");
const { pick } = require("../frontend/figure_pick_utils.cjs");
const { tipText } = require("../frontend/figure_tool_utils.cjs");
const f = contours(), before = structuredClone(f);
assert(validate(f).ok, JSON.stringify(validate(f)));
for (const mutation of [s => s.level = null, s => s.level += 1, s => s.contour_group = -1, s => s.face_color = {mode:"none",association:"constant"}]) {
  const bad = structuredClone(f); mutation(bad.axes[0].series[0]); assert(!validate(bad).ok);
}
const scene = geometry.buildScene(f.axes[0]), cam = camera.createCamera(f.axes[0]), vp = {x:0,y:0,width:400,height:400};
assert.deepEqual(f,before);
assert.deepEqual([...scene.meshes[0].colors.slice(0,3)],[1,0,0]);
assert.deepEqual([...scene.meshes[1].colors.slice(0,3)],[0,0,1]);
assert.equal(colors.contourDisplayColors(f.axes[0],[1,1,1],[0,0,0]).size,0,"filled regions do not enter line contour contrast mapping");
const p = camera.project(cam,vp,camera.dataToNormalised(f.axes[0],[.5,.5,1]));
const hit = pick(scene,cam,vp,p.x,p.y);
assert.equal(hit.series,1,"later equal-depth region wins like LEQUAL source draw order");
assert.equal(hit.level,2);assert.equal(hit.colorValue,2);
assert([1/6,5/6].includes(hit.data.x));assert([1/6,5/6].includes(hit.data.y));
assert.match(tipText({...hit.data,level:hit.level}),/Level: 2/);
const filename = path.resolve("frontend/figure_webgl.js");
const bundle = esbuild.buildSync({entryPoints:[filename],bundle:true,write:false,platform:"node",format:"cjs"});
const m = new Module(filename,module); m._compile(bundle.outputFiles[0].text,filename);
const draws = []; let offset = false, depthWrite = true, serial = 0;
const gl = new Proxy({getShaderParameter:()=>true,getProgramParameter:()=>true,isContextLost:()=>false,
  enable:k=>{if(k==="POLYGON_OFFSET_FILL")offset=true;},disable:k=>{if(k==="POLYGON_OFFSET_FILL")offset=false;},
  depthMask:value=>depthWrite=value,
  drawElements:()=>draws.push(["face",offset,depthWrite])},
  {get:(o,k)=>o[k]??(k.startsWith("create")?()=>++serial:/^[A-Z_]+$/.test(k)?k:()=>{})});
const renderer = new m.exports.FigureWebGL({getContext:()=>gl,addEventListener(){},removeEventListener(){}},[scene],()=>assert.fail());
renderer.pixels=()=>draws.push(["edge",offset,depthWrite]);
renderer.render([{scene,camera:cam,viewport:vp}],{background:[0,0,0],text:[1,1,1],axis:[.5,.5,.5],grid:[.2,.2,.2]},1);
assert.deepEqual(draws,[["face",true,true],["edge",false,false],["face",true,true],["edge",false,false]]);
assert.equal(depthWrite,true,"contour border pass restores depth writes");
renderer.dispose();
console.log("PASS contourf: guarded schema, source RGB/XY/level, coplanar picking and fill-border draw order");
