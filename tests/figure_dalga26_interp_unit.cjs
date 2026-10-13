"use strict";
const assert = require("node:assert/strict");
const path = require("node:path"), Module = require("node:module"), esbuild = require("esbuild");
const {interpolatedPatch,interpolatedSurface,descriptor} = require("./figure_dalga26_support.cjs");
const {validate} = require("../frontend/figure_data_utils.cjs");
const geometry = require("../frontend/figure_geometry_utils.cjs"), color = require("../frontend/figure_color_utils.cjs");
const camera = require("../frontend/figure_camera_utils.cjs"), {pick} = require("../frontend/figure_pick_utils.cjs");
const assertValid = f => assert(validate(f).ok,JSON.stringify(validate(f)));
const f = interpolatedPatch(), s = f.axes[0].series[0]; assertValid(f);
const scene = geometry.buildScene(f.axes[0]);
assert.equal(scene.meshes[0].colorInterpolation,"vertex-rgb");assert.equal(scene.edges[0].colorInterpolation,"vertex-rgb");
const expected = [[1,0,0],[0,1,0],[0,0,1]];
for (const object of [scene.meshes[0],scene.edges[0]]) for(let i=0;i<object.source.vertexIndices.length;i++) {
  assert.deepEqual([...object.colors.slice(i*3,i*3+3)],expected[object.source.vertexIndices[i]]);
}
const cam = camera.createCamera(f.axes[0]), vp = {x:0,y:0,width:400,height:400};
const p = camera.project(cam,vp,camera.dataToNormalised(f.axes[0],[.05,.05,1]));
const hit = pick(scene,cam,vp,p.x,p.y);assert.deepEqual(hit.colorValue,[255,0,0]);assert.deepEqual(hit.data,{x:0,y:0,z:1});
for (const mapping of ["scaled","direct"]) for (const cdata_class of ["double","single","uint8","uint16","int16"]) {
  const g = interpolatedPatch(), patch = g.axes[0].series[0];
  patch.face_color = {mode:"interp",association:"vertex",encoding:"indexed",mapping,cdata_class,data:descriptor([3,1],[1,2,1])};
  assertValid(g);assert.deepEqual(color.patchColor(patch.face_color,1,g.axes[0]),[0,0,1]);
}
for (const mutate of [x=>x.face_color.association="face",x=>x.face_color.data.shape=[1,1,3],x=>x.face_color.data.values[0]=null,
  x=>x.face_color.data.values[0]=256,x=>x.face_color.cdata_class="int8",x=>x.edge_color.data.values[0]=NaN]) {
  const g=interpolatedPatch();mutate(g.axes[0].series[0]);assert(!validate(g).ok);
}
// Exactly one vertex colour required per vertex, even when values repeat.
const bad=interpolatedPatch();bad.axes[0].series[0].face_color.data=descriptor([1,1,3],[1,0,0]);assert(!validate(bad).ok);
const sf=interpolatedSurface();assertValid(sf);const ss=sf.axes[0].series[0],sc=geometry.buildScene(sf.axes[0]);
for(let i=0;i<sc.edges[0].source.vertexIndices.length;i++) {
  const vertex=sc.edges[0].source.vertexIndices[i];
  const rgb=color.descriptorColor(ss.edge_color,vertex,sf.axes[0]);
  assert.deepEqual([...sc.edges[0].colors.slice(i*3,i*3+3)],[...new Float32Array(rgb)]);
}
// A nonfinite second endpoint cannot slip through the old first-owner guard.
const missing=interpolatedSurface(), ms=missing.axes[0].series[0];
const last=ms.cdata.values.length-1;ms.cdata.values[last]=null;ms.face_color.data.values[last]=null;ms.edge_color.data.values[last]=null;
assert(!validate(missing).ok);
const wire=interpolatedSurface();wire.axes[0].series[0].face_color={mode:"none",association:"constant"};assertValid(wire);
const wa=wire.axes[0],ws=geometry.buildScene(wa),wc=camera.createCamera(wa),wp=geometry.surfacePoint(wa.series[0],0);
const pix=camera.project(wc,vp,camera.dataToNormalised(wa,wp)),wh=pick(ws,wc,vp,pix.x,pix.y);
assert(wh);assert.equal(wh.kind,"surface");assert.deepEqual(wh.data,{x:wp[0],y:wp[1],z:wp[2]});
// Exercise actual dash/viewport clipping expansion through the real renderer.
const filename=path.resolve("frontend/figure_webgl.js"), bundle=esbuild.buildSync({entryPoints:[filename],bundle:true,write:false,platform:"node",format:"cjs"});
const m=new Module(filename,module);m._compile(bundle.outputFiles[0].text,filename);let serial=0;
const gl=new Proxy({getShaderParameter:()=>true,getProgramParameter:()=>true,isContextLost:()=>false},
 {get:(o,k)=>o[k]??(k.startsWith("create")?()=>++serial:/^[A-Z_]+$/.test(k)?k:()=>{})});
for(const style of ["-","--",":","-."]) for (const zoom of [1, 2]) {
  const ff=interpolatedPatch();ff.axes[0].series[0].line_style=style;
  const axes=ff.axes[0],scene=geometry.buildScene(axes),cam=camera.createCamera(axes);
  camera.zoom(cam,zoom);
  const renderer=new m.exports.FigureWebGL({getContext:()=>gl,addEventListener(){},removeEventListener(){}},[scene],()=>assert.fail());
  const batches=[];renderer.pixels=(positions,colors)=>batches.push({positions,colors});
  renderer.render([{scene,camera:cam,viewport:vp}],{background:[0,0,0],text:[1,1,1],axis:[.5,.5,.5],grid:[.2,.2,.2]},1);
  assert.equal(batches.length,1);const b=batches[0];assert.equal(b.positions.length,b.colors.length);
  assert(b.colors.every(Number.isFinite));
  // Every expanded triangle still carries RGB in the source convex hull.
  for(let i=0;i<b.colors.length;i+=3)assert(Math.abs(b.colors[i]+b.colors[i+1]+b.colors[i+2]-1)<1e-6);
  assert(b.colors.some(v=>v>0&&v<1)||style==="-","dashes interpolate at actual clipped segment positions");
  // First source edge goes red -> green, so the first quad's RGB cannot be flat.
  assert.notDeepEqual([...b.colors.slice(0,3)],[...b.colors.slice(6,9)]);
  // Colour at the expanded segment centre follows the original projected
  // endpoints, including portions clipped away at zoom 2 (not a new gradient).
  const a=camera.project(cam,vp,camera.dataToNormalised(axes,[0,0,1]));
  const z=camera.project(cam,vp,camera.dataToNormalised(axes,[1,0,1]));
  const cx=(b.positions[0]+b.positions[3])/2,cy=(b.positions[1]+b.positions[4])/2;
  const t=((cx-a.x)*(z.x-a.x)+(cy-a.y)*(z.y-a.y))/((z.x-a.x)**2+(z.y-a.y)**2);
  assert(Math.abs(b.colors[0]-(1-t))<1e-5);assert(Math.abs(b.colors[1]-t)<1e-5);
  renderer.dispose();
}
console.log("PASS patch face/edge and surface edge vertex RGB: schema, class mapping, original-value tips, dash interpolation");
