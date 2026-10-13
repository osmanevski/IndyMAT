"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
const { constant } = require("./figure_dalga26_support.cjs");
const { validate } = require("../frontend/figure_data_utils.cjs");
const { contourRingGeometry } = require("../frontend/figure_contour_ring_utils.cjs");
const C = require("../frontend/figure_camera_utils.cjs");
const { pick } = require("../frontend/figure_pick_utils.cjs");
const raw = require("./fixtures/contourf_round3_native.json");
const { native, prepared } = require("./figure_contour_round3_support.cjs");
assert.equal(raw.cases.length,4);
const counts = [402,287,22,6];
for (const [i,c] of raw.cases.entries()) {
  const before=JSON.stringify(c.record);
  const state=prepared(c.record.patches_in_draw_order.map(native));
  assert.equal(state.f.triangle_count,counts[i],c.name);
  assert.equal(state.scene.counts.triangles,counts[i],c.name);
  for(const s of state.a.series) assert.equal(contourRingGeometry(s).triangles.length,s.triangle_count);
  assert.equal(JSON.stringify(c.record),before,"raw measured coordinates remain untouched");
  console.log("PASS raw Round3 contour " + c.name);
}
const ramp=raw.cases[3].record.patches_in_draw_order.map(native);
const noarea=ramp.filter(s=>s.no_area);
assert(noarea.length>0,"the recorded exact ramp includes native no-area boundaries");
for(const s of noarea){const topology=contourRingGeometry(s);assert.equal(topology.triangles.length,0);assert(topology.edges.length>0);}
const hole=native({vertices:[[0,0],[1,0],[1,1],[0,1]],faces:[1,2,3,4],zdata:[],facevertexcdata:null,facecolor:[1,1,1],edgecolor:[0,0,0]});
const holeState=prepared([hole]),vp={x:0,y:0,width:400,height:400};
for(const p of [[.5,.5,0],[0,0,0]]){const screen=C.project(holeState.camera,vp,C.dataToNormalised(holeState.a,p));assert.equal(pick(holeState.scene,holeState.camera,vp,screen.x,screen.y),null,"background holes occlude without inventing level tips");}
const circle = n => native({vertices:Array.from({length:n},(_,i)=>[Math.cos(i*2*Math.PI/n),Math.sin(i*2*Math.PI/n)]),faces:Array.from({length:n},(_,i)=>i+1),zdata:[],facevertexcdata:1,facecolor:[1,0,0],edgecolor:"none"});
const large=prepared([circle(1000)]);assert.equal(large.scene.counts.triangles,998,"native rings exceed ordinary patch's 64-corner cap");
// At 50k circle corners the normalized turn lies below EPS: conservative
// geometry/work-budget fallback is acceptable, an argument-spread RangeError is not.
const overflow=circle(50000);let result;
assert.doesNotThrow(()=>{result=contourRingGeometry(overflow);});
assert(["","contour_filled","budget_exceeded"].includes(result.code));
if(result.code==="budget_exceeded")assert(["contour_operations","contour_time_ms"].includes(result.args.budget));
const malformed=structuredClone(holeState.f);malformed.axes[0].series[0].background_hole=false;assert.equal(validate(malformed).ok,false);
// Renderer submission check: use actual no-area raw coordinates but visible
// constant edges as an explicit harness override (raw ramp has edge='none').
const boundary=structuredClone(noarea.find(s=>s.faces.values.length>1));assert(boundary);
boundary.id=0;boundary.edge_color=constant([0,0,0]);
const boundaryState=prepared([boundary]);
const filename=path.resolve("frontend/figure_webgl.js");
const bundle=esbuild.buildSync({entryPoints:[filename],bundle:true,write:false,platform:"node",format:"cjs"});
const m=new Module(filename,module);m._compile(bundle.outputFiles[0].text,filename);
let serial=0,draws=0;
const gl=new Proxy({getShaderParameter:()=>true,getProgramParameter:()=>true,isContextLost:()=>false},{get:(o,k)=>o[k]??(k.startsWith("create")?()=>++serial:/^[A-Z_]+$/.test(k)?k:()=>{})});
const renderer=new m.exports.FigureWebGL({width:400,height:400,getContext:()=>gl,addEventListener(){},removeEventListener(){}},[boundaryState.scene],()=>assert.fail());
renderer.pixels=positions=>{if(positions.length)draws++;};
renderer.render([{scene:boundaryState.scene,camera:boundaryState.camera,viewport:vp}],{background:[0,0,0],text:[1,1,1],axis:[.5,.5,.5],grid:[.2,.2,.2]},1);
assert(draws>0,"no-area visible edges submit screen triangles without a mesh");renderer.dispose();
const fill=native({vertices:[[0,0],[2,0],[2,1],[0,1]],faces:[1,2,3,4],zdata:[],facevertexcdata:0,facecolor:[1,0,0],edgecolor:"none"});
boundary.id=1;
const orderedState=prepared([fill,boundary]),order=[];
gl.drawElements=()=>order.push("face");
const ordered=new m.exports.FigureWebGL({width:400,height:400,getContext:()=>gl,addEventListener(){},removeEventListener(){}},[orderedState.scene],()=>assert.fail());
ordered.pixels=positions=>{if(positions.length)order.push("boundary");};
ordered.render([{scene:orderedState.scene,camera:orderedState.camera,viewport:vp}],{background:[0,0,0],text:[1,1,1],axis:[.5,.5,.5],grid:[.2,.2,.2]},1);
ordered.dispose();
assert.deepEqual(order,["face","boundary"],"later native no-area children preserve source fill/border overpaint order");
console.log("PASS native no-area, background-hole tips, >64 rings and 50k spread safety");
